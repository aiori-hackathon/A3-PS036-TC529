//! Private RAII guards over the `climb-mtl-sys` bindings.
//!
//! Nothing in this module is public. Its entire purpose is to make every raw
//! libMTL allocation an owned Rust value whose `Drop` frees it exactly once, so
//! that no public API in [`crate`] needs to expose a pointer and no error path
//! can leak a C-side allocation.

use std::ffi::CStr;
use std::ptr::{self, NonNull};

use climb_mtl_sys as sys;

type Status = sys::MTLLIB_STATUS;

const OK: Status = sys::MTLLIB_STATUS_MTLLIB_OK;
const MEMORY_ERROR: Status = sys::MTLLIB_STATUS_MTLLIB_MEMORY_ERROR;

/// Owned `MTLLIB_BUFFER` allocated by libMTL.
///
/// Only the internal-allocation mode (`buffer_data == NULL`) is exposed: an
/// externally-wrapped buffer would alias Rust memory and is a use-after-free
/// hazard, so it is deliberately not modelled here.
pub(crate) struct Buffer(NonNull<sys::MTLLIB_BUFFER>);

impl Buffer {
    /// Allocate an owned internal buffer that can hold at least `len` bytes.
    ///
    /// libMTL uses `calloc(1, len)`, which may return a null pointer when
    /// `len == 0`, so the allocation is always at least one byte. This keeps the
    /// `NonNull` invariant valid on every path.
    pub(crate) fn owned(len: usize) -> Result<Self, Status> {
        let len = len.max(1);
        let mut raw: *mut sys::MTLLIB_BUFFER = ptr::null_mut();
        // SAFETY: `raw` is a valid out-pointer; `NULL` selects libMTL's internal
        // allocation mode, so no external memory is aliased. The returned buffer
        // is owned by this guard and freed exactly once in `Drop`.
        let status = unsafe { sys::mtllib_buffer_initialize(&mut raw, len, ptr::null_mut()) };
        if status != OK {
            return Err(status);
        }
        NonNull::new(raw).map(Buffer).ok_or(MEMORY_ERROR)
    }

    /// Allocate an owned buffer and append `data` to it.
    pub(crate) fn from_slice(data: &[u8]) -> Result<Self, Status> {
        let buffer = Self::owned(data.len())?;
        if data.is_empty() {
            return Ok(buffer);
        }
        // SAFETY: `buffer` is a live internal buffer with capacity >= data.len()
        // (it was allocated with exactly that length), and `data` is a live
        // slice for the duration of the call. libMTL copies the bytes.
        let status = unsafe {
            sys::mtllib_buffer_append(buffer.as_ptr(), data.as_ptr().cast_mut(), data.len())
        };
        if status != OK {
            // `buffer` drops here, so the allocation is not leaked on failure.
            return Err(status);
        }
        Ok(buffer)
    }

    pub(crate) fn as_ptr(&self) -> *mut sys::MTLLIB_BUFFER {
        self.0.as_ptr()
    }

    /// Copy the bytes currently in use out of the buffer.
    pub(crate) fn to_vec(&self) -> Vec<u8> {
        // SAFETY: `self.0` is a live, initialised libMTL buffer.
        let len = unsafe { sys::mtllib_buffer_in_use(self.as_ptr()) };
        if len == 0 {
            return Vec::new();
        }
        // SAFETY: `self.0` is live; for an internal buffer libMTL guarantees the
        // data pointer addresses at least `len` initialised bytes, and the guard
        // keeps the buffer alive across the copy.
        let ptr = unsafe { sys::mtllib_buffer_data_ptr(self.as_ptr()) };
        debug_assert!(
            !ptr.is_null(),
            "libMTL returned a null data pointer with a non-zero length"
        );
        // SAFETY: non-null (libMTL internal allocation) and valid for `len`
        // bytes; the resulting slice is immediately copied into an owned Vec.
        unsafe { std::slice::from_raw_parts(ptr, len) }.to_vec()
    }
}

impl Drop for Buffer {
    fn drop(&mut self) {
        // SAFETY: this guard is the sole owner of the allocation, so it is freed
        // exactly once. `mtllib_buffer_free` frees internal data then the struct.
        unsafe { sys::mtllib_buffer_free(self.0.as_ptr()) };
    }
}

/// Owned `MTLLIB_CTX` (a libMTL key/verifier context).
pub(crate) struct Ctx(NonNull<sys::MTLLIB_CTX>);

