//! Deterministic synthetic DNS dataset generation for CLIMB benchmarks.
//!
//! This crate produces reproducible, byte-identical DNS messages (response
//! wire format, via `hickory-proto`) from a seeded PRNG.  Each message is a
//! single DNS RRset — one owner name, one record type, 1..=4 resource records
//! — with the record type varying across messages (A / AAAA / MX / TXT / NS).
//!
//! ## Example
//!
//! ```
//! use climb_dns::{DatasetSize, generate_dataset};
//!
//! let dataset = generate_dataset(DatasetSize::Small, 42);
//! assert_eq!(dataset.len(), 100);
//! ```

pub mod anatomy;
pub mod dataset;
pub mod error;
mod synthetic;

pub use anatomy::{
    parse_response_anatomy, OptRecord, QuestionSection, ResponseAnatomy, RrEntry, RrsigEntry,
    EDNS_MTL_SIGTAG_CODE, HEADER_LEN, MTL_TYPE_CONDENSED, MTL_TYPE_FULL,
};
pub use dataset::{generate_dataset, generate_dataset_count, DatasetSize, MAX_MESSAGE_LEN};
pub use error::DnsError;
pub use synthetic::SyntheticRecord;

/// Return the recorded headline anatomies for the committed fixtures (full and condensed).
pub fn recorded_headline_anatomies() -> (ResponseAnatomy, ResponseAnatomy) {
    let full_hex = include_str!("fixtures/headline-full.hex");
    let cond_hex = include_str!("fixtures/headline-condensed.hex");
    let full_bytes = decode_hex_fixtures(full_hex.trim());
    let cond_bytes = decode_hex_fixtures(cond_hex.trim());
    let full = parse_response_anatomy(&full_bytes, Some("TCP")).expect("parse recorded full");
    let cond = parse_response_anatomy(&cond_bytes, Some("TCP")).expect("parse recorded condensed");
    (full, cond)
}

fn decode_hex_fixtures(s: &str) -> Vec<u8> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len() / 2);
    for pair in b.chunks(2) {
        if pair.len() < 2 {
            continue;
        }
        let hi = (pair[0] as char).to_digit(16).unwrap_or(0) as u8;
        let lo = (pair[1] as char).to_digit(16).unwrap_or(0) as u8;
        out.push((hi << 4) | lo);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each recorded headline pass carries three RRSIGs, all the same length:
    /// full-signature fields of 8,069 B (MTL-Type 0x01) and condensed proofs of
    /// 141 B (0x02). This pins the fixture anatomy the analytical UDP-fit model
    /// is validated against.
    #[test]
    fn recorded_headline_passes_carry_three_matching_rrsigs() {
        let (full, condensed) = recorded_headline_anatomies();
        assert_eq!(full.rrsig_count, 3);
        assert_eq!(condensed.rrsig_count, 3);
        let full_lens: Vec<u32> = full.rrsig_entries.iter().map(|e| e.signature_len).collect();
        let cond_lens: Vec<u32> = condensed
            .rrsig_entries
            .iter()
            .map(|e| e.signature_len)
            .collect();
        assert_eq!(full_lens, vec![8_069, 8_069, 8_069], "full sig fields");
        assert_eq!(cond_lens, vec![141, 141, 141], "condensed sig fields");
        assert_eq!(full.full_rrsig_count, 3);
        assert_eq!(condensed.condensed_rrsig_count, 3);
    }
}
