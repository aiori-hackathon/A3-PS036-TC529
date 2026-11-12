//! Batch-size sweep measurement: real libMTL bytes and signing time per `N`.
//!
//! One keypair is reused across every requested size. For each size the
//! harness generates the deterministic dataset, times `sign_batch` on the whole
//! batch (one ladder signature plus tree work), and parses the real signed
//! ladder and every condensed signature back into their structural fields.
//! The per-operation costs used above the measured limit are timed once in the
//! same pass — a plain SLH-DSA sign and verify, one complete MTL signature, and
//! one condensed verification with and without a trust-cached ladder.
//!
//! Everything fallible returns [`MtlError`]; nothing here panics on a signing
//! failure or assumes a non-empty batch.

#[cfg(feature = "native")]
use std::time::Instant;

#[cfg(feature = "native")]
use climb_dns::generate_dataset_count;
#[cfg(feature = "native")]
use climb_mtl::{MtlError, MtlKeyPair, PlainSlhDsa};

#[cfg(feature = "native")]
use crate::ladder_verify::parse_condensed_raw;
#[cfg(feature = "native")]
use crate::ladder_verify::parse_signed_ladder_raw;
#[cfg(feature = "native")]
use crate::ladder_verify::LadderVerifyError;

/// Errors from a sweep measurement pass.
#[cfg(feature = "native")]
#[derive(Debug, thiserror::Error)]
pub enum SweepError {
    /// A libMTL operation failed.
    #[error(transparent)]
    Mtl(#[from] MtlError),
    /// The returned bytes did not match the pinned ladder layout.
    #[error(transparent)]
    Ladder(#[from] LadderVerifyError),
}

/// Per-operation costs timed once, used to model sizes above the limit.
#[derive(Debug, Clone, PartialEq)]
pub struct SweepModelInputs {
    /// One plain SLH-DSA signature.
    pub t_plain_sign_ns: f64,
    /// One plain SLH-DSA verification.
    pub t_plain_verify_ns: f64,
    /// One complete MTL signature (a batch of one).
    pub t_mtl_single_sign_ns: f64,
    /// One condensed verification against a trust-cached ladder.
    pub t_verify_condensed_cached_ns: f64,
    /// One condensed verification that validates the ladder first.
    pub t_verify_condensed_cold_ns: f64,
    /// Plain signature length read from liboqs, in bytes.
    pub plain_signature_len_bytes: usize,
    /// Sign samples per timed operation.
    pub sign_samples: usize,
    /// Verification samples per timed operation.
    pub verify_samples: usize,
}

/// One measured batch size.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SweepMeasuredRow {
    /// Messages in the batch.
    pub n: u64,
    /// Rungs libMTL emitted.
    pub rung_count: usize,
    /// Unsigned ladder-buffer length in bytes.
    pub ladder_buffer_len: usize,
    /// Signed ladder length in bytes.
    pub signed_ladder_len: usize,
    /// Embedded underlying signature length in bytes.
    pub signature_len: u64,
    /// Smallest condensed signature.
    pub condensed_min: usize,
    /// Mean condensed signature.
    pub condensed_mean: f64,
    /// Largest condensed signature.
    pub condensed_max: usize,
    /// Sum of every condensed signature.
    pub condensed_total: u64,
    /// Median wall-clock time to sign the batch.
    pub batch_sign_ns: f64,
}

/// A complete measurement pass.
#[derive(Debug, Clone, PartialEq)]
pub struct SweepMeasurement {
    /// Per-operation costs.
    pub model: SweepModelInputs,
    /// Measured rows in the requested order.
    pub rows: Vec<SweepMeasuredRow>,
}

/// Median of a sample. An empty sample is zero; callers always supply at least
/// one sample.
#[cfg(any(feature = "native", test))]
fn median_ns(values: &mut [f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mid = values.len() / 2;
    if values.len() % 2 == 1 {
        values.get(mid).copied().unwrap_or(0.0)
    } else {
        let low = values.get(mid.saturating_sub(1)).copied().unwrap_or(0.0);
        let high = values.get(mid).copied().unwrap_or(0.0);
        (low + high) / 2.0
    }
}

#[cfg(feature = "native")]
fn time_sign_batch(
    signer: &climb_mtl::MtlSigner,
    messages: &[&[u8]],
    samples: usize,
) -> Result<f64, MtlError> {
    let mut times = Vec::with_capacity(samples.max(1));
    for _ in 0..samples.max(1) {
        let start = Instant::now();
        let output = signer.sign_batch(messages)?;
        let elapsed = start.elapsed().as_secs_f64() * 1e9;
        std::hint::black_box(&output);
        times.push(elapsed);
    }
    Ok(median_ns(&mut times))
}

#[cfg(feature = "native")]
fn time_plain_sign(plain: &PlainSlhDsa, message: &[u8], samples: usize) -> Result<f64, MtlError> {
    let mut times = Vec::with_capacity(samples.max(1));
    for _ in 0..samples.max(1) {
        let start = Instant::now();
        let signature = plain.sign(message)?;
        let elapsed = start.elapsed().as_secs_f64() * 1e9;
        std::hint::black_box(&signature);
        times.push(elapsed);
    }
    Ok(median_ns(&mut times))
}

#[cfg(feature = "native")]
fn time_plain_verify(
    plain: &PlainSlhDsa,
    message: &[u8],
    signature: &[u8],
    samples: usize,
) -> Result<f64, MtlError> {
    let mut times = Vec::with_capacity(samples.max(1));
    for _ in 0..samples.max(1) {
        let start = Instant::now();
        let ok = plain.verify(message, signature)?;
        let elapsed = start.elapsed().as_secs_f64() * 1e9;
        std::hint::black_box(ok);
        times.push(elapsed);
    }
    Ok(median_ns(&mut times))
}

/// Measure every requested batch size, plus the per-operation cost model.
///
/// # Errors
///
/// Returns [`MtlError`] if key generation, signing, verification or signature
/// extraction fails.
#[cfg(feature = "native")]
pub fn measure_sweep(
    seed: u64,
    sizes: &[u64],
    sign_samples: usize,
    verify_samples: usize,
) -> Result<SweepMeasurement, SweepError> {
    let keypair = MtlKeyPair::generate()?;
    let signer = keypair.signer();
    let verifier = keypair.verifier()?;
    let plain = PlainSlhDsa::generate()?;
    let plain_signature_len_bytes = plain.signature_len();

    // A small deterministic batch for the per-operation timings.
    let warm = generate_dataset_count(4, seed);
    let warm_messages: Vec<&[u8]> = warm.iter().map(|m| m.as_slice()).collect();
    let Some(first_message) = warm_messages.first().copied() else {
        return Err(SweepError::Mtl(MtlError::EmptyMessage { index: 0 }));
    };
    let plain_signature = plain.sign(first_message)?;
    let t_plain_sign_ns = time_plain_sign(&plain, first_message, sign_samples)?;
    let t_plain_verify_ns =
        time_plain_verify(&plain, first_message, &plain_signature, verify_samples)?;
    let t_mtl_single_sign_ns = time_sign_batch(&signer, &[first_message], sign_samples)?;

    // One small MTL batch for the condensed-verification timings.
    let warm_output = signer.sign_batch(&warm_messages)?;
    let warm_ladder = warm_output.ladder();
    let mut warm_signatures = Vec::with_capacity(warm_messages.len());
    for index in 0..warm_messages.len() {
        warm_signatures.push(warm_output.signature(index)?);
    }
    let mut cached_times = Vec::with_capacity(verify_samples.max(1));
    let mut cold_times = Vec::with_capacity(verify_samples.max(1));
    for sample in 0..verify_samples.max(1) {
        let Some(index) =
            (0..warm_messages.len()).find(|i| *i == sample % warm_messages.len().max(1))
        else {
            continue;
        };
        let (Some(message), Some(signature)) =
            (warm_messages.get(index), warm_signatures.get(index))
        else {
            continue;
        };
        let start = Instant::now();
        let ok = verifier.verify(message, signature, Some(warm_ladder), true)?;
        cached_times.push(start.elapsed().as_secs_f64() * 1e9);
        std::hint::black_box(ok);

        if sample < 3 {
            let start = Instant::now();
            let ok = verifier.verify(message, signature, Some(warm_ladder), false)?;
            cold_times.push(start.elapsed().as_secs_f64() * 1e9);
            std::hint::black_box(ok);
        }
    }
    let t_verify_condensed_cached_ns = median_ns(&mut cached_times);
    let t_verify_condensed_cold_ns = median_ns(&mut cold_times);

    let mut rows = Vec::with_capacity(sizes.len());
    for &n in sizes {
        let dataset = generate_dataset_count(usize::try_from(n).unwrap_or(usize::MAX), seed);
        let messages: Vec<&[u8]> = dataset.iter().map(|m| m.as_slice()).collect();
        let batch_sign_ns = time_sign_batch(&signer, &messages, sign_samples)?;
        let output = signer.sign_batch(&messages)?;
        let signed_ladder_len = output.ladder().len();
        let parsed = parse_signed_ladder_raw(output.ladder().as_bytes(), n as usize)?;
        let mut condensed_lens = Vec::with_capacity(messages.len());
        for (index, message) in output.messages().iter().enumerate() {
            condensed_lens.push(parse_condensed_raw(
                message.condensed().as_bytes(),
                n as usize,
                index,
            )?);
        }
        let min = condensed_lens.iter().copied().min().unwrap_or(0);
        let max = condensed_lens.iter().copied().max().unwrap_or(0);
        let total: u64 = condensed_lens.iter().map(|&len| len as u64).sum();
        let mean = if condensed_lens.is_empty() {
            0.0
        } else {
            total as f64 / condensed_lens.len() as f64
        };
        rows.push(SweepMeasuredRow {
            n,
            rung_count: parsed.rung_count,
            ladder_buffer_len: parsed.ladder_buffer_len,
            signed_ladder_len,
            signature_len: parsed.signature_len as u64,
            condensed_min: min,
            condensed_mean: mean,
            condensed_max: max,
            condensed_total: total,
            batch_sign_ns,
        });
    }

    Ok(SweepMeasurement {
        model: SweepModelInputs {
            t_plain_sign_ns,
            t_plain_verify_ns,
            t_mtl_single_sign_ns,
            t_verify_condensed_cached_ns,
            t_verify_condensed_cold_ns,
            plain_signature_len_bytes,
            sign_samples,
            verify_samples,
        },
        rows,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn median_of_an_odd_sample_is_the_middle_value() {
        assert_eq!(median_ns(&mut [3.0, 1.0, 2.0]), 2.0);
    }

    #[test]
    fn median_of_an_even_sample_averages_the_middle_pair() {
        assert_eq!(median_ns(&mut [4.0, 1.0, 3.0, 2.0]), 2.5);
    }

    #[test]
    fn an_empty_sample_has_no_median() {
        assert_eq!(median_ns(&mut []), 0.0);
    }
}
