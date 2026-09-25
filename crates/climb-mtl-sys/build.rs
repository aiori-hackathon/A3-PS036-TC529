//! Build script for `climb-mtl-sys`.
//!
//! Generates raw `bindgen` FFI bindings to **libMTL** (`mtlslib`) and emits the
//! linker flags needed to link the vendored, pinned C toolchain.
//!
//! # Header discovery (Phase 2.1 finding)
//!
//! The public C API behind the `mtlkeygen` / `mtlsign` / `mtlverify` CLI tools
//! lives in `src/mtllib.h` (installed as `include/mtllib/mtllib.h`). It pulls in
//! `mtl.h`, `mtllib_buffer.h` and `mtllib_status.h` from the same directory, and
//! `<oqs/sig.h>` from the prefix include root. The CLI argument-parsing code is
//! in `examples/mtl*.c` and is deliberately **not** bound.
//!
//! # Prefix resolution — env var, not a Cargo feature
//!
//! We chose the `CLIMB_MTL_PREFIX` environment variable (with an optional
//! `CLIMB_MTL_INCLUDE` / `CLIMB_MTL_LIB` override) instead of a Cargo feature
//! because:
//!
//! * a single build can be pointed at *any* prefix (Docker `/out`, a local
//!   `make install` tree, or `/usr/local`) without recompiling feature-gated
//!   code paths or editing `Cargo.toml`;
//! * it keeps one code path, so there is no risk of the two feature variants
//!   drifting; and
//! * the value is naturally supplied by CI / `xtask` without touching the
//!   dependency graph.
//!
//! Resolution order:
//! 1. `CLIMB_MTL_PREFIX` (uses `$PREFIX/include` and `$PREFIX/lib`);
//! 2. `<workspace>/.mtl-install` (produced by `docker/extract-libs.sh`);
//! 3. `/out` (the Dockerfile `final` stage extraction point);
//! 4. `/usr/local`.
//!
//! `CLIMB_MTL_INCLUDE` and `CLIMB_MTL_LIB` override the derived include/lib
//! directories individually.

use std::env;
use std::path::{Path, PathBuf};

/// Candidate locations for the MTL/oqs/OpenSSL install prefix, in priority order.
fn candidate_prefixes(workspace_root: &Path) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(prefix) = env::var("CLIMB_MTL_PREFIX") {
        candidates.push(PathBuf::from(prefix));
    }
    candidates.push(workspace_root.join(".mtl-install"));
    candidates.push(PathBuf::from("/out"));
    candidates.push(PathBuf::from("/usr/local"));
    candidates
}

/// Locate the public API header inside an include directory.
///
/// Accepts either the installed layout (`<include>/mtllib/mtllib.h`) or a raw
/// source checkout (`<include>/mtllib.h`).
fn find_mtllib_header(include_dir: &Path) -> Option<PathBuf> {
    let installed = include_dir.join("mtllib").join("mtllib.h");
    if installed.is_file() {
        return Some(installed);
    }
    let raw = include_dir.join("mtllib.h");
    if raw.is_file() {
        return Some(raw);
    }
    None
}

