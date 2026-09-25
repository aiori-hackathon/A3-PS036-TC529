//! Safe Rust API for MTL mode operations.
//!
//! This crate wraps the raw FFI bindings in [`climb_mtl_sys`] behind owned,
//! opaque types. **No public item in this crate exposes a raw pointer**, and no
//! caller needs `unsafe` to sign or verify.
//!
//! # The two signing modes
//!
//! MTL mode amortizes the cost of an underlying post-quantum signature (SLH-DSA
//! here) across a batch of messages by signing a *Merkle tree ladder* of their
//! roots, and giving each message only a small *condensed* authentication path.
//! That produces the two conditions CLIMB benchmarks:
//!
//! * **With MTL** — call [`MtlSigner::sign_batch`] once with all `N` messages.
//!   libMTL builds one shared ladder and signs it **once**; every message gets a
//!   condensed signature.
//! * **Without MTL** — call [`MtlSigner::sign_batch`] `N` times with one message
//!   each. Each call builds a fresh ladder and pays for a full underlying
//!   signature, so no amortization occurs. This is the fair baseline: the same
//!   tool, algorithm, and key, differing only in batching.
//!
//! This `N`-calls-of-one-versus-one-call-of-`N` equivalence was verified
//! empirically against the native library during Phase 3, not assumed.
//!
//! # Example
//!
//! ```
//! use climb_mtl::MtlKeyPair;
//!
//! # fn main() -> Result<(), climb_mtl::MtlError> {
//! let keypair = MtlKeyPair::generate()?;
//! let signer = keypair.signer();
//! let verifier = keypair.verifier()?;
//!
//! let messages: [&[u8]; 2] = [b"first message", b"second message"];
//! let output = signer.sign_batch(&messages)?;
//!
//! let signature = output.signature(0)?;
//! assert!(verifier.verify(&messages[0], &signature, Some(output.ladder()), false)?);
//! # Ok(())
//! # }
//! ```

mod error;
mod ffi;

use std::ffi::CString;

pub use error::MtlError;

/// The algorithm selected in the project blueprint: the small-signature ("s")
/// SLH-DSA variant with an SHA2-128 MTL tree, chosen because DNS over UDP is a
/// size-sensitive context.
pub const DEFAULT_ALGORITHM: &str = "SLH-DSA-SHA2-128s-MTL-SHA2-128";

/// The maximum message length libMTL accepts, in bytes.
///
/// libMTL takes the message length as a C `uint16_t`; longer messages would be
/// silently truncated, so they are rejected instead.
pub const MAX_MESSAGE_LEN: usize = u16::MAX as usize;

fn cstring(algorithm: &str) -> Result<CString, MtlError> {
    CString::new(algorithm).map_err(|_| MtlError::InteriorNul(algorithm.to_string()))
}

/// Map a libMTL status from a signing/parsing operation into a hard error.
///
/// Used everywhere except verification, where a rejected signature is an
/// expected outcome rather than an error (see [`classify_verify`]).
fn sign_error(operation: &'static str, status: u32) -> MtlError {
    use climb_mtl_sys as sys;
    match status {
        sys::MTLLIB_STATUS_MTLLIB_MEMORY_ERROR => MtlError::OutOfMemory { operation },
        sys::MTLLIB_STATUS_MTLLIB_BUFFER_ISSUE => MtlError::BufferTooSmall { operation },
        _ => MtlError::SignFailed { operation, status },
    }
}

