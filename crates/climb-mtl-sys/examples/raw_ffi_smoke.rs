//! Raw-FFI end-to-end smoke test for `climb-mtl-sys`.
//!
//! This is deliberately **not** a safe wrapper: it calls the `bindgen`-generated
//! `extern "C"` symbols directly, so a successful run proves the FFI linkage
//! genuinely works (search paths, libraries, symbol resolution, ABI layout) —
//! not merely that the bindings compile.
//!
//! It reproduces the exact call sequence of the upstream CLI tools
//! (`mtlkeygen` → `mtlsign` → `mtlverify`) in-process:
//!
//! 1. Generate a keypair (`mtllib_key_new`).
//! 2. Serialize the public key (`mtllib_pubkey_to_buffer`).
//! 3. Append one short message and produce both a **full** signature and a
//!    **condensed** signature plus the **signed ladder**.
//! 4. Rebuild a *verifier* context from the public key only
//!    (`mtllib_pubkey_from_buffer`) and verify:
//!    - the full signature standalone (`mtllib_verify`, no ladder), and
//!    - the condensed signature against the signed ladder (`mtllib_verify`).
//!
//! Run with:
//! ```sh
//! cargo run -p climb-mtl-sys --example raw_ffi_smoke
//! ```

use std::ffi::CString;
use std::ptr;

use climb_mtl_sys::*;

/// Algorithm string selected in `the design notes` §3: the small-signature ("s")
/// SLH-DSA variant with an SHA2-128 MTL tree. Its `sec_param` is 16, so the
/// Series ID embedded at the front of every signature is 32 bytes.
const ALGORITHM: &str = "SLH-DSA-SHA2-128s-MTL-SHA2-128";

/// Expected Series ID length for the algorithm above (2 × sec_param).
const EXPECTED_SID_LEN: usize = 32;

const MESSAGE: &[u8] = b"CLIMB phase-2 FFI smoke test message";

/// Return `Ok(())` for either success status, otherwise the numeric code.
fn check(status: MTLLIB_STATUS, what: &str) -> Result<(), MTLLIB_STATUS> {
    if status == MTLLIB_STATUS_MTLLIB_OK || status == MTLLIB_STATUS_MTLLIB_OK_VALIDATED_LADDER {
        Ok(())
    } else {
        eprintln!("FAIL: {what} returned MTLLIB status {status}");
        Err(status)
    }
}

/// Allocate an owned internal buffer and append `data` bytes to it.
///
/// # Safety
/// Calls libMTL buffer APIs; `out` must be a valid place to write the pointer.
unsafe fn buffer_from_slice(data: &[u8]) -> Result<*mut MTLLIB_BUFFER, MTLLIB_STATUS> {
    let mut buf: *mut MTLLIB_BUFFER = ptr::null_mut();
    check(
        mtllib_buffer_initialize(&mut buf, data.len(), ptr::null_mut()),
        "mtllib_buffer_initialize",
    )?;
    check(
        mtllib_buffer_append(buf, data.as_ptr().cast_mut(), data.len()),
        "mtllib_buffer_append",
    )?;
    Ok(buf)
}

/// Copy the used bytes out of an `MTLLIB_BUFFER` into a `Vec<u8>`.
///
/// # Safety
/// `buffer` must be a live buffer created by libMTL.
unsafe fn buffer_to_vec(buffer: *mut MTLLIB_BUFFER) -> Vec<u8> {
    let len = mtllib_buffer_in_use(buffer);
    let ptr = mtllib_buffer_data_ptr(buffer);
    assert!(!ptr.is_null() || len == 0, "buffer data pointer is null");
    if len == 0 {
        return Vec::new();
    }
    // SAFETY: libMTL guarantees `buffer_data` points to at least `len` valid
    // bytes for an initialised buffer, and the buffer outlives this copy.
    std::slice::from_raw_parts(ptr, len).to_vec()
}

fn main() {
    // SAFETY: every call below is an FFI call into libMTL. Pointer arguments
    // are either null where the C API documents "optional", or refer to live
    // buffers/contexts allocated by libMTL itself. Buffers are freed exactly
    // once, and the contexts that own them are freed last.
    unsafe {
        let rc = run();
        match rc {
            Ok(()) => {
                println!("\nraw_ffi_smoke: PASS");
            }
            Err(code) => {
                eprintln!("raw_ffi_smoke: FAIL (status {code})");
                std::process::exit(1);
            }
        }
    }
}

