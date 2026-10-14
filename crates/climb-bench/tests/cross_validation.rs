//! Cross-validation tests (Phase 11.2).
//!
//! These close three cross-crate invariants that were deliberately *deferred*
//! when their owning crates were written, each because the owning crate must
//! not depend on the other:
//!
//! 1. **`MAX_MESSAGE_LEN` equality** (flagged in Phase 4, `climb-dns`).
//!    `climb-dns` deliberately does not depend on `climb-mtl` (a normal
//!    dependency would drag bindgen + native-lib linking into every pure-DNS
//!    consumer and break `cargo test -p climb-dns` on machines without
//!    `.mtl-install`).  So it duplicated the bound as a documented constant.
//!    This test — which lives in `climb-bench`, the one crate that already
//!    depends on both — asserts the two constants are equal.
//!
//! 2. **`DatasetSize` equality** (flagged in Phase 9, `climb-analytical`).
//!    `climb-analytical` deliberately does not depend on `climb-dns`, so it
//!    duplicated the enum.  This test asserts every variant's leaf count
//!    matches between the two enums.
//!
//! 3. **`DatasetSize` value bounds** — the `DatasetSize` variants' counts must
//!    stay within a `u16`-addressable message (the native signing path takes
//!    message lengths as `uint16_t`), which ties the duplicated enum back to
//!    the duplicated length constant.
//!
//! `climb-bench` is the correct home because it is the only crate in the
//! workspace permitted to depend on `climb-dns`, `climb-analytical`, and
//! `climb-mtl` simultaneously — the integration layer is exactly where
//! cross-crate contracts belong.

// ---------------------------------------------------------------------------
// 1. MAX_MESSAGE_LEN cross-validation (Phase 4 flag)
// ---------------------------------------------------------------------------

/// The message-length bound is defined independently in `climb-dns` (as a
/// documented mirror) and `climb-mtl` (the origin, matching libMTL's C
/// `uint16_t` length parameter).  They must agree.
///
/// If they drift, `climb_dns::generate_dataset` could emit a message that
/// `climb_mtl::sign_batch` rejects (or worse, that silently truncates at the
/// FFI boundary).  This is the assertion Phase 4 recorded as Batch 3's job.
#[test]
fn max_message_len_matches_between_dns_and_mtl() {
    assert_eq!(
        climb_dns::MAX_MESSAGE_LEN,
        climb_mtl::MAX_MESSAGE_LEN,
        "climb-dns and climb-mtl disagree on the maximum message length; \
         climb-dns::MAX_MESSAGE_LEN is a documented mirror of \
         climb-mtl::MAX_MESSAGE_LEN and must be kept equal"
    );
    // Both are documented as mirroring libMTL's `uint16_t` message-length
    // parameter, so both must equal `u16::MAX`.
    assert_eq!(climb_dns::MAX_MESSAGE_LEN, u16::MAX as usize);
}

/// A generated message must never exceed the *other* crate's bound either.
///
/// `max_message_len_matches_between_dns_and_mtl` proves the constants match;
/// this test proves the generated data respects them, so the two facts
/// together rule out the drift scenario end-to-end.
#[test]
fn generated_messages_respect_mtl_message_bound() {
    let dataset = climb_dns::generate_dataset(climb_dns::DatasetSize::Small, 0x434C494D425F5635);
    for (i, msg) in dataset.iter().enumerate() {
        assert!(!msg.is_empty(), "message {i} is empty");
        assert!(
            msg.len() <= climb_mtl::MAX_MESSAGE_LEN,
            "message {i} len {} exceeds climb_mtl::MAX_MESSAGE_LEN {}",
            msg.len(),
            climb_mtl::MAX_MESSAGE_LEN
        );
    }
}

// ---------------------------------------------------------------------------
// 2. DatasetSize cross-validation (Phase 9 flag)
// ---------------------------------------------------------------------------

/// Every `climb_dns::DatasetSize` variant's leaf count must equal the
/// corresponding `climb_analytical::DatasetSize` variant's leaf count.
///
/// The two enums are duplicated by design (neither crate depends on the
/// other); this is the cross-check Phase 9 deferred to Batch 3.
#[test]
fn dataset_size_counts_match_between_dns_and_analytical() {
    use climb_analytical::DatasetSize as Analytical;
    use climb_dns::DatasetSize as Dns;

    let pairs = [
        (Dns::Small, Analytical::Small),
        (Dns::OneK, Analytical::OneK),
        (Dns::Medium, Analytical::Medium),
        (Dns::Large, Analytical::Large),
    ];

    for (dns, analytical) in pairs {
        assert_eq!(
            dns.count(),
            analytical.count(),
            "DatasetSize mismatch: climb-dns {dns:?} = {} but \
             climb-analytical {analytical:?} = {}",
            dns.count(),
            analytical.count()
        );
    }
}