/// Map a libMTL status from a verification call into `Ok(true)` (accepted),
/// `Ok(false)` (rejected), or `Err` (the verifier itself failed).
///
/// Signature and ladder bytes are untrusted input. libMTL's parser rejects
/// malformed input with the same status range it uses for cryptographic
/// mismatches, and an empirical probe during Phase 3 confirmed that flipping a
/// single signature byte yields `MTLLIB_NULL_PARAMS` while flipping a message
/// byte yields `MTLLIB_BOGUS_CRYPTO`. Both mean the signature did not verify, so
/// every "rejected" status maps to `Ok(false)` and can never be mistaken for
/// success. Only allocation/buffer failures — which are not the signature's
/// fault — become `Err`.
fn classify_verify(operation: &'static str, status: u32) -> Result<bool, MtlError> {
    use climb_mtl_sys as sys;
    match status {
        sys::MTLLIB_STATUS_MTLLIB_OK | sys::MTLLIB_STATUS_MTLLIB_OK_VALIDATED_LADDER => Ok(true),
        sys::MTLLIB_STATUS_MTLLIB_BOGUS_CRYPTO
        | sys::MTLLIB_STATUS_MTLLIB_NO_LADDER
        | sys::MTLLIB_STATUS_MTLLIB_BAD_VALUE
        | sys::MTLLIB_STATUS_MTLLIB_NULL_PARAMS
        | sys::MTLLIB_STATUS_MTLLIB_INDETERMINATE => Ok(false),
        sys::MTLLIB_STATUS_MTLLIB_MEMORY_ERROR => Err(MtlError::OutOfMemory { operation }),
        sys::MTLLIB_STATUS_MTLLIB_BUFFER_ISSUE => Err(MtlError::BufferTooSmall { operation }),
        other => Err(MtlError::Native {
            operation,
            status: other,
        }),
    }
}

/// Validate one message against libMTL's per-message constraints.
fn validate_message(index: usize, message: &[u8]) -> Result<(), MtlError> {
    if message.is_empty() {
        return Err(MtlError::EmptyMessage { index });
    }
    if message.len() > MAX_MESSAGE_LEN {
        return Err(MtlError::MessageTooLong {
            index,
            len: message.len(),
            max: MAX_MESSAGE_LEN,
        });
    }
    Ok(())
}

/// Parse the series id out of a serialized key image.
///
/// The image written by libMTL is `[u32 len][algorithm][u32 len][secret key]
/// [u32 len][public key][u16 flags][u32 len][series id]...`, with every length a
/// big-endian `u32`. The series id is a fixed property of the keypair and must
/// be re-presented when building a verifier from the public key alone.
fn parse_series_id(image: &[u8]) -> Result<Vec<u8>, MtlError> {
    fn take<'a>(image: &'a [u8], pos: &mut usize) -> Result<&'a [u8], MtlError> {
        let start = *pos;
        let end = start.checked_add(4).filter(|e| *e <= image.len()).ok_or(
            MtlError::MalformedKeyImage {
                reason: "truncated length prefix",
            },
        )?;
        let len = u32::from_be_bytes(image[start..end].try_into().map_err(|_| {
            MtlError::MalformedKeyImage {
                reason: "truncated length prefix",
            }
        })?) as usize;
        let data_start = end;
        let data_end = data_start
            .checked_add(len)
            .filter(|e| *e <= image.len())
            .ok_or(MtlError::MalformedKeyImage {
                reason: "field extends past end of key image",
            })?;
        *pos = data_end;
        Ok(&image[data_start..data_end])
    }

    let mut pos = 0usize;
    for _ in 0..3 {
        take(image, &mut pos)?;
    }
    let flags_end =
        pos.checked_add(2)
            .filter(|e| *e <= image.len())
            .ok_or(MtlError::MalformedKeyImage {
                reason: "truncated flags field",
            })?;
    pos = flags_end;
    let series_id = take(image, &mut pos)?;
    Ok(series_id.to_vec())
}

/// A generated MTL keypair.
///
/// Owns the native key context (including its secret key) and frees it on drop.
/// Used to derive a [`MtlSigner`], which owns only the serialized key material,
/// and a [`MtlVerifier`], which holds public key plus series id.
pub struct MtlKeyPair {
    // Held purely for RAII: this is the owned native key context (secret key,
    // public key, OQS signature, series id). Its fields are never read because
    // the public key, series id, and key image captured below already cover
    // every accessor, but the context must stay alive for the keypair's lifetime
    // and be freed exactly once on drop.
    #[allow(dead_code)]
    ctx: ffi::Ctx,
    algorithm: String,
    secret_parameter: u16,
    public_key: Vec<u8>,
    series_id: Vec<u8>,
    key_image: Vec<u8>,
}

