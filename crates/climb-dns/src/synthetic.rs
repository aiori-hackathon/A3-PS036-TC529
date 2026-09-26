//! Deterministic synthetic DNS RRset generation.
//!
//! One [`SyntheticRecord`] is one DNS response carrying exactly one RRset:
//! a single owner name, a single record type, and 1..=4 resource records
//! sharing the same TTL.  The RRset type varies **across** messages (mix of
//! A, AAAA, MX, TXT, NS), not within them.  Every field is drawn from a
//! seeded [`ChaCha8Rng`] so the output is byte-reproducible.
//!
//! Zone layout:
//! - `zone<k>.example.` — apex (used as owner for MX / NS RRSets).
//! - `host<j>.zone<k>.example.` — leaf host (owner for A / AAAA / TXT).
//! - `mx<n>.zone<k>.example.` — mail exchange target.
//! - `ns<n>.zone<k>.example.` — nameserver target.

use std::net::{Ipv4Addr, Ipv6Addr};

use hickory_proto::op::{Message, MessageType, OpCode, Query};
use hickory_proto::rr::rdata::{A, AAAA, MX, NS, TXT};
use hickory_proto::rr::{Name, RData, Record, RecordType};
use rand::RngExt;

use crate::dataset::MAX_MESSAGE_LEN;
use crate::error::DnsError;

/// Number of synthetic zones.
const NUM_ZONES: usize = 10;

/// Hosts per zone.
const HOSTS_PER_ZONE: usize = 10;

/// MX records per zone.
const MX_PER_ZONE: usize = 4;

/// NS records per zone.
const NS_PER_ZONE: usize = 2;

/// Fixed TTL set for realistic variation.
const TTL_SET: [u32; 4] = [300, 900, 3600, 86400];

// ── name helpers ────────────────────────────────────────────────────────

/// `zone{k}.example.`
fn zone_name(zone_idx: usize) -> Name {
    let s = format!("zone{}.example.", zone_idx);
    // Guaranteed by construction: digits, dots, `.example.` suffix — always
    // a valid FQDN.
    Name::from_ascii(&s).expect("synthetic zone name is always valid")
}

/// `host{host_idx}.zone{zone_idx}.example.`
fn host_name(zone_idx: usize, host_idx: usize) -> Name {
    let s = format!("host{}.zone{}.example.", host_idx, zone_idx);
    Name::from_ascii(&s).expect("synthetic host name is always valid")
}

/// `mx{n}.zone{zone_idx}.example.`
fn mx_name(zone_idx: usize, n: usize) -> Name {
    let s = format!("mx{}.zone{}.example.", n, zone_idx);
    Name::from_ascii(&s).expect("synthetic MX name is always valid")
}

/// `ns{n}.zone{zone_idx}.example.`
fn ns_name(zone_idx: usize, n: usize) -> Name {
    let s = format!("ns{}.zone{}.example.", n, zone_idx);
    Name::from_ascii(&s).expect("synthetic NS name is always valid")
}

// ── TXT payload ─────────────────────────────────────────────────────────

/// Generate a short random printable-ASCII string (1..=32 chars, 0x20..0x7e).
fn random_txt_string(rng: &mut impl RngExt) -> String {
    let len = rng.random_range(1_usize..=32);
    let bytes: Vec<u8> = (0..len).map(|_| rng.random_range(0x20_u8..=0x7e)).collect();
    // Guaranteed by construction: every byte is in the ASCII printable range
    // 0x20..0x7e, which is a subset of UTF-8.
    String::from_utf8(bytes).expect("printable ASCII bytes are valid UTF-8")
}

// ── RDATA generation ────────────────────────────────────────────────────

