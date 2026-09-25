//! Build script for `climb-mtl`.
//!
//! `climb-mtl-sys` links the native `mtlslib`/`oqs`/`crypto` stack and bakes an
//! rpath into binaries built *inside that crate*. That rpath does not propagate
//! to dependent crates, so integration tests in this crate would fail at runtime
//! with `liboqs.so.9: cannot open shared object file`. `climb-mtl-sys` publishes
//! its resolved lib directory via `cargo::metadata` (because it declares
//! `links = "mtlslib"`), which Cargo exposes here as `DEP_MTLSLIB_LIB_DIR`.

use std::env;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    let lib_dir = env::var("DEP_MTLSLIB_LIB_DIR").unwrap_or_else(|_| {
        panic!(
            "climb-mtl: DEP_MTLSLIB_LIB_DIR was not provided by climb-mtl-sys. \
             A dependent crate's build script only receives `DEP_<links>_*` \
             variables when the dependency declares `links`. Verify that \
             climb-mtl-sys still sets `links = \"mtlslib\"` and prints \
             `cargo::metadata=lib_dir=...`."
        )
    });

    println!("cargo:rustc-link-arg=-Wl,-rpath,{lib_dir}");
}