impl MtlKeyPair {
    /// Generate a keypair for [`DEFAULT_ALGORITHM`].
    pub fn generate() -> Result<Self, MtlError> {
        Self::generate_with_algorithm(DEFAULT_ALGORITHM)
    }

    /// Generate a keypair for a specific libMTL algorithm string.
    pub fn generate_with_algorithm(algorithm: &str) -> Result<Self, MtlError> {
        let alg_c = cstring(algorithm)?;
        let secret_parameter = ffi::security_parameter(&alg_c);
        if secret_parameter == 0 {
            return Err(MtlError::UnknownAlgorithm(algorithm.to_string()));
        }

        let ctx =
            ffi::Ctx::key_new(&alg_c).map_err(|status| sign_error("mtllib_key_new", status))?;
        let public_key = ctx
            .public_key()
            .map_err(|status| sign_error("mtllib_pubkey_to_buffer", status))?;
        let key_image = ctx
            .key_image()
            .map_err(|status| sign_error("mtllib_key_to_buffer", status))?;

        let series_id = parse_series_id(&key_image)?;
        let expected_sid_len = usize::from(secret_parameter) * 2;
        if series_id.len() != expected_sid_len {
            return Err(MtlError::SeriesIdLength {
                algorithm: algorithm.to_string(),
                expected: expected_sid_len,
                actual: series_id.len(),
            });
        }

        Ok(Self {
            ctx,
            algorithm: algorithm.to_string(),
            secret_parameter,
            public_key,
            series_id,
            key_image,
        })
    }

    /// The libMTL algorithm string this keypair was generated for.
    pub fn algorithm(&self) -> &str {
        &self.algorithm
    }

    /// The serialized public key.
    pub fn public_key(&self) -> &[u8] {
        &self.public_key
    }

    /// The series id, `2 x security parameter` bytes.
    pub fn series_id(&self) -> &[u8] {
        &self.series_id
    }

    /// Create a signer that re-derives a fresh native context per batch.
    ///
    /// Each batch starts from an empty ladder, so repeated calls do not
    /// accumulate leaves — this is what makes `N` single-message calls a
    /// faithful "without MTL" baseline.
    pub fn signer(&self) -> MtlSigner {
        MtlSigner {
            algorithm: self.algorithm.clone(),
            secret_parameter: self.secret_parameter,
            key_image: self.key_image.clone(),
        }
    }

    /// Create a verifier from this keypair's public key and series id.
    pub fn verifier(&self) -> Result<MtlVerifier, MtlError> {
        MtlVerifier::from_public_key(&self.algorithm, &self.public_key, &self.series_id)
    }
}

/// Signs batches of messages under one MTL ladder.
///
/// Holds no native resources, so it is freely `Send + Sync`: every
/// [`sign_batch`](MtlSigner::sign_batch) call reconstructs a private context from
/// the stored key image.
pub struct MtlSigner {
    algorithm: String,
    secret_parameter: u16,
    key_image: Vec<u8>,
}

impl MtlSigner {
    /// The libMTL algorithm string this signer uses.
    pub fn algorithm(&self) -> &str {
        &self.algorithm
    }