/// Generate one `RData` value for the given record type.
///
/// All randomness is drawn from `rng` in a fixed order so the output is
/// deterministic for a given RNG state.
fn generate_rdata(rng: &mut impl RngExt, rt: RecordType, zone_idx: usize) -> RData {
    match rt {
        RecordType::A => RData::A(A::from(Ipv4Addr::new(
            rng.random(),
            rng.random(),
            rng.random(),
            rng.random(),
        ))),
        RecordType::AAAA => {
            let segments: [u16; 8] = rng.random();
            RData::AAAA(AAAA::from(Ipv6Addr::from(segments)))
        }
        RecordType::MX => {
            let pref: u16 = rng.random();
            let exchange = mx_name(zone_idx, rng.random_range(0..MX_PER_ZONE));
            RData::MX(MX::new(pref, exchange))
        }
        RecordType::TXT => {
            let n_strs = rng.random_range(1_usize..=3);
            let strings: Vec<String> = (0..n_strs).map(|_| random_txt_string(rng)).collect();
            RData::TXT(TXT::new(strings))
        }
        RecordType::NS => {
            let ns = ns_name(zone_idx, rng.random_range(0..NS_PER_ZONE));
            RData::NS(NS(ns))
        }
        _ => unreachable!("only five supported record types (A/AAAA/MX/TXT/NS)"),
    }
}

/// Map a `u8` index to one of the five supported [`RecordType`]s.
fn record_type_from_index(idx: u8) -> RecordType {
    match idx {
        0 => RecordType::A,
        1 => RecordType::AAAA,
        2 => RecordType::MX,
        3 => RecordType::TXT,
        4 => RecordType::NS,
        _ => unreachable!("index in 0..5"),
    }
}

// ── SyntheticRecord ─────────────────────────────────────────────────────

/// A synthetic DNS response carrying one RRset.
///
/// One RRset = one owner name, one record type, one TTL, and 1..=4 resource
/// records.  The wire-format bytes are deterministic for a given RNG state
/// and are fed to `climb_mtl::sign_batch` as one "message".
#[derive(Debug, Clone)]
pub struct SyntheticRecord {
    message_id: u16,
    owner: Name,
    record_type: RecordType,
    ttl: u32,
    rdata_values: Vec<RData>,
}

impl SyntheticRecord {
    /// Draw one record deterministically from `rng`.
    ///
    /// Draw order (the fixed contract for reproducibility):
    /// 1. `message_id`
    /// 2. zone index
    /// 3. RRset record type (A / AAAA / MX / TXT / NS)
    /// 4. owner name (apex for MX/NS; host for A/AAAA/TXT)
    /// 5. TTL
    /// 6. answer count (1..=4)
    /// 7. that many RDATA values
    #[must_use]
    pub fn generate(rng: &mut impl RngExt) -> Self {
        let message_id: u16 = rng.random();
        let zone_idx = rng.random_range(0..NUM_ZONES);

        let rt = record_type_from_index(rng.random_range(0_u8..5));

        // MX and NS RRsets live at the zone apex; everything else lives at
        // a leaf host name.
        let owner = match rt {
            RecordType::MX | RecordType::NS => zone_name(zone_idx),
            _ => host_name(zone_idx, rng.random_range(0..HOSTS_PER_ZONE)),
        };

        // One TTL for the whole RRset (per RFC, all records in an RRset
        // MUST share the same TTL).
        let ttl = TTL_SET[rng.random_range(0..TTL_SET.len())];

        let n = rng.random_range(1_usize..=4);
        let rdata_values: Vec<RData> = (0..n).map(|_| generate_rdata(rng, rt, zone_idx)).collect();

        Self {
            message_id,
            owner,
            record_type: rt,
            ttl,
            rdata_values,
        }
    }

    /// The RRset's record type.
    #[must_use]
    pub fn record_type(&self) -> RecordType {
        self.record_type
    }

    /// Number of resource records in the RRset (1..=4).
    #[must_use]
    pub fn answer_count(&self) -> usize {
        self.rdata_values.len()
    }

    /// The QNAME / owner name of the RRset.
    #[must_use]
    pub fn owner(&self) -> &Name {
        &self.owner
    }

    /// Build the hickory-proto [`Message`].
    #[must_use]
    pub fn to_message(&self) -> Message {
        let mut message = Message::new(self.message_id, MessageType::Response, OpCode::Query);
        message.add_query(Query::query(self.owner.clone(), self.record_type));
        for rdata in &self.rdata_values {
            message.add_answer(Record::from_rdata(
                self.owner.clone(),
                self.ttl,
                rdata.clone(),
            ));
        }
        message
    }

