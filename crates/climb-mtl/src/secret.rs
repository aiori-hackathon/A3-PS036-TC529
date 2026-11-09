//! Zeroize-on-drop storage for secret key material.
//!
//! The wrapper keeps the serialized libMTL key image and the liboqs secret key
//! alive for the lifetime of their owners. Both contain the private key, so
//! they are stored in [`SecretBytes`], which overwrites its buffer with zeros
//! when dropped, never implements `Clone`, and redacts itself from `Debug`
//! output. This is benchmark tooling, not a production key store: the type
//! prevents accidental logging and reduces the residual lifetime of key bytes,
//! it does not defend against a process with memory access.

use std::fmt;

use zeroize::Zeroize;

/// A byte buffer that zeroes itself on drop and never prints its contents.
pub(crate) struct SecretBytes {
    bytes: Vec<u8>,
}

impl SecretBytes {
    /// Take ownership of `bytes` as secret material.
    pub(crate) fn new(bytes: Vec<u8>) -> Self {
        Self { bytes }
    }

    /// Borrow the secret bytes for a native call.
    pub(crate) fn as_slice(&self) -> &[u8] {
        &self.bytes
    }

    /// Length of the secret buffer in bytes.
    pub(crate) fn len(&self) -> usize {
        self.bytes.len()
    }

    /// Duplicate the secret material into a second owned buffer.
    ///
    /// This exists instead of `Clone` so every duplication site is explicit:
    /// creating a signer that reuses the keypair's key image is the one place
    /// the wrapper intentionally holds two copies.
    pub(crate) fn duplicate(&self) -> Self {
        Self::new(self.bytes.clone())
    }

    /// Overwrite the buffer in place and release its capacity.
    ///
    /// `Drop` calls this method, so testing it directly also covers the drop
    /// path without reading freed memory.
    fn zeroize_now(&mut self) {
        self.bytes.zeroize();
        self.bytes.clear();
        self.bytes.shrink_to_fit();
    }
}

impl Drop for SecretBytes {
    fn drop(&mut self) {
        self.zeroize_now();
    }
}

impl fmt::Debug for SecretBytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<redacted>")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_never_prints_key_material() {
        let secret = SecretBytes::new(vec![0xde, 0xad, 0xbe, 0xef]);
        let rendered = format!("{secret:?}");
        assert_eq!(rendered, "<redacted>");
        assert!(!rendered.contains("dead"));
        assert!(!rendered.contains("222"));
    }

    #[test]
    fn zeroize_now_overwrites_the_buffer() {
        let mut secret = SecretBytes::new(vec![0xaa; 64]);
        assert!(secret.as_slice().iter().all(|&b| b == 0xaa));
        secret.zeroize_now();
        assert!(secret.as_slice().is_empty());
        assert_eq!(secret.len(), 0);
    }

    #[test]
    fn duplicate_is_an_independent_buffer() {
        let original = SecretBytes::new(vec![1, 2, 3]);
        let mut copy = original.duplicate();
        assert_eq!(copy.as_slice(), &[1, 2, 3]);
        copy.zeroize_now();
        assert_eq!(original.as_slice(), &[1, 2, 3]);
    }
}
