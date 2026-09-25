//! DNS wire-format helpers for MTL-signed DNSSEC zones.
//!
//! This crate uses `hickory-dns` / `hickory-proto` for zone parsing, RRSIG
//! handling, and signature-size measurement. Implemented in **Batch 2+**.

/// Placeholder — replaced in Batch 2+ with DNS wire-format helpers.
#[doc(hidden)]
pub fn placeholder() -> &'static str {
    "climb-dns: DNS wire-format helpers (Batch 2+)"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stub_compiles() {
        assert_eq!(
            placeholder(),
            "climb-dns: DNS wire-format helpers (Batch 2+)"
        );
    }
}