/// The message-length bound is a property of each individual message's wire
/// length, **not** of the dataset's leaf count.  This guards against the
/// confusion that the bound somehow limits how many leaves a ladder may hold:
/// `DatasetSize::Large` legitimately has a million leaves, each of which is
/// itself well under `MAX_MESSAGE_LEN`.
#[test]
fn dataset_leaf_counts_are_independent_of_message_length_bound() {
    use climb_dns::{generate_dataset, DatasetSize};

    assert!(
        DatasetSize::Large.count() > climb_dns::MAX_MESSAGE_LEN,
        "Large's leaf count ({}) should exceed the per-message byte bound ({}); \
         they are different quantities",
        DatasetSize::Large.count(),
        climb_dns::MAX_MESSAGE_LEN
    );

    for size in [
        DatasetSize::Small,
        DatasetSize::OneK,
        DatasetSize::Medium,
        DatasetSize::Large,
    ] {
        assert!(size.count() > 0, "{size:?} must be non-empty");
    }

    for size in [DatasetSize::Small, DatasetSize::OneK, DatasetSize::Medium] {
        let dataset = generate_dataset(size, 0x434C494D425F5635);
        assert_eq!(dataset.len(), size.count());
        for (i, msg) in dataset.iter().enumerate() {
            assert!(
                msg.len() <= climb_dns::MAX_MESSAGE_LEN,
                "{size:?} message {i} len {} exceeds MAX_MESSAGE_LEN {}",
                msg.len(),
                climb_dns::MAX_MESSAGE_LEN
            );
        }
    }
}

// ---------------------------------------------------------------------------
// 3. Wire-size ↔ analytical-memory cross-check (Phase 11.3 sanity pass)
// ---------------------------------------------------------------------------

/// The wire-size model (Phase 8) and the analytical memory model (Phase 9)
/// share exactly one quantum: the Merkle hash size (`climb_analytical::HASH_SIZE`,
/// 16 B for SHA2-128).  A condensed signature is dominated by its Merkle
/// authentication path, which gains one hash per extra level of tree depth, so
/// its maximum length must grow by exactly `HASH_SIZE` per additional
/// `floor(log2 N)` level.  Pinning the measured growth to the analytical
/// constant is the concrete link between the two measurement families.
#[test]
fn condensed_growth_quantum_equals_analytical_hash_size() {
    use climb_analytical::HASH_SIZE;
    use climb_dns::DatasetSize;

    let seed = climb_bench::DEFAULT_WIRE_SEED;
    let small = climb_bench::measure_wire_sizes(DatasetSize::Small, seed).expect("small wire");
    let medium = climb_bench::measure_wire_sizes(DatasetSize::Medium, seed).expect("medium wire");

    let levels_small = (DatasetSize::Small.count() as f64).log2().floor() as usize;
    let levels_medium = (DatasetSize::Medium.count() as f64).log2().floor() as usize;
    assert_eq!(levels_small, 6, "floor(log2 100)");
    assert_eq!(levels_medium, 13, "floor(log2 10000)");

    let measured_growth = medium.condensed.max - small.condensed.max;
    let expected_growth = (levels_medium - levels_small) * HASH_SIZE;
    assert_eq!(
        measured_growth,
        expected_growth,
        "max condensed length grew {measured_growth} B from Small to Medium, \
         but {levels_medium}-{levels_small}={} tree levels at {HASH_SIZE} B/hash \
         predict {expected_growth} B",
        levels_medium - levels_small
    );
}

/// The Phase 9 analytical model and the Phase 10 measured RSS delta must agree
/// to within an order of magnitude at every blueprint size.  They are *not*
/// required to be tight: the analytical model counts whole 1 MiB page storage
/// (an upper bound on resident bytes, since libMTL allocates pages lazily),
/// while the measured delta additionally contains the Rust-side `SignOutput`
/// and allocator slack (which dominates at Large).  The effects pull in
/// opposite directions, so the defensible invariant is order-of-magnitude
/// agreement on the reported mean — never a forced equality.
///
/// The Phase 10 numbers-of-record are transcribed so the test documents the
/// historical comparison without re-running an expensive RSS measurement.
#[test]
fn analytical_memory_agrees_with_measured_rss_within_an_order_of_magnitude() {
    use climb_analytical::estimate_memory_footprint;

    // Phase 10 §10.B RSS deltas (bytes): (leaf_count, run_a, run_b).
    let phase10 = [
        (100usize, 1_466_368u64, 307_200u64),
        (10_000, 3_563_520, 946_176),
        (1_000_000, 561_254_400, 80_027_648),
    ];

    for (n, run_a, run_b) in phase10 {
        let analytical = estimate_memory_footprint(n).total_memory_bytes as u64;
        let measured_mean = (run_a + run_b) / 2;
        let ratio = measured_mean as f64 / analytical as f64;
        assert!(
            (0.1..=10.0).contains(&ratio),
            "n={n}: mean measured {measured_mean} B vs analytical {analytical} B \
             (ratio {ratio:.3}) — drift beyond one order of magnitude means \
             one of the two models is no longer plausible"
        );
    }
}

