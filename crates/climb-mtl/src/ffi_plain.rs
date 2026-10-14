//! Private RAII guard over the liboqs `OQS_SIG` binding used by the plain
//! (non-MTL) SLH-DSA baseline.
//!
//! Nothing here is public. The guard owns the `OQS_SIG*` returned by
//! `OQS_SIG_new` and frees it with `OQS_SIG_free` exactly once on drop, so no
//! raw pointer escapes into the crate's public API.

use std::ffi::CStr;
use std::ptr::NonNull;

use climb_mtl_sys as sys;

type Status = sys::OQS_STATUS;

const OQS_OK: Status = sys::OQS_STATUS_OQS_SUCCESS;

/// Owned `OQS_SIG*` for one plain SLH-DSA scheme.
pub(crate) struct Scheme(NonNull<sys::OQS_SIG>);

impl Scheme {
    /// Look up the compiled-in scheme named by `algorithm`, or `None` when
    /// liboqs does not provide it.
    pub(crate) fn new(algorithm: &CStr) -> Option<Self> {
        // SAFETY: `algorithm` is a valid NUL-terminated C string. `OQS_SIG_new`
        // returns a heap allocation owned by the caller (null when the name is
        // unknown); on success ownership transfers to this guard.
        let raw = unsafe { sys::OQS_SIG_new(algorithm.as_ptr()) };
        NonNull::new(raw).map(Scheme)
    }

    /// The byte lengths liboqs reports for this scheme, never hardcoded.
    pub(crate) fn lengths(&self) -> Lengths {
        // SAFETY: `self.0` is a live scheme returned by `OQS_SIG_new`.
        let sig = unsafe { self.0.as_ref() };
        Lengths {
            public_key: sig.length_public_key,
            secret_key: sig.length_secret_key,
            signature: sig.length_signature,
        }
    }

    /// Generate a keypair, returning `(public_key, secret_key)` on success.
    pub(crate) fn keypair(&self) -> Result<(Vec<u8>, Vec<u8>), Status> {
        let lengths = self.lengths();
        let mut public_key = vec![0u8; lengths.public_key];
        let mut secret_key = vec![0u8; lengths.secret_key];
        // SAFETY: `self.0` is live and both vectors are allocated with the exact
        // lengths liboqs documents for this scheme, so the callee writes only
        // inside them.
        let status = unsafe {
            sys::OQS_SIG_keypair(
                self.0.as_ptr(),
                public_key.as_mut_ptr(),
                secret_key.as_mut_ptr(),
            )
        };
        if status != OQS_OK {
            return Err(status);
        }
        Ok((public_key, secret_key))
    }

    /// Sign `message` with `secret_key`, returning exactly the signature bytes.
    pub(crate) fn sign(&self, message: &[u8], secret_key: &[u8]) -> Result<Vec<u8>, Status> {
        let max = self.lengths().signature;
        let mut signature = vec![0u8; max];
        let mut signature_len = 0usize;
        // SAFETY: `self.0` is live; `signature` holds `max` writable bytes (the
        // scheme's documented maximum); `message` and `secret_key` are live for
        // the call. liboqs writes at most `max` bytes and reports the true
        // length through `signature_len`.
        let status = unsafe {
            sys::OQS_SIG_sign(
                self.0.as_ptr(),
                signature.as_mut_ptr(),
                &mut signature_len,
                message.as_ptr(),
                message.len(),
                secret_key.as_ptr(),
            )
        };
        if status != OQS_OK {
            return Err(status);
        }
        if signature_len > max {
            return Err(sys::OQS_STATUS_OQS_ERROR);
        }
        signature.truncate(signature_len);
        Ok(signature)
    }

    /// Verify `signature` over `message` with `public_key`.
    ///
    /// A cryptographic mismatch and a malformed input both surface as liboqs's
    /// `OQS_ERROR`, which is an expected outcome rather than a failure; any
    /// other status is returned as an error.
    pub(crate) fn verify(
        &self,
        message: &[u8],
        signature: &[u8],
        public_key: &[u8],
    ) -> Result<bool, Status> {
        // SAFETY: `self.0` is live and all three slices are live for the call.
        let status = unsafe {
            sys::OQS_SIG_verify(
                self.0.as_ptr(),
                message.as_ptr(),
                message.len(),
                signature.as_ptr(),
                signature.len(),
                public_key.as_ptr(),
            )
        };
        match status {
            sys::OQS_STATUS_OQS_SUCCESS => Ok(true),
            sys::OQS_STATUS_OQS_ERROR => Ok(false),
            other => Err(other),
        }
    }
}

impl Drop for Scheme {
    fn drop(&mut self) {
        // SAFETY: this guard is the sole owner of the scheme allocation, so it
        // is freed exactly once.
        unsafe { sys::OQS_SIG_free(self.0.as_ptr()) };
    }
}

/// The byte lengths liboqs reports for a scheme.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Lengths {
    pub(crate) public_key: usize,
    pub(crate) secret_key: usize,
    pub(crate) signature: usize,
}