/// # Safety
/// Performs raw FFI calls into libMTL; see `main` for the invariants.
unsafe fn run() -> Result<(), MTLLIB_STATUS> {
    println!("== climb-mtl-sys raw FFI smoke test ==");
    println!("algorithm: {ALGORITHM}");
    println!(
        "message:   {:?} ({} bytes)",
        String::from_utf8_lossy(MESSAGE),
        MESSAGE.len()
    );

    // ── 1. Key generation (mtlkeygen) ───────────────────────────────────────
    let alg_cstr = CString::new(ALGORITHM).expect("algorithm string has no NUL");
    let mut signer: *mut MTLLIB_CTX = ptr::null_mut();
    check(
        mtllib_key_new(alg_cstr.as_ptr().cast_mut(), &mut signer),
        "mtllib_key_new",
    )?;
    assert!(!signer.is_null(), "mtllib_key_new returned a null context");
    println!("[1] keypair generated (signer ctx = {signer:p})");

    // ── 2. Serialize the public key ─────────────────────────────────────────
    let pubkey_len = mtllib_pubkey_to_buffer_length(signer);
    assert!(pubkey_len > 0, "public key length must be non-zero");

    let mut pubkey_buf: *mut MTLLIB_BUFFER = ptr::null_mut();
    check(
        mtllib_buffer_initialize(&mut pubkey_buf, pubkey_len, ptr::null_mut()),
        "mtllib_buffer_initialize(pubkey)",
    )?;
    check(
        mtllib_pubkey_to_buffer(signer, pubkey_buf),
        "mtllib_pubkey_to_buffer",
    )?;
    let pubkey = buffer_to_vec(pubkey_buf);
    assert_eq!(pubkey.len(), pubkey_len, "public key length mismatch");
    println!("[2] public key serialized: {pubkey_len} bytes");

    // ── 3a. Append the message and get a full signature (mtlsign) ───────────
    let msg_buf = buffer_from_slice(MESSAGE)?;

    let mut handle: *mut MTL_HANDLE = ptr::null_mut();
    check(
        mtllib_sign_append(signer, msg_buf, &mut handle),
        "mtllib_sign_append",
    )?;
    assert!(!handle.is_null(), "sign append returned a null handle");
    let leaf_index = (*handle).leaf_index;
    println!("[3] message appended at leaf index {leaf_index}");

    let full_sig_len = mtllib_sign_get_full_sig_length(signer, handle);
    assert!(full_sig_len > 0, "full signature length must be non-zero");
    let mut full_sig_buf: *mut MTLLIB_BUFFER = ptr::null_mut();
    check(
        mtllib_buffer_initialize(&mut full_sig_buf, full_sig_len, ptr::null_mut()),
        "mtllib_buffer_initialize(full_sig)",
    )?;
    check(
        mtllib_sign_get_full_sig(signer, handle, full_sig_buf),
        "mtllib_sign_get_full_sig",
    )?;
    let full_sig = buffer_to_vec(full_sig_buf);
    assert_eq!(
        full_sig.len(),
        full_sig_len,
        "full signature length mismatch"
    );
    println!("[3] full signature: {full_sig_len} bytes");

    // ── 3b. Condensed signature + signed ladder ─────────────────────────────
    let condensed_len = mtllib_sign_get_condensed_sig_length(signer, handle);
    assert!(
        condensed_len > 0,
        "condensed signature length must be non-zero"
    );
    let mut condensed_buf: *mut MTLLIB_BUFFER = ptr::null_mut();
    check(
        mtllib_buffer_initialize(&mut condensed_buf, condensed_len, ptr::null_mut()),
        "mtllib_buffer_initialize(condensed)",
    )?;
    check(
        mtllib_sign_get_condensed_sig(signer, handle, condensed_buf),
        "mtllib_sign_get_condensed_sig",
    )?;
    let condensed_sig = buffer_to_vec(condensed_buf);

    let ladder_len = mtllib_sign_get_signed_ladder_length(signer);
    assert!(ladder_len > 0, "signed ladder length must be non-zero");
    let mut ladder_buf: *mut MTLLIB_BUFFER = ptr::null_mut();
    check(
        mtllib_buffer_initialize(&mut ladder_buf, ladder_len, ptr::null_mut()),
        "mtllib_buffer_initialize(ladder)",
    )?;
    check(
        mtllib_sign_get_signed_ladder(signer, ladder_buf),
        "mtllib_sign_get_signed_ladder",
    )?;
    let ladder = buffer_to_vec(ladder_buf);
    println!(
        "[3] condensed signature: {} bytes | signed ladder: {} bytes | full = {} + {} = {}",
        condensed_sig.len(),
        ladder.len(),
        condensed_sig.len(),
        ladder.len(),
        condensed_sig.len() + ladder.len(),
    );

    // The condensed signature and the full signature must describe the same
    // message: `full == condensed || signed_ladder` (see upstream mtlsign.c).
    assert_eq!(
        full_sig.len(),
        condensed_sig.len() + ladder.len(),
        "full signature is not condensed||ladder"
    );
    assert_eq!(
        &full_sig[..condensed_sig.len()],
        condensed_sig.as_slice(),
        "condensed signature is not a prefix of the full signature"
    );

    // ── 4. Rebuild a verifier context from the public key (mtlverify) ───────
    // The Series ID is carried as the leading `2 * sec_param` bytes of any
    // signature (full or condensed); the upstream mtlverify passes exactly that
    // pointer into mtllib_pubkey_from_buffer.
    let sid_len = mtllib_sig_buffer_get_hash_size(alg_cstr.as_ptr().cast_mut()) as usize * 2;
    assert_eq!(sid_len, EXPECTED_SID_LEN, "unexpected SID length");

    let mut verifier: *mut MTLLIB_CTX = ptr::null_mut();
    check(
        mtllib_pubkey_from_buffer(
            alg_cstr.as_ptr().cast_mut(),
            &mut verifier,
            pubkey_buf,
            mtllib_buffer_data_ptr(full_sig_buf),
        ),
        "mtllib_pubkey_from_buffer",
    )?;
    assert!(!verifier.is_null(), "verifier context is null");
    println!("[4] verifier ctx built from public key (sid = {sid_len} bytes)");

    // 4a. Verify the full signature standalone (no cached ladder).
    let full_status = mtllib_verify(
        verifier,
        msg_buf,
        full_sig_buf,
        ptr::null_mut(),
        ptr::null_mut(),
    );
    check(full_status, "mtllib_verify(full)")?;
    println!("[4] full signature verified (status {full_status})");

    // 4b. Verify the condensed signature against the signed ladder.
    let condensed_status = mtllib_verify(
        verifier,
        msg_buf,
        condensed_buf,
        ladder_buf,
        ptr::null_mut(),
    );
    check(condensed_status, "mtllib_verify(condensed+ladder)")?;
    println!("[4] condensed signature verified against ladder (status {condensed_status})");

    // 4c. Negative control: a tampered message must be rejected. This proves we
    // are not merely observing a function that returns success unconditionally.
    eprintln!("[4] negative control: expecting one libMTL LOG_ERROR above (tampered message)");
    let tampered = b"CLIMB phase-2 FFI smoke test messagX"; // one byte different
    assert_eq!(tampered.len(), MESSAGE.len());
    let tampered_buf = buffer_from_slice(tampered)?;
    let tampered_status = mtllib_verify(
        verifier,
        tampered_buf,
        full_sig_buf,
        ptr::null_mut(),
        ptr::null_mut(),
    );
    if tampered_status == MTLLIB_STATUS_MTLLIB_OK
        || tampered_status == MTLLIB_STATUS_MTLLIB_OK_VALIDATED_LADDER
    {
        eprintln!("FAIL: tampered message verified successfully (status {tampered_status})");
        return Err(tampered_status);
    }
    println!("[4] tampered message correctly rejected (status {tampered_status})");

    // ── 5. Cleanup ──────────────────────────────────────────────────────────
    // Order matters only in that contexts own their internal buffers; these
    // buffers are independently allocated and freed here first.
    mtllib_buffer_free(tampered_buf);
    mtllib_sign_free_handle(&mut handle);
    mtllib_buffer_free(msg_buf);
    mtllib_buffer_free(full_sig_buf);
    mtllib_buffer_free(condensed_buf);
    mtllib_buffer_free(ladder_buf);
    mtllib_buffer_free(pubkey_buf);
    mtllib_key_free(verifier);
    mtllib_key_free(signer);
    assert!(
        handle.is_null(),
        "mtllib_sign_free_handle must null the handle"
    );
    println!("[5] all contexts and buffers freed");

    Ok(())
}