impl Ctx {
    /// Generate a fresh keypair and random series id for `algorithm`.
    pub(crate) fn key_new(algorithm: &CStr) -> Result<Self, Status> {
        let mut raw: *mut sys::MTLLIB_CTX = ptr::null_mut();
        // SAFETY: `algorithm` is a valid NUL-terminated C string; `raw` is a
        // valid out-pointer. On failure libMTL leaves nothing allocated, and on
        // success ownership transfers to this guard.
        let status = unsafe { sys::mtllib_key_new(algorithm.as_ptr().cast_mut(), &mut raw) };
        if status != OK {
            return Err(status);
        }
        NonNull::new(raw).map(Ctx).ok_or(MEMORY_ERROR)
    }

    /// Reconstruct a context from a serialized key image.
    pub(crate) fn from_key_image(image: &Buffer) -> Result<Self, Status> {
        let mut raw: *mut sys::MTLLIB_CTX = ptr::null_mut();
        // SAFETY: `image` is a live buffer holding a key image produced by
        // `mtllib_key_to_buffer`; `raw` is a valid out-pointer. libMTL copies the
        // key material into the new context, so `image` need not outlive it.
        let status = unsafe { sys::mtllib_key_from_buffer(image.as_ptr(), &mut raw) };
        if status != OK {
            return Err(status);
        }
        NonNull::new(raw).map(Ctx).ok_or(MEMORY_ERROR)
    }

    /// Build a verifier-only context from a public key and its series id.
    ///
    /// `series_id` must be exactly `2 * sec_param` bytes. libMTL reads that many
    /// bytes through the raw pointer, so the slice must be non-empty.
    pub(crate) fn from_pubkey(
        algorithm: &CStr,
        public_key: &Buffer,
        series_id: &[u8],
    ) -> Result<Self, Status> {
        debug_assert!(!series_id.is_empty(), "series id must be non-empty");
        let mut raw: *mut sys::MTLLIB_CTX = ptr::null_mut();
        // SAFETY: `algorithm` is a valid C string, `public_key` is a live buffer,
        // and `series_id` is a live slice of at least `2 * sec_param` bytes
        // (validated by the caller). `raw` is a valid out-pointer; ownership of
        // the new context transfers to this guard.
        let status = unsafe {
            sys::mtllib_pubkey_from_buffer(
                algorithm.as_ptr().cast_mut(),
                &mut raw,
                public_key.as_ptr(),
                series_id.as_ptr().cast_mut(),
            )
        };
        if status != OK {
            return Err(status);
        }
        NonNull::new(raw).map(Ctx).ok_or(MEMORY_ERROR)
    }

    pub(crate) fn as_ptr(&self) -> *mut sys::MTLLIB_CTX {
        self.0.as_ptr()
    }

    /// Serialize the public key.
    pub(crate) fn public_key(&self) -> Result<Vec<u8>, Status> {
        // SAFETY: `self.0` is a live key context.
        let len = unsafe { sys::mtllib_pubkey_to_buffer_length(self.as_ptr()) };
        if len == 0 {
            return Err(sys::MTLLIB_STATUS_MTLLIB_BAD_VALUE);
        }
        let buffer = Buffer::owned(len)?;
        // SAFETY: `self.0` is live and `buffer` has capacity `len`.
        let status = unsafe { sys::mtllib_pubkey_to_buffer(self.as_ptr(), buffer.as_ptr()) };
        if status != OK {
            return Err(status);
        }
        Ok(buffer.to_vec())
    }

    /// Serialize the full key image (algorithm, keys, series id, tree state).
    pub(crate) fn key_image(&self) -> Result<Vec<u8>, Status> {
        // SAFETY: `self.0` is a live key context.
        let len = unsafe { sys::mtllib_key_to_buffer_length(self.as_ptr()) };
        if len == 0 {
            return Err(sys::MTLLIB_STATUS_MTLLIB_BAD_VALUE);
        }
        let buffer = Buffer::owned(len)?;
        // SAFETY: `self.0` is live and `buffer` has capacity `len`.
        let status = unsafe { sys::mtllib_key_to_buffer(self.as_ptr(), buffer.as_ptr()) };
        if status != OK {
            return Err(status);
        }
        Ok(buffer.to_vec())
    }

    /// Append a message to the node set, returning a handle for it.
    pub(crate) fn sign_append(&self, message: &Buffer) -> Result<Handle, Status> {
        let mut raw: *mut sys::MTL_HANDLE = ptr::null_mut();
        // SAFETY: `self.0` is a live context, `message` is a live buffer, and
        // `raw` is a valid out-pointer. libMTL heap-allocates the handle; its
        // ownership transfers to the returned guard.
        let status = unsafe { sys::mtllib_sign_append(self.as_ptr(), message.as_ptr(), &mut raw) };
        if status != OK {
            return Err(status);
        }
        NonNull::new(raw).map(Handle).ok_or(MEMORY_ERROR)
    }

