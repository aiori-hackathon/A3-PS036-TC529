//! The benchmark-group inventory that `xtask bench-all` schedules.
//!
//! Each entry names one Criterion group defined by the `climb-bench` suite,
//! the dataset size it covers, which bench binary defines it, and whether a
//! single iteration is infeasibly slow on the reference hardware.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatasetSize {
    Small,
    OneK,
    Medium,
    Large,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BenchTarget {
    Signing,
    Verifying,
    Classical,
    Primitives,
}

/// One Criterion benchmark group defined by the `climb-bench` suite.
pub struct GroupSpec {
    pub name: &'static str,
    pub size: DatasetSize,
    pub target: BenchTarget,
    /// True when a single iteration takes hours to days on the reference laptop.
    pub slow: bool,
}

pub const GROUPS: &[GroupSpec] = &[
    GroupSpec {
        name: "signing_small_with_mtl",
        size: DatasetSize::Small,
        target: BenchTarget::Signing,
        slow: false,
    },
    GroupSpec {
        name: "signing_small_without_mtl",
        size: DatasetSize::Small,
        target: BenchTarget::Signing,
        slow: false,
    },
    GroupSpec {
        name: "signing_onek_with_mtl",
        size: DatasetSize::OneK,
        target: BenchTarget::Signing,
        slow: false,
    },
    // 1,000 × ~670 ms/sig = ~670 s per iteration (~111 min for a 10-sample run).
    GroupSpec {
        name: "signing_onek_without_mtl",
        size: DatasetSize::OneK,
        target: BenchTarget::Signing,
        slow: true,
    },
    GroupSpec {
        name: "signing_medium_with_mtl",
        size: DatasetSize::Medium,
        target: BenchTarget::Signing,
        slow: false,
    },
    // 10,000 x ~670 ms/sig = ~1.9 h per iteration (~19 h for a 10-sample run).
    GroupSpec {
        name: "signing_medium_without_mtl",
        size: DatasetSize::Medium,
        target: BenchTarget::Signing,
        slow: true,
    },
    GroupSpec {
        name: "signing_large_with_mtl",
        size: DatasetSize::Large,
        target: BenchTarget::Signing,
        slow: false,
    },
    // 1,000,000 x ~670 ms/sig = ~7.8 days per iteration (~78 days for a 10-sample run).
    GroupSpec {
        name: "signing_large_without_mtl",
        size: DatasetSize::Large,
        target: BenchTarget::Signing,
        slow: true,
    },
    // Configuration A: one plain SLH-DSA signature per message, measured on a
    // single message from the seeded dataset. Large is not scheduled — its
    // per-message cost is extrapolated to `n x mean` from the Small/Medium
    // samples (k = 2), not run.
    GroupSpec {
        name: "signing_small_plain_slh_dsa",
        size: DatasetSize::Small,
        target: BenchTarget::Signing,
        slow: false,
    },
    GroupSpec {
        name: "signing_medium_plain_slh_dsa",
        size: DatasetSize::Medium,
        target: BenchTarget::Signing,
        slow: false,
    },
    GroupSpec {
        name: "verifying_small_with_mtl_trust_true",
        size: DatasetSize::Small,
        target: BenchTarget::Verifying,
        slow: false,
    },
    GroupSpec {
        name: "verifying_small_with_mtl_trust_false",
        size: DatasetSize::Small,
        target: BenchTarget::Verifying,
        slow: false,
    },
    GroupSpec {
        name: "verifying_small_without_mtl",
        size: DatasetSize::Small,
        target: BenchTarget::Verifying,
        slow: false,
    },
    GroupSpec {
        name: "verifying_onek_with_mtl_trust_true",
        size: DatasetSize::OneK,
        target: BenchTarget::Verifying,
        slow: false,
    },
    GroupSpec {
        name: "verifying_onek_with_mtl_trust_false",
        size: DatasetSize::OneK,
        target: BenchTarget::Verifying,
        slow: false,
    },
    GroupSpec {
        name: "verifying_onek_without_mtl",
        size: DatasetSize::OneK,
        target: BenchTarget::Verifying,
        slow: false,
    },
    GroupSpec {
        name: "verifying_medium_with_mtl_trust_true",
        size: DatasetSize::Medium,
        target: BenchTarget::Verifying,
        slow: false,
    },
    GroupSpec {
        name: "verifying_medium_with_mtl_trust_false",
        size: DatasetSize::Medium,
        target: BenchTarget::Verifying,
        slow: false,
    },
    GroupSpec {
        name: "verifying_medium_without_mtl",
        size: DatasetSize::Medium,
        target: BenchTarget::Verifying,
        slow: false,
    },
    GroupSpec {
        name: "verifying_large_with_mtl_trust_true",
        size: DatasetSize::Large,
        target: BenchTarget::Verifying,
        slow: false,
    },
    GroupSpec {
        name: "verifying_small_plain_slh_dsa",
        size: DatasetSize::Small,
        target: BenchTarget::Verifying,
        slow: false,
    },
    GroupSpec {
        name: "verifying_medium_plain_slh_dsa",
        size: DatasetSize::Medium,
        target: BenchTarget::Verifying,
        slow: false,
    },
    GroupSpec {
        name: "classical_ed25519_small",
        size: DatasetSize::Small,
        target: BenchTarget::Classical,
        slow: false,
    },
    GroupSpec {
        name: "classical_ed25519_medium",
        size: DatasetSize::Medium,
        target: BenchTarget::Classical,
        slow: false,
    },
    GroupSpec {
        name: "classical_ecdsa_p256_small",
        size: DatasetSize::Small,
        target: BenchTarget::Classical,
        slow: false,
    },
    GroupSpec {
        name: "classical_ecdsa_p256_medium",
        size: DatasetSize::Medium,
        target: BenchTarget::Classical,
        slow: false,
    },
    GroupSpec {
        name: "classical_rsa2048_small",
        size: DatasetSize::Small,
        target: BenchTarget::Classical,
        slow: false,
    },
    GroupSpec {
        name: "classical_rsa2048_medium",
        size: DatasetSize::Medium,
        target: BenchTarget::Classical,
        slow: false,
    },
    GroupSpec {
        name: "signing_primitives",
        size: DatasetSize::Small,
        target: BenchTarget::Primitives,
        slow: false,
    },
];
