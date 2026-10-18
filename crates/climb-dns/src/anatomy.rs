//! DNS response anatomy parsing for captured wire bytes.
//!
//! This module decomposes a captured DNS response into the byte accounting
//! needed to answer "where do the bytes go?": the fixed 12-byte header, the
//! question, every resource record in the answer / authority / additional
//! sections (owner length, type, class, TTL, RDATA length, total length), the
//! EDNS OPT pseudo-record and its option codes, and the RRSIG records with
//! their signature lengths and MTL-Type lead byte.
//!
//! # Why a hand-rolled reader
//!
//! The sizes reported here are *measured* off the wire, so the parse must
//! account for every byte: name-compression pointers, RDATA lengths and
//! section counts all have to line up exactly with the captured length. A
//! bounds-checked reader that returns an offset on failure keeps malformed or
//! truncated input from panicking mid-parse and lets the caller report where
//! the message stopped making sense.
//!
//! # MTL-Type byte
//!
//! For SLH-DSA-MTL DNSSEC responses the first byte of each RRSIG signature
//! field distinguishes a full signature (0x01 — carries a freshly signed
//! ladder) from a condensed proof (0x02 — carries only a Merkle
//! authentication path). See `draft-fregly-dnsop-slh-dsa-mtl-dnssec`.

use serde::{Deserialize, Serialize};

use crate::error::DnsError;

/// Fixed DNS header length in bytes.
pub const HEADER_LEN: usize = 12;

/// EDNS OPT pseudo-record type code.
const TYPE_OPT: u16 = 41;
/// RRSIG record type code.
const TYPE_RRSIG: u16 = 46;

/// The EDNS option code that carries an MTL SigTag (ladder hash).
pub const EDNS_MTL_SIGTAG_CODE: u16 = 65050;

/// MTL-Type lead byte: the response carries a full signature.
pub const MTL_TYPE_FULL: u8 = 0x01;
/// MTL-Type lead byte: the response carries a condensed proof.
pub const MTL_TYPE_CONDENSED: u8 = 0x02;

// ---------------------------------------------------------------------------
// Value types
// ---------------------------------------------------------------------------

/// The question section's byte accounting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuestionSection {
    /// Bytes occupied by the (possibly compressed) QNAME field.
    pub name_bytes: usize,
    /// Total bytes: QNAME + QTYPE (2) + QCLASS (2).
    pub total_bytes: usize,
}

/// One resource record's wire accounting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RrEntry {
    /// Bytes occupied by the owner name field (compression pointer included).
    pub owner_name_bytes: usize,
    /// RR type code.
    pub rtype: u16,
    /// RR class code.
    pub rclass: u16,
    /// TTL in seconds.
    pub ttl: u32,
    /// RDATA length in bytes.
    pub rdata_len: u16,
    /// Total bytes: owner + fixed 10-byte header + RDATA.
    pub total_len: usize,
}

impl RrEntry {
    /// True when this is the OPT pseudo-record (type 41).
    pub fn is_opt(&self) -> bool {
        self.rtype == TYPE_OPT
    }

    /// True when this is an RRSIG record (type 46).
    pub fn is_rrsig(&self) -> bool {
        self.rtype == TYPE_RRSIG
    }
}

/// The EDNS OPT pseudo-record's decoded contents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OptRecord {
    /// Advertised UDP payload size (the CLASS field of the OPT RR).
    pub udp_payload_size: u16,
    /// Extended flags (the TTL field: high-order bits extended RCODE + version,
    /// low-order flags including DO).
    pub flags: u32,
    /// DO (DNSSEC OK) bit, extracted from `flags`.
    pub dnssec_ok: bool,
    /// EDNS option codes present, in wire order.
    pub option_codes: Vec<u16>,
    /// True when the MTL SigTag option (65050) is present.
    pub has_mtl_sigtag: bool,
}

/// One RRSIG record's signature accounting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RrsigEntry {
    /// RR type covered by this signature.
    pub type_covered: u16,
    /// Signature algorithm (e.g. the SLH-DSA-MTL algorithm number).
    pub algorithm: u8,
    /// Length of the signature field in bytes.
    pub signature_len: u32,
    /// The MTL-Type lead byte (0x01 full / 0x02 condensed) when the signature
    /// field is non-empty; `None` for an empty signature field.
    pub mtl_type_byte: Option<u8>,
    /// Total bytes this RRSIG record occupies on the wire.
    pub total_len: usize,
}

