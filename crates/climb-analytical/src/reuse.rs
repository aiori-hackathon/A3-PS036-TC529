//! Pure cold/warm ladder-reuse model.
//!
//! When one signed MTL ladder is reused across `r` messages, the batch pays the
//! cold (full-ladder) verification once and the warm (trust-cached) verification
//! for the remaining `r - 1` messages, so the effective per-message time is
//! `(t_cold + (r - 1) * t_warm) / r`. On the wire the shared ladder replaces one
//! ladder per message, so the effective per-message bytes are
//! `s_cond + s_ladder / r` (a condensed path plus a share of the ladder). Both
//! reduce to the unamortised cost at `r = 1` and approach the warm / condensed
//! floor as `r` grows.
//!
//! The candidate reuse counts are enumerated, clipped to the batch size, and
//! deduplicated; the smallest candidate at or below each baseline is the
//! break-even `r*`. This module performs arithmetic over measured inputs only —
//! it introduces no new cryptographic construction.

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReuseInput {
    pub t_cold: f64,
    pub t_warm: f64,
    pub s_cond: f64,
    pub s_ladder: f64,
    pub batch_size: f64,
    pub baseline_time_ns: f64,
    pub baseline_bytes: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReusePoint {
    pub reuse: u64,
    pub effective_time_ns: f64,
    pub effective_bytes: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReuseModel {
    pub batch_size: u64,
    pub points: Vec<ReusePoint>,
    pub breakeven_reuse_time: Option<u64>,
    pub breakeven_reuse_bytes: Option<u64>,
    pub r_star_time: Option<u64>,
    pub r_star_bytes: Option<u64>,
    pub baseline_time_ns: f64,
    pub baseline_bytes: f64,
}

/// Small reuse candidate set: `1, 2, 5, 10, 100`, each clipped to `≤ batch_size` and deduped.
pub const REUSE_CANDIDATES_SMALL: [u64; 5] = [1, 2, 5, 10, 100];
/// Medium reuse candidate set: `1, 2, 5, 10, 100, 1000, 10000`, each clipped to `≤ batch_size`
/// and deduped.
pub const REUSE_CANDIDATES_MEDIUM: [u64; 7] = [1, 2, 5, 10, 100, 1000, 10000];

pub fn reuse_candidates(batch_size: u64) -> Vec<u64> {
    let candidates = if batch_size <= 100 {
        REUSE_CANDIDATES_SMALL.to_vec()
    } else {
        REUSE_CANDIDATES_MEDIUM.to_vec()
    };
    let mut out: Vec<u64> = candidates
        .into_iter()
        .filter(|&r| r <= batch_size)
        .collect();
    out.push(batch_size);
    out.sort_unstable();
    out.dedup();
    out
}

pub fn effective_time_ns(t_cold: f64, t_warm: f64, r: u64) -> f64 {
    if r == 0 {
        return t_cold;
    }
    (t_cold + ((r as f64) - 1.0) * t_warm) / (r as f64)
}

pub fn effective_bytes(s_cond: f64, s_ladder: f64, r: u64) -> f64 {
    if r == 0 {
        return s_cond + s_ladder;
    }
    s_cond + s_ladder / (r as f64)
}

pub fn reuse_model(input: ReuseInput) -> ReuseModel {
    let candidates = reuse_candidates(input.batch_size as u64);
    let mut points = Vec::with_capacity(candidates.len());
    for &r in &candidates {
        points.push(ReusePoint {
            reuse: r,
            effective_time_ns: effective_time_ns(input.t_cold, input.t_warm, r),
            effective_bytes: effective_bytes(input.s_cond, input.s_ladder, r),
        });
    }

    let breakeven_reuse_time = find_breakeven_time(&points, input.baseline_time_ns);
    let breakeven_reuse_bytes = find_breakeven_bytes(&points, input.baseline_bytes);

    ReuseModel {
        batch_size: input.batch_size as u64,
        points,
        breakeven_reuse_time,
        breakeven_reuse_bytes,
        r_star_time: breakeven_reuse_time,
        r_star_bytes: breakeven_reuse_bytes,
        baseline_time_ns: input.baseline_time_ns,
        baseline_bytes: input.baseline_bytes,
    }
}

fn find_breakeven_time(points: &[ReusePoint], baseline: f64) -> Option<u64> {
    for p in points {
        if p.effective_time_ns <= baseline + 1e-12 {
            return Some(p.reuse);
        }
    }
    None
}

fn find_breakeven_bytes(points: &[ReusePoint], baseline: f64) -> Option<u64> {
    for p in points {
        if p.effective_bytes <= baseline + 1e-12 {
            return Some(p.reuse);
        }
    }
    None
}

impl fmt::Display for ReuseModel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "ReuseModel (batch={})", self.batch_size)?;
        for p in &self.points {
            writeln!(
                f,
                "  r={}: t={:.3}ns, b={:.3}B",
                p.reuse, p.effective_time_ns, p.effective_bytes
            )?;
        }
        writeln!(
            f,
            "  breakeven_time={:?}, breakeven_bytes={:?}",
            self.breakeven_reuse_time, self.breakeven_reuse_bytes
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monotone_in_r() {
        let t_cold = 629867.6127;
        let t_warm = 5155.100973638213;
        let s_cond = 164.32;
        let s_ladder = 7992.0;
        let batch = 100;
        let baseline_t = 736473.4552875111;
        let baseline_b = 7856.0;

        let model = reuse_model(ReuseInput {
            t_cold,
            t_warm,
            s_cond,
            s_ladder,
            batch_size: batch as f64,
            baseline_time_ns: baseline_t,
            baseline_bytes: baseline_b,
        });

        let mut last_t = f64::INFINITY;
        for p in &model.points {
            assert!(p.effective_time_ns <= last_t + 1e-12);
            last_t = p.effective_time_ns;
        }

        let mut last_b = f64::INFINITY;
        for p in &model.points {
            assert!(p.effective_bytes <= last_b + 1e-12);
            last_b = p.effective_bytes;
        }
    }

    #[test]
    fn equals_cold_at_r1() {
        let t_cold = 629867.6127;
        let t_warm = 5155.100973638213;
        let s_cond = 164.32;
        let s_ladder = 7992.0;
        let r = 1;
        assert!((effective_time_ns(t_cold, t_warm, r) - t_cold).abs() < 1e-9);
        assert!((effective_bytes(s_cond, s_ladder, r) - (s_cond + s_ladder)).abs() < 1e-9);
    }

    #[test]
    fn tends_to_warm_scond_as_r_grows() {
        let t_cold = 629867.6127;
        let t_warm = 5155.100973638213;
        let s_cond = 164.32;
        let s_ladder = 7992.0;
        // The excess over the warm/s_cond floor is exactly the cold/one-ladder
        // cost spread over `r` messages, so shrinking `r`-fold must shrink the
        // gap by the same factor.
        let r = 1_000u64;
        let expected_time = t_warm + (t_cold - t_warm) / r as f64;
        let expected_bytes = s_cond + s_ladder / r as f64;
        assert!((effective_time_ns(t_cold, t_warm, r) - expected_time).abs() < 1e-9);
        assert!((effective_bytes(s_cond, s_ladder, r) - expected_bytes).abs() < 1e-9);
        // And both are strictly closer to the floor than the r=1 value is.
        assert!((effective_time_ns(t_cold, t_warm, r) - t_warm).abs() < t_cold - t_warm);
        assert!(effective_bytes(s_cond, s_ladder, r) - s_cond < s_ladder);
    }

    #[test]
    fn candidates_clip_and_dedup() {
        let c_small = reuse_candidates(10);
        assert_eq!(c_small, vec![1, 2, 5, 10]);
        let c_batch = reuse_candidates(50);
        assert_eq!(c_batch, vec![1, 2, 5, 10, 50]);
        let c_large = reuse_candidates(10000);
        assert!(c_large.contains(&1000));
        assert!(c_large.contains(&10000));
        assert!(c_large[0] == 1);
    }
}