    /// Sign a batch of messages, sharing one MTL ladder across all of them.
    ///
    /// This is the **with MTL** condition. libMTL appends every message to a
    /// single node set and signs the resulting ladder once; each message is
    /// returned with its condensed signature, and the shared signed ladder is
    /// available on the [`SignOutput`].
    ///
    /// Calling this with one message at a time is the **without MTL** baseline:
    /// each call produces a fresh ladder and pays for a full underlying
    /// signature.
    ///
    /// # Errors
    ///
    /// Returns [`MtlError::EmptyBatch`] for an empty slice, and
    /// [`MtlError::EmptyMessage`] / [`MtlError::MessageTooLong`] for messages
    /// outside libMTL's supported range. The check happens before any native
    /// context is created.
    pub fn sign_batch(&self, messages: &[&[u8]]) -> Result<SignOutput, MtlError> {
        if messages.is_empty() {
            return Err(MtlError::EmptyBatch);
        }
        for (index, message) in messages.iter().enumerate() {
            validate_message(index, message)?;
        }

        let image = ffi::Buffer::from_slice(&self.key_image)
            .map_err(|status| sign_error("mtllib_buffer_initialize", status))?;
        let ctx = ffi::Ctx::from_key_image(&image)
            .map_err(|status| sign_error("mtllib_key_from_buffer", status))?;

        let mut handles = Vec::with_capacity(messages.len());
        for message in messages {
            let message_buffer = ffi::Buffer::from_slice(message)
                .map_err(|status| sign_error("mtllib_buffer_initialize", status))?;
            let handle = ctx
                .sign_append(&message_buffer)
                .map_err(|status| sign_error("mtllib_sign_append", status))?;
            handles.push(handle);
        }

        let ladder = ctx
            .signed_ladder()
            .map_err(|status| sign_error("mtllib_sign_get_signed_ladder", status))?;

        let mut signed = Vec::with_capacity(handles.len());
        for handle in &handles {
            let condensed = ctx
                .condensed_signature(handle)
                .map_err(|status| sign_error("mtllib_sign_get_condensed_sig", status))?;
            signed.push(SignedMessage {
                leaf_index: handle.leaf_index(),
                condensed: CondensedSignature { bytes: condensed },
            });
        }

        Ok(SignOutput {
            ladder: Ladder { bytes: ladder },
            messages: signed,
            secret_parameter: self.secret_parameter,
        })
    }
}

/// Verifies MTL signatures against a fixed public key.
///
/// Owns a native verifier context. It is intentionally **not** `Send` or `Sync`:
/// concurrent verification against one context would depend on thread-safety
/// guarantees the native library does not document. Construct one verifier per
/// thread if verification is parallelized.
pub struct MtlVerifier {
    ctx: ffi::Ctx,
    algorithm: String,
    secret_parameter: u16,
}

impl MtlVerifier {
    /// Build a verifier from a public key and its series id.
    ///
    /// `series_id` must be exactly `2 x security parameter` bytes; it is
    /// obtained from [`MtlKeyPair::series_id`].
    pub fn from_public_key(
        algorithm: &str,
        public_key: &[u8],
        series_id: &[u8],
    ) -> Result<Self, MtlError> {
        let alg_c = cstring(algorithm)?;
        let secret_parameter = ffi::security_parameter(&alg_c);
        if secret_parameter == 0 {
            return Err(MtlError::UnknownAlgorithm(algorithm.to_string()));
        }
        let expected_sid_len = usize::from(secret_parameter) * 2;
        if series_id.len() != expected_sid_len {
            return Err(MtlError::SeriesIdLength {
                algorithm: algorithm.to_string(),
                expected: expected_sid_len,
                actual: series_id.len(),
            });
        }

        let public_key_buffer = ffi::Buffer::from_slice(public_key)
            .map_err(|status| sign_error("mtllib_buffer_initialize", status))?;
        let ctx = ffi::Ctx::from_pubkey(&alg_c, &public_key_buffer, series_id)
            .map_err(|status| sign_error("mtllib_pubkey_from_buffer", status))?;

        Ok(Self {
            ctx,
            algorithm: algorithm.to_string(),
            secret_parameter,
        })
    }

    /// The libMTL algorithm string this verifier uses.
    pub fn algorithm(&self) -> &str {
        &self.algorithm
    }