impl RrsigEntry {
    /// Human-readable MTL kind, or `None` when the lead byte is absent or not
    /// a recognised MTL-Type.
    pub fn mtl_kind(&self) -> Option<&'static str> {
        match self.mtl_type_byte {
            Some(MTL_TYPE_FULL) => Some("full"),
            Some(MTL_TYPE_CONDENSED) => Some("condensed"),
            _ => None,
        }
    }
}

/// Decomposed anatomy of one captured DNS response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseAnatomy {
    /// Total captured wire length in bytes.
    pub wire_length: usize,
    /// Fixed header length (always 12).
    pub header_bytes: usize,
    /// TC (truncation) flag from the header.
    pub tc: bool,
    /// Transport the response was captured over, when known.
    pub transport: Option<String>,
    /// Question section accounting.
    pub question: QuestionSection,
    /// Answer-section records.
    pub answer: Vec<RrEntry>,
    /// Authority-section records.
    pub authority: Vec<RrEntry>,
    /// Additional-section records (excluding the OPT pseudo-record, which is
    /// reported separately via `opt`).
    pub additional: Vec<RrEntry>,
    /// Decoded OPT record, when present.
    pub opt: Option<OptRecord>,
    /// Bytes the OPT pseudo-record occupies, 0 when absent.
    pub opt_total_len: usize,
    /// Number of RRSIG records across all sections.
    pub rrsig_count: usize,
    /// Per-RRSIG signature accounting, in wire order.
    pub rrsig_entries: Vec<RrsigEntry>,
    /// Number of full-signature RRSIGs (MTL-Type 0x01).
    pub full_rrsig_count: usize,
    /// Number of condensed-proof RRSIGs (MTL-Type 0x02).
    pub condensed_rrsig_count: usize,
    /// Sum of every part: header + question + answer + authority + additional
    /// + OPT. Equals `wire_length` for a well-formed message.
    pub parts_sum: usize,
}

// ---------------------------------------------------------------------------
// Bounds-checked readers
// ---------------------------------------------------------------------------

fn read_u8(data: &[u8], offset: usize) -> Result<u8, DnsError> {
    data.get(offset)
        .copied()
        .ok_or(DnsError::Truncated { offset, need: 1 })
}

fn read_u16_be(data: &[u8], offset: usize) -> Result<u16, DnsError> {
    let end = offset.saturating_add(2);
    let bytes = data
        .get(offset..end)
        .ok_or(DnsError::Truncated { offset, need: 2 })?;
    Ok(u16::from_be_bytes([bytes[0], bytes[1]]))
}