/// Pick a usable prefix, failing with an actionable message if none exists.
///
/// An explicitly-set `CLIMB_MTL_PREFIX` is authoritative: if it does not
/// resolve, the build fails rather than silently falling back to a different
/// prefix, so a typo cannot produce a binary linked against unexpected
/// libraries.
fn resolve_prefix(workspace_root: &Path) -> (PathBuf, PathBuf, PathBuf) {
    // Individual overrides win over the derived prefix.
    let include_override = env::var("CLIMB_MTL_INCLUDE").ok().map(PathBuf::from);
    let lib_override = env::var("CLIMB_MTL_LIB").ok().map(PathBuf::from);
    let explicit = env::var("CLIMB_MTL_PREFIX").ok().map(PathBuf::from);

    let candidates = match &explicit {
        Some(prefix) => vec![prefix.clone()],
        None => candidate_prefixes(workspace_root),
    };

    for prefix in candidates {
        let include_dir = include_override
            .clone()
            .unwrap_or_else(|| prefix.join("include"));
        let lib_dir = lib_override.clone().unwrap_or_else(|| prefix.join("lib"));

        if !include_dir.is_dir() || !lib_dir.is_dir() {
            continue;
        }
        if find_mtllib_header(&include_dir).is_some() {
            return (prefix, include_dir, lib_dir);
        }
    }

    let checked = if explicit.is_some() {
        "CLIMB_MTL_PREFIX was set explicitly, so no fallback was attempted.".to_string()
    } else {
        format!(
            "Checked: {}",
            candidate_prefixes(workspace_root)
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )
    };

    panic!(
        "climb-mtl-sys: could not locate a libMTL install prefix.\n\
         Set CLIMB_MTL_PREFIX to a prefix containing include/mtllib/mtllib.h and lib/libmtlslib.so,\n\
         or run `docker/extract-libs.sh` to populate <workspace>/.mtl-install.\n{checked}"
    );
}

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir
        .parent()
        .and_then(Path::parent)
        .expect("crate is nested under <workspace>/crates/<crate>")
        .to_path_buf();

    for var in ["CLIMB_MTL_PREFIX", "CLIMB_MTL_INCLUDE", "CLIMB_MTL_LIB"] {
        println!("cargo:rerun-if-env-changed={var}");
    }

    let (prefix, include_dir, lib_dir) = resolve_prefix(&workspace_root);
    let header = find_mtllib_header(&include_dir).expect("header checked by resolve_prefix");

    println!("cargo:rerun-if-changed={}", header.display());
    println!("cargo:rerun-if-changed={}", include_dir.display());
    println!("cargo:rerun-if-changed={}", lib_dir.display());

    // Directory that holds mtllib.h and its sibling headers (relative includes).
    let mtllib_include_dir = header
        .parent()
        .expect("header always has a parent directory")
        .to_path_buf();

    // ── Linker configuration ────────────────────────────────────────────────
    //
    // DISCOVERED DEFECT (Phase 2): the shipped `libmtlslib.so` does NOT record
    // its liboqs dependency in DT_NEEDED — it carries 7 undefined `OQS_*`
    // symbols, and its `.la` dependency_libs omits `-loqs`. rustc links with
    // `-Wl,--as-needed`, and ld drops a shared library whose only undefined
    // references originate from *another shared library* (they are not "real"
    // undefined symbols for `--as-needed` purposes). The result is a binary
    // whose DT_NEEDED lacks liboqs, which then fails at runtime with
    // `undefined symbol: OQS_SIG_new`.
    //
    // Fix: link the *static* `libmtlslib.a` (shipped alongside the .so by the
    // same libtool build). Its object files produce ordinary undefined symbols,
    // so `-loqs` survives `--as-needed`. Fall back to the shared library if the
    // archive is unavailable (in which case the caller must ensure liboqs is
    // retained, e.g. via `-Wl,--no-as-needed`).
    println!("cargo:rustc-link-search=native={}", lib_dir.display());
    let static_mtlslib = lib_dir.join("libmtlslib.a").is_file();
    if static_mtlslib {
        println!("cargo:rustc-link-lib=static=mtlslib");
    } else {
        println!("cargo:rustc-link-lib=mtlslib");
        println!("cargo:rustc-link-arg=-Wl,--no-as-needed");
    }
    // liboqs ≥0.14 (dynamic) and the OpenSSL libcrypto + libm required by
    // libMTL per its README. libssl is not needed by the MTL signing path.
    for lib in ["oqs", "crypto", "m"] {
        println!("cargo:rustc-link-lib={lib}");
    }

    // Make the example/test binaries in this crate runnable without setting
    // LD_LIBRARY_PATH, by baking the prefix lib dir into the rpath. This does
    // NOT propagate to dependent crates (verified in Phase 3) — dependent
    // crates must consume `DEP_MTLSLIB_LIB_DIR` below and emit their own rpath.
    println!("cargo:rustc-link-arg=-Wl,-rpath,{}", lib_dir.display());

    // Expose the resolved lib directory to dependent crates. Because this crate
    // declares `links = "mtlslib"`, Cargo passes this value to a dependent
    // crate's build script as `DEP_MTLSLIB_LIB_DIR`. `climb-mtl` uses it to add
    // an rpath so its integration tests can load liboqs/libcrypto at runtime
    // without a system-wide install or LD_LIBRARY_PATH.
    println!("cargo::metadata=lib_dir={}", lib_dir.display());

    // ── bindgen ─────────────────────────────────────────────────────────────
    let bindings = bindgen::Builder::default()
        .header(header.to_string_lossy())
        // Prefix include root: resolves <oqs/sig.h> and <openssl/evp.h>.
        .clang_arg(format!("-I{}", include_dir.display()))
        // Header-local directory: resolves "mtl.h", "mtllib_buffer.h", etc.
        .clang_arg(format!("-I{}", mtllib_include_dir.display()))
        // Bind the library entry points only — not the CLI argument parsing.
        .allowlist_function("^mtllib_.*")
        .allowlist_function("^mtl_.*")
        .allowlist_type(
            "^(MTL|MTLLIB|SERIESID|SEED|AUTHPATH|RANDOMIZER|RUNG|LADDER|H_LEAF|H_INT|OQS_).*",
        )
        // CLI help helper takes a FILE*; not part of the crypto API surface.
        .blocklist_function("mtllib_key_write_algorithms")
        .blocklist_item("FILE")
        // Keep generated code deterministic and free of layout assertions on
        // opaque FFI types.
        .layout_tests(false)
        .derive_default(false)
        .generate_comments(true)
        .generate()
        .expect("bindgen failed to generate libMTL bindings");

    let out_path = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR")).join("bindings.rs");
    bindings
        .write_to_file(&out_path)
        .expect("failed to write bindings.rs");

    println!(
        "cargo:warning=climb-mtl-sys: generated bindings from {} (prefix {})",
        header.display(),
        prefix.display()
    );
}