    /// Verify `signature` over `message`.
    ///
    /// * A [`Signature::Full`] carries its own ladder, so the `ladder` argument
    ///   is ignored for it.
    /// * A [`Signature::Condensed`] requires `ladder` to be supplied, otherwise
    ///   [`MtlError::LadderRequired`] is returned.
    /// * `trust_cached_ladder` selects the modelling assumption: when `false`,
    ///   the ladder's own signature is verified first (a cold resolver); when
    ///   `true`, the ladder is assumed already validated and only the condensed
    ///   proof is checked (a resolver holding a previously validated ladder).
    ///   This mirrors the native verifier's `-t` option.
    ///
    /// Returns `Ok(true)` only when the signature verifies. Every rejection —
    /// including malformed input the native parser refuses — is `Ok(false)`, so
    /// a false positive is impossible.
    pub fn verify(
        &self,
        message: &[u8],
        signature: &Signature,
        ladder: Option<&Ladder>,
        trust_cached_ladder: bool,
    ) -> Result<bool, MtlError> {
        if message.is_empty() {
            return Err(MtlError::EmptyMessage { index: 0 });
        }

        match signature {
            Signature::Full(full) => {
                let message_buffer = ffi::Buffer::from_slice(message)
                    .map_err(|status| sign_error("mtllib_buffer_initialize", status))?;
                let signature_buffer = ffi::Buffer::from_slice(full.as_bytes())
                    .map_err(|status| sign_error("mtllib_buffer_initialize", status))?;
                let status = self.ctx.verify(&message_buffer, &signature_buffer, None);
                classify_verify("mtllib_verify", status)
            }
            Signature::Condensed(condensed) => {
                let ladder = ladder.ok_or(MtlError::LadderRequired)?;

                if !trust_cached_ladder {
                    let ladder_buffer = ffi::Buffer::from_slice(ladder.as_bytes())
                        .map_err(|status| sign_error("mtllib_buffer_initialize", status))?;
                    let status = self.ctx.verify_signed_ladder(&ladder_buffer);
                    if !classify_verify("mtllib_verify_signed_ladder", status)? {
                        return Ok(false);
                    }
                }

                let message_buffer = ffi::Buffer::from_slice(message)
                    .map_err(|status| sign_error("mtllib_buffer_initialize", status))?;
                let signature_buffer = ffi::Buffer::from_slice(condensed.as_bytes())
                    .map_err(|status| sign_error("mtllib_buffer_initialize", status))?;
                let ladder_buffer = ffi::Buffer::from_slice(ladder.as_bytes())
                    .map_err(|status| sign_error("mtllib_buffer_initialize", status))?;
                let status =
                    self.ctx
                        .verify(&message_buffer, &signature_buffer, Some(&ladder_buffer));
                classify_verify("mtllib_verify", status)
            }
        }
    }

    /// The security parameter (hash size in bytes) of this verifier's algorithm.
    pub fn secret_parameter(&self) -> u16 {
        self.secret_parameter
    }
}

/// A condensed MTL signature: a Merkle authentication path plus a randomizer.
///
/// This is the small form most responses carry. It authenticates a message
/// against a [`Ladder`] rather than against the public key directly.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CondensedSignature {
    bytes: Vec<u8>,
}

impl CondensedSignature {
    /// Wrap raw condensed-signature bytes.
    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        Self { bytes }
    }

    /// The raw signature bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Length in bytes.
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    /// Whether the signature is empty.
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }
}

/// A full MTL signature: a condensed signature followed by the signed ladder.
///
/// This is the large form a response occasionally carries so a verifier can
/// validate the ladder without having seen it before.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FullSignature {
    bytes: Vec<u8>,
}

impl FullSignature {
    /// Wrap raw full-signature bytes.
    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        Self { bytes }
    }

    /// The raw signature bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Length in bytes.
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    /// Whether the signature is empty.
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }
}

/// A signed Merkle tree ladder.
///
/// One ladder is shared by every message in a batch. It is signed once by the
/// underlying scheme, which is the source of MTL mode's amortization.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ladder {
    bytes: Vec<u8>,
}

impl Ladder {
    /// Wrap raw signed-ladder bytes.
    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        Self { bytes }
    }

    /// The raw ladder bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Length in bytes.
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    /// Whether the ladder is empty.
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }
}

/// A signature in whichever form it arrived: condensed or full.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Signature {
    /// A condensed signature, requiring a separate ladder to verify.
    Condensed(CondensedSignature),
    /// A full signature, carrying its own ladder.
    Full(FullSignature),
}

impl Signature {
    /// The raw signature bytes.
    pub fn as_bytes(&self) -> &[u8] {
        match self {
            Signature::Condensed(signature) => signature.as_bytes(),
            Signature::Full(signature) => signature.as_bytes(),
        }
    }

    /// Length in bytes.
    pub fn len(&self) -> usize {
        self.as_bytes().len()
    }

    /// Whether the signature is empty.
    pub fn is_empty(&self) -> bool {
        self.as_bytes().is_empty()
    }