/// Sanity relation between the wire-size model (Phase 8) and the analytical
/// memory model (Phase 9).
///
/// The Phase 9 analytical estimate models *page bytes*, not condensed-signature
/// bytes.  The two are not equal and are not supposed to be.  What must hold is
/// the coarse ordering that makes both models physically plausible: the tree
/// page storage (rounded up to whole 1 MiB pages) is at least the raw hash-array
/// storage implied by the node count, and the randomizer storage is at least
/// `N` hash-sized randomizers.
///
/// This asserts the analytical model's internal accounting, which is the
/// quantity a reader cross-checks against measured RSS (Phase 10) and against
/// the per-leaf wire bytes (Phase 8).  It is deliberately a *sanity* check, not
/// an equality: see `the dev notes` §11.3 for the documented comparison of the
/// two measurement families.
#[test]
fn analytical_memory_model_is_internally_consistent() {
    use climb_analytical::{estimate_memory_footprint, HASH_SIZE, MTL_TREE_PAGE_SIZE};

    for n in [100usize, 1_000, 10_000, 1_000_000] {
        let est = estimate_memory_footprint(n);

        // Tree page bytes must cover the raw hash array (page rounding is up).
        let raw_tree_bytes = est.tree_nodes_max * HASH_SIZE;
        assert!(
            est.tree_memory_bytes >= raw_tree_bytes,
            "n={n}: tree pages {} B < raw hash array {} B",
            est.tree_memory_bytes,
            raw_tree_bytes
        );

        // Randomizer page bytes must cover N hash-sized randomizers.
        let raw_rand_bytes = n * HASH_SIZE;
        assert!(
            est.randomizer_memory_bytes >= raw_rand_bytes,
            "n={n}: randomizer pages {} B < raw randomizers {} B",
            est.randomizer_memory_bytes,
            raw_rand_bytes
        );

        // Pages are whole.
        assert_eq!(est.tree_memory_bytes % MTL_TREE_PAGE_SIZE, 0);
        assert_eq!(est.randomizer_memory_bytes % MTL_TREE_PAGE_SIZE, 0);

        // Total is the sum of the two parts.
        assert_eq!(
            est.total_memory_bytes,
            est.tree_memory_bytes + est.randomizer_memory_bytes
        );
    }
}

/// The analytical dataset-buffer term must match what the DNS encoder actually
/// allocates: hickory's `Message::to_vec` starts from a 512-byte buffer, so the
/// sum of the stored messages' capacities equals `n * DATASET_MESSAGE_BUFFER_BYTES`.
/// If hickory ever changes that pre-allocation this fails, and the model
/// constant must be updated rather than the measurement being tuned.
#[test]
fn dataset_message_capacity_matches_analytical_buffer_term() {
    use climb_analytical::DATASET_MESSAGE_BUFFER_BYTES;
    use climb_dns::{generate_dataset, DatasetSize};

    for size in [DatasetSize::Small, DatasetSize::OneK, DatasetSize::Medium] {
        let dataset = generate_dataset(size, 0x434C494D425F5636);
        let capacity_sum: usize = dataset.iter().map(Vec::capacity).sum();
        assert_eq!(
            capacity_sum,
            dataset.len() * DATASET_MESSAGE_BUFFER_BYTES,
            "{size:?}: message capacity sum {capacity_sum} != n * {DATASET_MESSAGE_BUFFER_BYTES}",
        );
    }
}

/// The analytical keypair term must match the length of a freshly generated key
/// image; a libMTL serialization change fails this rather than silently
/// mis-modelling the keypair stage.
#[test]
fn keypair_term_matches_generated_key_image_length() {
    use climb_analytical::KEY_IMAGE_BYTES;

    let keypair = climb_mtl::MtlKeyPair::generate().expect("keygen");
    assert_eq!(keypair.key_image_len(), KEY_IMAGE_BYTES);
}
