//! A safe wrapper over a plain (no-MTL) SLH-DSA scheme from liboqs.
//!
//! This is the ancestor baseline the ladder comparison is measured against: one
//! full SLH-DSA signature per message, no Merkle tree ladder and no
//! amortization. It uses the same liboqs primitive the MTL wrapper signs its
//! ladder with, but bypasses libMTL entirely, so the measured cost is the
//! scheme's own.

use std::ffi::CString;
use std::marker::PhantomData;

use crate::error::MtlError;
use crate::ffi_plain::Scheme;
use crate::{DEFAULT_PLAIN_ALGORITHM, PLAIN_ALGORITHM_ENV};

/// A plain (non-MTL) SLH-DSA keypair and its scheme.
///
/// Owns the native `OQS_SIG` scheme and the generated keypair, and frees the
/// scheme on drop. It is deliberately **not** `Send` or `Sync`: liboqs does not
/// document the thread-safety of an `OQS_SIG` object, so sharing one across
/// threads would rely on behavior the library does not guarantee.
pub struct PlainSlhDsa {
    scheme: Scheme,
    public_key: Vec<u8>,
    secret_key: Vec<u8>,
    _not_thread_safe: PhantomData<*const ()>,
}

impl PlainSlhDsa {
    /// Generate a keypair for the configured plain scheme.
    pub fn generate() -> Result<Self, MtlError> {
        Self::generate_with_algorithm(configured_algorithm())
    }

    /// Generate a keypair for a named liboqs scheme.
    pub fn generate_with_algorithm(name: String) -> Result<Self, MtlError> {
        let scheme = scheme_for(&name)?;
        let (public_key, secret_key) = scheme
            .keypair()
            .map_err(|status| MtlError::PlainKeypairFailed { status })?;
        Ok(Self {
            scheme,
            public_key,
            secret_key,
            _not_thread_safe: PhantomData,
        })
    }

    /// The liboqs-reported signature length in bytes for this scheme.
    pub fn signature_len(&self) -> usize {
        self.scheme.lengths().signature
    }

    /// The public key bytes.
    pub fn public_key(&self) -> &[u8] {
        &self.public_key
    }

    /// Sign `message`, returning exactly the signature bytes.
    pub fn sign(&self, message: &[u8]) -> Result<Vec<u8>, MtlError> {
        self.scheme
            .sign(message, &self.secret_key)
            .map_err(|status| MtlError::PlainSignFailed { status })
    }

    /// Verify `signature` over `message` with this keypair's public key.
    pub fn verify(&self, message: &[u8], signature: &[u8]) -> Result<bool, MtlError> {
        self.scheme
            .verify(message, signature, &self.public_key)
            .map_err(|status| MtlError::PlainNative {
                operation: "OQS_SIG_verify",
                status,
            })
    }
}

fn configured_algorithm() -> String {
    std::env::var(PLAIN_ALGORITHM_ENV).unwrap_or_else(|_| DEFAULT_PLAIN_ALGORITHM.to_string())
}

fn scheme_for(name: &str) -> Result<Scheme, MtlError> {
    let algorithm =
        CString::new(name.to_string()).map_err(|_| MtlError::InteriorNul(name.to_string()))?;
    Scheme::new(&algorithm).ok_or_else(|| MtlError::SchemeUnavailable {
        algorithm: name.to_string(),
    })
}

/// The configured plain scheme's signature length in bytes, read from liboqs
/// rather than compiled in.
///
/// This is the per-message length the plain RRSIG wire total is extrapolated
/// from: one signature carries the whole message, so a batch of `n` messages
/// costs `n * signature_len()` signature bytes.
pub fn plain_signature_len() -> Result<usize, MtlError> {
    Ok(scheme_for(&configured_algorithm())?.lengths().signature)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::marker::PhantomData;

    #[test]
    fn round_trip_accepts_and_tampering_rejects() {
        let keypair = PlainSlhDsa::generate().expect("plain SLH-DSA keypair");
        let message = b"climb.example. IN TXT \"a plain baseline RRset\"";

        let signature = keypair.sign(message).expect("sign");
        assert_eq!(signature.len(), keypair.signature_len());
        assert!(keypair.verify(message, &signature).expect("verify"));

        let mut tampered = message.to_vec();
        tampered[0] ^= 0x01;
        assert!(!keypair
            .verify(&tampered, &signature)
            .expect("verify tampered message"));

        let mut broken = signature.clone();
        broken[0] ^= 0x01;
        assert!(!keypair
            .verify(message, &broken)
            .expect("verify tampered signature"));
    }

    #[test]
    fn signature_length_matches_liboqs_reported_length() {
        let keypair = PlainSlhDsa::generate().expect("plain SLH-DSA keypair");
        // Test data: 7856 is the value the pinned liboqs header records for
        // SLH_DSA_PURE_SHA2_128S; the assertion checks the wrapper surfaces the
        // library's own reported length rather than an internal constant.
        const HEADER_LENGTH_SLH_DSA_PURE_SHA2_128S: usize = 7856;
        assert_eq!(
            keypair.signature_len(),
            HEADER_LENGTH_SLH_DSA_PURE_SHA2_128S
        );
    }

    #[test]
    fn unknown_algorithm_is_reported() {
        let error = PlainSlhDsa::generate_with_algorithm("NOT_A_REAL_SCHEME".to_string())
            .err()
            .expect("unknown scheme must fail");
        assert!(matches!(error, MtlError::SchemeUnavailable { .. }));
    }

    #[test]
    fn plain_signature_len_matches_the_live_scheme() {
        let keypair = PlainSlhDsa::generate().expect("plain SLH-DSA keypair");
        let signature = keypair.sign(b"configuration A length probe").expect("sign");

        let reported = plain_signature_len().expect("liboqs signature length");
        assert_eq!(reported, keypair.signature_len());
        assert_eq!(signature.len(), reported);
    }

    #[test]
    fn plain_slh_dsa_is_neither_send_nor_sync() {
        trait AmbiguousIfSend<A> {
            fn item() {}
        }
        trait AmbiguousIfSync<A> {
            fn item() {}
        }
        struct Helper<T: ?Sized>(PhantomData<T>);
        impl<T: ?Sized> AmbiguousIfSend<()> for Helper<T> {}
        impl<T: ?Sized + Send> AmbiguousIfSend<u8> for Helper<T> {}
        impl<T: ?Sized> AmbiguousIfSync<()> for Helper<T> {}
        impl<T: ?Sized + Sync> AmbiguousIfSync<u8> for Helper<T> {}

        // Each of these only type-checks while exactly one impl applies; if
        // `PlainSlhDsa` became `Send`/`Sync` the marker would be ambiguous and
        // this module would not compile.
        let _ = <Helper<PlainSlhDsa> as AmbiguousIfSend<_>>::item;
        let _ = <Helper<PlainSlhDsa> as AmbiguousIfSync<_>>::item;
    }
}