fn read_u32_be(data: &[u8], offset: usize) -> Result<u32, DnsError> {
    let end = offset.saturating_add(4);
    let bytes = data
        .get(offset..end)
        .ok_or(DnsError::Truncated { offset, need: 4 })?;
    Ok(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

/// Skip a DNS name field (label sequence or compression pointer).
///
/// Returns the number of bytes the field occupies on the wire (2 for a
/// pointer, the full uncompressed length otherwise) and the offset just past
/// the field. Name contents are not decoded — only the byte span matters for
/// the anatomy accounting.
fn skip_name(data: &[u8], offset: usize) -> Result<(usize, usize), DnsError> {
    let mut pos = offset;
    let mut consumed = 0usize;
    loop {
        let len = read_u8(data, pos)? as usize;
        consumed += 1;
        pos += 1;
        if len == 0 {
            break;
        }
        if len & 0xc0 == 0xc0 {
            // Compression pointer: one more byte, then stop.
            read_u8(data, pos)?;
            consumed += 1;
            pos += 1;
            break;
        }
        if len & 0xc0 != 0 {
            // Reserved label type — malformed.
            return Err(DnsError::Truncated {
                offset: pos - 1,
                need: 1,
            });
        }
        let end = pos.saturating_add(len);
        if end > data.len() {
            return Err(DnsError::Truncated {
                offset: pos,
                need: len,
            });
        }
        pos = end;
        consumed += len;
    }
    Ok((consumed, pos))
}

// ---------------------------------------------------------------------------
// Section / RRSIG parsing
// ---------------------------------------------------------------------------

/// Parse one RR at `offset`, returning its accounting and the next offset.
fn parse_rr(data: &[u8], offset: usize) -> Result<(RrEntry, usize, usize, usize), DnsError> {
    let (owner_name_bytes, after_name) = skip_name(data, offset)?;
    let rtype = read_u16_be(data, after_name)?;
    let rclass = read_u16_be(data, after_name + 2)?;
    let ttl = read_u32_be(data, after_name + 4)?;
    let rdlength = read_u16_be(data, after_name + 8)? as usize;
    let rdata_start = after_name + 10;
    let rdata_end = rdata_start.saturating_add(rdlength);
    if rdata_end > data.len() {
        return Err(DnsError::Truncated {
            offset: rdata_start,
            need: rdlength,
        });
    }
    let total_len = owner_name_bytes + 10 + rdlength;
    let entry = RrEntry {
        owner_name_bytes,
        rtype,
        rclass,
        ttl,
        rdata_len: rdlength as u16,
        total_len,
    };
    Ok((entry, rdata_end, rdata_start, rdlength))
}

/// Decode an RRSIG's signature accounting from its RDATA span.
///
/// Layout: TypeCovered(2) Algorithm(1) Labels(1) OrigTTL(4) Expiration(4)
/// Inception(4) KeyTag(2) SignerName(variable) Signature(variable).
fn parse_rrsig(data: &[u8], rdata_start: usize, rdlength: usize, total_len: usize) -> RrsigEntry {
    let rdata_end = rdata_start + rdlength;
    let type_covered = read_u16_be(data, rdata_start).unwrap_or(0);
    let algorithm = read_u8(data, rdata_start + 2).unwrap_or(0);
    // Fixed prefix before the signer name is 18 bytes.
    const SIG_PREFIX: usize = 18;
    if rdlength < SIG_PREFIX {
        return RrsigEntry {
            type_covered,
            algorithm,
            signature_len: 0,
            mtl_type_byte: None,
            total_len,
        };
    }
    let signer_start = rdata_start + SIG_PREFIX;
    let sig_start = match skip_name(data, signer_start) {
        Ok((_, next)) => next,
        Err(_) => rdata_end,
    };
    let signature_len = rdata_end.saturating_sub(sig_start) as u32;
    let mtl_type_byte = if signature_len > 0 {
        read_u8(data, sig_start).ok()
    } else {
        None
    };
    RrsigEntry {
        type_covered,
        algorithm,
        signature_len,
        mtl_type_byte,
        total_len,
    }
}

/// Decode an OPT pseudo-record from its RR.
fn parse_opt(entry: &RrEntry, data: &[u8], rdata_start: usize, rdlength: usize) -> OptRecord {
    let rdata_end = rdata_start + rdlength;
    let mut option_codes = Vec::new();
    let mut pos = rdata_start;
    while pos + 4 <= rdata_end {
        let code = read_u16_be(data, pos).unwrap_or(0);
        let optlen = read_u16_be(data, pos + 2).unwrap_or(0) as usize;
        pos += 4;
        if pos + optlen > rdata_end {
            break;
        }
        option_codes.push(code);
        pos += optlen;
    }
    let dnssec_ok = entry.ttl & 0x8000 != 0;
    OptRecord {
        udp_payload_size: entry.rclass,
        flags: entry.ttl,
        dnssec_ok,
        has_mtl_sigtag: option_codes.contains(&EDNS_MTL_SIGTAG_CODE),
        option_codes,
    }
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Parse a captured DNS response into its [`ResponseAnatomy`].
///
/// `transport` is recorded verbatim when known (e.g. `"TCP"` / `"UDP"`).
///
/// # Errors
///
/// Returns [`DnsError::Truncated`] naming the byte offset when the message is
/// malformed or shorter than its own section counts and lengths imply.
pub fn parse_response_anatomy(
    data: &[u8],
    transport: Option<&str>,
) -> Result<ResponseAnatomy, DnsError> {
    if data.len() < HEADER_LEN {
        return Err(DnsError::Truncated {
            offset: 0,
            need: HEADER_LEN,
        });
    }
    let wire_length = data.len();

    // Header flags live at offset 2; TC is bit 9 of the 16-bit flags word.
    let flags = read_u16_be(data, 2)?;
    let tc = flags & 0x0200 != 0;
    let qdcount = read_u16_be(data, 4)? as usize;
    let ancount = read_u16_be(data, 6)? as usize;
    let nscount = read_u16_be(data, 8)? as usize;
    let arcount = read_u16_be(data, 10)? as usize;

    let mut offset = HEADER_LEN;

    // Question section — responses carry exactly one question in practice, but
    // the accounting loops over QDCOUNT so a multi-question message stays
    // byte-exact.
    let mut question = QuestionSection {
        name_bytes: 0,
        total_bytes: 0,
    };
    for _ in 0..qdcount {
        let (qname_bytes, after_qname) = skip_name(data, offset)?;
        // Skip QTYPE + QCLASS.
        read_u16_be(data, after_qname)?;
        read_u16_be(data, after_qname + 2)?;
        question.name_bytes += qname_bytes;
        question.total_bytes += qname_bytes + 4;
        offset = after_qname + 4;
    }

    let mut answer = Vec::with_capacity(ancount);
    let mut authority = Vec::with_capacity(nscount);
    let mut additional = Vec::with_capacity(arcount);
    let mut rrsig_entries = Vec::new();
    let mut opt: Option<OptRecord> = None;
    let mut opt_total_len = 0usize;

    // Answer section.
    for _ in 0..ancount {
        let (entry, next, rdata_start, rdlength) = parse_rr(data, offset)?;
        if entry.is_rrsig() {
            rrsig_entries.push(parse_rrsig(data, rdata_start, rdlength, entry.total_len));
        }
        answer.push(entry);
        offset = next;
    }

    // Authority section.
    for _ in 0..nscount {
        let (entry, next, rdata_start, rdlength) = parse_rr(data, offset)?;
        if entry.is_rrsig() {
            rrsig_entries.push(parse_rrsig(data, rdata_start, rdlength, entry.total_len));
        }
        authority.push(entry);
        offset = next;
    }

    // Additional section (OPT reported separately, not double-counted).
    for _ in 0..arcount {
        let (entry, next, rdata_start, rdlength) = parse_rr(data, offset)?;
        if entry.is_opt() {
            opt = Some(parse_opt(&entry, data, rdata_start, rdlength));
            opt_total_len = entry.total_len;
        } else {
            if entry.is_rrsig() {
                rrsig_entries.push(parse_rrsig(data, rdata_start, rdlength, entry.total_len));
            }
            additional.push(entry);
        }
        offset = next;
    }

    let full_rrsig_count = rrsig_entries
        .iter()
        .filter(|e| e.mtl_type_byte == Some(MTL_TYPE_FULL))
        .count();
    let condensed_rrsig_count = rrsig_entries
        .iter()
        .filter(|e| e.mtl_type_byte == Some(MTL_TYPE_CONDENSED))
        .count();
    let rrsig_count = rrsig_entries.len();

    // Sum every part. Answer/authority/additional include their RRSIGs; OPT is
    // added once here because it is excluded from `additional`.
    let parts_sum = HEADER_LEN
        + question.total_bytes
        + answer.iter().map(|r| r.total_len).sum::<usize>()
        + authority.iter().map(|r| r.total_len).sum::<usize>()
        + additional.iter().map(|r| r.total_len).sum::<usize>()
        + opt_total_len;

    Ok(ResponseAnatomy {
        wire_length,
        header_bytes: HEADER_LEN,
        tc,
        transport: transport.map(str::to_string),
        question,
        answer,
        authority,
        additional,
        opt,
        opt_total_len,
        rrsig_count,
        rrsig_entries,
        full_rrsig_count,
        condensed_rrsig_count,
        parts_sum,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal one-question response with no records.
    fn minimal_response() -> Vec<u8> {
        let mut m = Vec::new();
        m.extend_from_slice(&[0x12, 0x34]); // ID
        m.extend_from_slice(&[0x84, 0x00]); // flags: response, no TC
        m.extend_from_slice(&[0x00, 0x01]); // QDCOUNT 1
        m.extend_from_slice(&[0x00, 0x00]); // ANCOUNT 0
        m.extend_from_slice(&[0x00, 0x00]); // NSCOUNT 0
        m.extend_from_slice(&[0x00, 0x00]); // ARCOUNT 0
                                            // Question: "a.example." type A class IN
        m.extend_from_slice(&[
            0x01, b'a', 0x07, b'e', b'x', b'a', b'm', b'p', b'l', b'e', 0x00,
        ]);
        m.extend_from_slice(&[0x00, 0x01]); // type A
        m.extend_from_slice(&[0x00, 0x01]); // class IN
        m
    }

    #[test]
    fn readers_are_bounds_checked() {
        let b = [0x01u8, 0x02, 0x03];
        assert_eq!(read_u16_be(&b, 0).ok(), Some(0x0102));
        assert!(read_u16_be(&b, 2).is_err());
        assert!(read_u32_be(&b, 0).is_err());
        assert!(read_u8(&b, 3).is_err());
    }

    #[test]
    fn minimal_response_parts_sum_equals_wire_length() {
        let m = minimal_response();
        let a = parse_response_anatomy(&m, Some("UDP")).expect("parse");
        assert_eq!(a.wire_length, m.len());
        assert_eq!(a.parts_sum, a.wire_length);
        assert_eq!(a.header_bytes, HEADER_LEN);
        assert!(!a.tc);
        assert_eq!(a.question.name_bytes, 11); // "a" + "example" + root labels
        assert_eq!(a.question.total_bytes, 11 + 4); // + QTYPE + QCLASS
        assert_eq!(a.rrsig_count, 0);
        assert!(a.opt.is_none());
        assert_eq!(a.transport.as_deref(), Some("UDP"));
    }

    #[test]
    fn truncated_message_returns_err_with_offset() {
        let m = minimal_response();
        // Cut short inside the question.
        let err = parse_response_anatomy(&m[..14], None).expect_err("must fail");
        match err {
            DnsError::Truncated { offset, .. } => assert!(offset >= HEADER_LEN),
            other => panic!("expected Truncated, got {other:?}"),
        }
    }

    #[test]
    fn short_header_rejected() {
        let err = parse_response_anatomy(&[0u8; 4], None).expect_err("must fail");
        match err {
            DnsError::Truncated { offset, need } => {
                assert_eq!(offset, 0);
                assert_eq!(need, HEADER_LEN);
            }
            other => panic!("expected Truncated, got {other:?}"),
        }
    }

    /// Parse the captured full-signature headline response (24,438 B) and the
    /// condensed-proof headline response (658 B) — real bytes recorded from the
    /// live MTL pipeline — and check the sum-of-parts invariant holds for both.
    #[test]
    fn recorded_headline_responses_sum_to_wire_length() {
        for (hex, expect_len, expect_kind) in [
            (
                include_str!("fixtures/headline-full.hex"),
                24_438usize,
                "full",
            ),
            (
                include_str!("fixtures/headline-condensed.hex"),
                658,
                "condensed",
            ),
        ] {
            let bytes = decode_hex(hex.trim()).expect("fixture hex decodes");
            assert_eq!(bytes.len(), expect_len, "fixture length");
            let a = parse_response_anatomy(&bytes, Some("TCP")).expect("parse recorded response");
            assert_eq!(a.wire_length, expect_len);
            assert_eq!(a.parts_sum, a.wire_length, "parts must sum to wire length");
            assert!(a.rrsig_count > 0, "response must carry RRSIGs");
            // Every RRSIG's total_len is accounted inside a section; the first
            // one carries the MTL-Type lead byte for this algorithm.
            let first = &a.rrsig_entries[0];
            assert_eq!(
                first.mtl_kind(),
                Some(expect_kind),
                "lead byte of first RRSIG"
            );
            assert!(first.signature_len > 0);
        }
    }

    /// Decode a hex string into bytes (test helper; fixture files are hex).
    fn decode_hex(s: &str) -> Option<Vec<u8>> {
        let b = s.as_bytes();
        if !b.len().is_multiple_of(2) {
            return None;
        }
        b.chunks(2)
            .map(|pair| {
                let hi = (pair[0] as char).to_digit(16)?;
                let lo = (pair[1] as char).to_digit(16)?;
                Some(((hi << 4) | lo) as u8)
            })
            .collect()
    }
}