    /// Extract the condensed signature for a handle.
    pub(crate) fn condensed_signature(&self, handle: &Handle) -> Result<Vec<u8>, Status> {
        // SAFETY: both `self.0` and `handle.0` are live and belong together.
        let len =
            unsafe { sys::mtllib_sign_get_condensed_sig_length(self.as_ptr(), handle.as_ptr()) };
        if len == 0 {
            return Err(sys::MTLLIB_STATUS_MTLLIB_BAD_VALUE);
        }
        let buffer = Buffer::owned(len)?;
        // SAFETY: `self.0`/`handle.0` are live and `buffer` has capacity `len`.
        let status = unsafe {
            sys::mtllib_sign_get_condensed_sig(self.as_ptr(), handle.as_ptr(), buffer.as_ptr())
        };
        if status != OK {
            return Err(status);
        }
        Ok(buffer.to_vec())
    }

    /// Export the signed ladder for the current node set.
    ///
    /// This performs the single underlying signature over the ladder; every
    /// condensed signature in the batch authenticates against the result.
    pub(crate) fn signed_ladder(&self) -> Result<Vec<u8>, Status> {
        // SAFETY: `self.0` is a live context.
        let len = unsafe { sys::mtllib_sign_get_signed_ladder_length(self.as_ptr()) };
        if len == 0 {
            return Err(sys::MTLLIB_STATUS_MTLLIB_BAD_VALUE);
        }
        let buffer = Buffer::owned(len)?;
        // SAFETY: `self.0` is live and `buffer` has capacity `len`.
        let status = unsafe { sys::mtllib_sign_get_signed_ladder(self.as_ptr(), buffer.as_ptr()) };
        if status != OK {
            return Err(status);
        }
        Ok(buffer.to_vec())
    }

    /// Verify the signature embedded in a signed ladder.
    pub(crate) fn verify_signed_ladder(&self, ladder: &Buffer) -> Status {
        // SAFETY: `self.0` is a live context and `ladder` is a live buffer.
        unsafe { sys::mtllib_verify_signed_ladder(self.as_ptr(), ladder.as_ptr()) }
    }

    /// Verify `signature` over `message`, optionally against a known ladder.
    ///
    /// Passing `None` for the ladder is the full-signature path: libMTL uses the
    /// ladder embedded after the condensed prefix. Passing `Some` uses the
    /// caller-supplied ladder for a condensed signature.
    pub(crate) fn verify(
        &self,
        message: &Buffer,
        signature: &Buffer,
        ladder: Option<&Buffer>,
    ) -> Status {
        let ladder_ptr = ladder.map_or(ptr::null_mut(), Buffer::as_ptr);
        // SAFETY: `self.0`, `message`, and `signature` are live; `ladder_ptr` is
        // either null (valid for full signatures) or a live buffer. The final
        // `condensed_len` argument is documented optional and passed as null.
        unsafe {
            sys::mtllib_verify(
                self.as_ptr(),
                message.as_ptr(),
                signature.as_ptr(),
                ladder_ptr,
                ptr::null_mut(),
            )
        }
    }
}

impl Drop for Ctx {
    fn drop(&mut self) {
        // SAFETY: sole owner of the context; `mtllib_key_free` frees the keys,
        // the underlying OQS signature, the MTL node-set pages, and the context.
        unsafe { sys::mtllib_key_free(self.0.as_ptr()) };
    }
}

/// Owned `MTL_HANDLE` returned by [`Ctx::sign_append`].
///
/// The handle is a plain `{ sid, sid_len, leaf_index }` struct with no owned
/// pointers of its own, but it is heap-allocated by libMTL and must be freed.
/// This guard is deliberately not `Clone`: duplicating it would free the same
/// allocation twice.
pub(crate) struct Handle(NonNull<sys::MTL_HANDLE>);

impl Handle {
    pub(crate) fn as_ptr(&self) -> *mut sys::MTL_HANDLE {
        self.0.as_ptr()
    }

    /// The leaf index libMTL assigned to this message.
    pub(crate) fn leaf_index(&self) -> u64 {
        // SAFETY: `self.0` is a live MTL_HANDLE whose `leaf_index` field was
        // populated by `mtllib_sign_append`. Reading the Copy field is sound.
        unsafe { (*self.0.as_ptr()).leaf_index }
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        let mut raw = self.0.as_ptr();
        // SAFETY: sole owner of the handle. libMTL frees the allocation and sets
        // the local pointer to null; `raw` is discarded immediately after.
        unsafe { sys::mtllib_sign_free_handle(&mut raw) };
    }
}

/// Look up the security parameter (hash size in bytes) for an algorithm.
pub(crate) fn security_parameter(algorithm: &CStr) -> u16 {
    // SAFETY: `algorithm` is a valid NUL-terminated C string. The function only
    // performs a table lookup and returns 0 for unknown algorithms.
    unsafe { sys::mtllib_sig_buffer_get_hash_size(algorithm.as_ptr().cast_mut()) }
}