    /// Whether this is the condensed form.
    pub fn is_condensed(&self) -> bool {
        matches!(self, Signature::Condensed(_))
    }

    /// Whether this is the full form.
    pub fn is_full(&self) -> bool {
        matches!(self, Signature::Full(_))
    }
}

/// One signed message within a batch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignedMessage {
    leaf_index: u64,
    condensed: CondensedSignature,
}

impl SignedMessage {
    /// The leaf index libMTL assigned to this message within the ladder.
    pub fn leaf_index(&self) -> u64 {
        self.leaf_index
    }

    /// The condensed signature for this message (the form MTL mode emits).
    pub fn condensed(&self) -> &CondensedSignature {
        &self.condensed
    }

    /// Reconstruct the full signature using the batch's shared ladder.
    pub fn full_signature(&self, ladder: &Ladder) -> FullSignature {
        let mut bytes = Vec::with_capacity(self.condensed.len() + ladder.len());
        bytes.extend_from_slice(self.condensed.as_bytes());
        bytes.extend_from_slice(ladder.as_bytes());
        FullSignature { bytes }
    }

    /// The condensed signature as a [`Signature`].
    pub fn condensed_signature(&self) -> Signature {
        Signature::Condensed(self.condensed.clone())
    }

    /// The reconstructed full signature as a [`Signature`].
    pub fn signature(&self, ladder: &Ladder) -> Signature {
        Signature::Full(self.full_signature(ladder))
    }
}

/// The result of signing a batch.
///
/// Holds one shared [`Ladder`] plus one condensed signature per message. The
/// full signature for any message is reconstructed on demand via
/// [`SignOutput::full_signature`]; storing all `N` full signatures eagerly would
/// cost `N x ~8 KB` and is unnecessary because the full form is exactly the
/// condensed bytes followed by the ladder bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignOutput {
    ladder: Ladder,
    messages: Vec<SignedMessage>,
    secret_parameter: u16,
}

impl SignOutput {
    /// The shared signed ladder for the batch.
    pub fn ladder(&self) -> &Ladder {
        &self.ladder
    }

    /// The signed messages, one per input message, in input order.
    pub fn messages(&self) -> &[SignedMessage] {
        &self.messages
    }

    /// Number of messages in the batch.
    pub fn len(&self) -> usize {
        self.messages.len()
    }

    /// Whether the batch was empty. A [`SignOutput`] is never empty in practice,
    /// because [`MtlSigner::sign_batch`] rejects empty input.
    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    /// The security parameter (hash size in bytes) of the signing algorithm.
    pub fn secret_parameter(&self) -> u16 {
        self.secret_parameter
    }

    /// The signed message at `index`, if present.
    pub fn get(&self, index: usize) -> Option<&SignedMessage> {
        self.messages.get(index)
    }

    /// The condensed signature at `index`.
    pub fn condensed_signature(&self, index: usize) -> Result<&CondensedSignature, MtlError> {
        self.get(index)
            .map(SignedMessage::condensed)
            .ok_or(MtlError::IndexOutOfRange {
                index,
                count: self.messages.len(),
            })
    }

    /// Reconstruct the full signature at `index` using the shared ladder.
    pub fn full_signature(&self, index: usize) -> Result<FullSignature, MtlError> {
        self.get(index)
            .map(|message| message.full_signature(&self.ladder))
            .ok_or(MtlError::IndexOutOfRange {
                index,
                count: self.messages.len(),
            })
    }

    /// The signature at `index` as a [`Signature`].
    ///
    /// Returns the condensed form, which is what MTL mode emits on the wire; use
    /// [`SignOutput::full_signature`] or [`SignOutput::full_signature_as`] for
    /// the full form.
    pub fn signature(&self, index: usize) -> Result<Signature, MtlError> {
        self.condensed_signature(index)
            .map(|c| Signature::Condensed(c.clone()))
    }

    /// The full signature at `index` as a [`Signature::Full`].
    pub fn full_signature_as(&self, index: usize) -> Result<Signature, MtlError> {
        self.full_signature(index).map(Signature::Full)
    }
}