    /// Encode to DNS wire bytes, enforcing the `[1, MAX_MESSAGE_LEN]` invariant.
    ///
    /// # Errors
    ///
    /// Returns [`DnsError::Encode`] if hickory-proto rejects the message.
    /// Returns [`DnsError::EmptyMessage`] or [`DnsError::MessageTooLong`] if
    /// the encoded output violates the size invariant (these are construction
    /// bugs — our fixed small record shapes always encode correctly).
    pub fn to_wire(&self) -> Result<Vec<u8>, DnsError> {
        // Encoding a validly-constructed Message to a Vec<u8> is infallible
        // in practice, but we propagate errors just in case.
        let bytes = self.to_message().to_vec()?;
        if bytes.is_empty() {
            return Err(DnsError::EmptyMessage);
        }
        if bytes.len() > MAX_MESSAGE_LEN {
            return Err(DnsError::MessageTooLong {
                len: bytes.len(),
                max: MAX_MESSAGE_LEN,
            });
        }
        Ok(bytes)
    }
}

// ── unit tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;

    #[test]
    fn generate_is_deterministic() {
        let mut rng1 = ChaCha8Rng::seed_from_u64(42);
        let mut rng2 = ChaCha8Rng::seed_from_u64(42);

        let r1 = SyntheticRecord::generate(&mut rng1);
        let r2 = SyntheticRecord::generate(&mut rng2);

        assert_eq!(r1.message_id, r2.message_id);
        assert_eq!(r1.owner, r2.owner);
        assert_eq!(r1.record_type, r2.record_type);
        assert_eq!(r1.ttl, r2.ttl);
        assert_eq!(r1.rdata_values.len(), r2.rdata_values.len());

        let w1 = r1.to_wire().expect("encode should succeed");
        let w2 = r2.to_wire().expect("encode should succeed");
        assert_eq!(w1, w2);
    }

    #[test]
    fn answer_count_in_range() {
        let mut rng = ChaCha8Rng::seed_from_u64(0);
        for _ in 0..50 {
            let record = SyntheticRecord::generate(&mut rng);
            let n = record.answer_count();
            assert!((1..=4).contains(&n), "answer count {n} out of range");
        }
    }

    #[test]
    fn record_type_is_one_of_five() {
        let valid = [
            RecordType::A,
            RecordType::AAAA,
            RecordType::MX,
            RecordType::TXT,
            RecordType::NS,
        ];
        let mut rng = ChaCha8Rng::seed_from_u64(1);
        for _ in 0..50 {
            let record = SyntheticRecord::generate(&mut rng);
            assert!(
                valid.contains(&record.record_type()),
                "unexpected record type {:?}",
                record.record_type()
            );
        }
    }

    #[test]
    fn to_wire_non_empty_and_bounded() {
        let mut rng = ChaCha8Rng::seed_from_u64(2);
        for _ in 0..50 {
            let record = SyntheticRecord::generate(&mut rng);
            let wire = record.to_wire().expect("encode should succeed");
            assert!(!wire.is_empty(), "wire must be non-empty");
            assert!(
                wire.len() <= MAX_MESSAGE_LEN,
                "wire len {} > MAX_MESSAGE_LEN",
                wire.len()
            );
        }
    }

    #[test]
    fn different_seeds_produce_different_records() {
        let mut rng1 = ChaCha8Rng::seed_from_u64(42);
        let mut rng2 = ChaCha8Rng::seed_from_u64(99);

        let r1 = SyntheticRecord::generate(&mut rng1);
        let r2 = SyntheticRecord::generate(&mut rng2);

        let w1 = r1.to_wire().expect("encode should succeed");
        let w2 = r2.to_wire().expect("encode should succeed");
        assert_ne!(w1, w2, "different seeds must produce different output");
    }

    #[test]
    fn all_answers_share_owner() {
        // Given: a SerialRecord
        let mut rng = ChaCha8Rng::seed_from_u64(7);
        let record = SyntheticRecord::generate(&mut rng);
        let msg = record.to_message();

        // Then: every answer has the same owner name
        for rr in &msg.answers {
            assert_eq!(
                rr.name, record.owner,
                "all answers must share the owner name"
            );
        }
    }
}
