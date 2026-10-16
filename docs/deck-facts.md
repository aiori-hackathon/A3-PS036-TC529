# CLIMB deck quote sheet

Every number a slide deck or README may quote, read back from the recorded
artifacts at generation time. Each row carries where the value lives (JSON
pointer and source file), how it was obtained (evidence class, `n`/`k`) and
what it is compared against (baseline). Nothing here is typed in by hand.

- **Evidence classes.** `MEASURED` was read from a measurement artifact;
  `DERIVED` was computed from other recorded values (ratios, byte arithmetic);
  `ANALYTICAL` comes from a closed-form model; `EXTRAPOLATED` comes from the
  `N × per-message` identity applied to a measured per-message cost rather than
  a measurement over the whole batch, and is rendered with `≈`;
  `UNCLASSIFIED` carries no recorded class.
- **Baselines.** `A` plain SLH-DSA (one full signature per message, no ladder);
  `B` MTL without amortisation (one full MTL signature per message); `C` MTL
  amortised (one signed ladder per batch plus condensed paths); `n/a` is not an
  A/B/C comparison. Definitions are maintained in `docs/BASELINES.md`.
- **Scope.** This sheet reports empirical measurements only. It is not a
  security proof, a formal verification, or a certification of any kind.

## Wire bytes

> no wire rows recorded for: OneK, Large

| Label | Value | Unit | Evidence class | n / k | Baseline | JSON pointer | Source |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Small — wire total, A (plain SLH-DSA) | 785600 | B | MEASURED | n = 100 | A | /0/plain/total + /wire_size/0/plain_total_bytes | target/wire-report.json + target/climb-results.json |
| Small — wire total, B (MTL, no amortisation) | 815632 | B | MEASURED | n = 100 | B | /0/without_mtl_total + /wire_size/0/without_mtl_total_bytes | target/wire-report.json + target/climb-results.json |
| Small — wire total, C (MTL amortised) | 24424 | B | MEASURED | n = 100 | C | /0/with_mtl_total + /wire_size/0/with_mtl_total_bytes | target/wire-report.json + target/climb-results.json |
| Small — plain SLH-DSA signature length | 7856 | B | MEASURED | n = 100 | A | /0/plain/signature_len + /wire_size/0/plain_signature_len_bytes | target/wire-report.json + target/climb-results.json |
| Small — ladder bytes | 7992 | B | MEASURED | n = 100 | C | /0/ladder_len + /wire_size/0/ladder_len | target/wire-report.json + target/climb-results.json |
| Small — condensed signature, min | 108 | B | MEASURED | n = 100 | C | /0/condensed/min + /wire_size/0/condensed_min | target/wire-report.json + target/climb-results.json |
| Small — condensed signature, max | 172 | B | MEASURED | n = 100 | C | /0/condensed/max + /wire_size/0/condensed_max | target/wire-report.json + target/climb-results.json |
| Small — condensed signature, mean | 164.32 | B | MEASURED | n = 100 | C | /0/condensed/mean + /wire_size/0/condensed_mean | target/wire-report.json + target/climb-results.json |
| Small — wire reduction, C vs B | 97.006 | % | DERIVED | n = 100 | B | /0/reduction_percent + /wire_size/0/reduction_percent + /comparisons/0/wire_reduction_c_vs_b_percent | target/wire-report.json + target/climb-results.json |
| Medium — wire total, A (plain SLH-DSA) | 78560000 | B | MEASURED | n = 10000 | A | /1/plain/total + /wire_size/1/plain_total_bytes | target/wire-report.json + target/climb-results.json |
| Medium — wire total, B (MTL, no amortisation) | 83295296 | B | MEASURED | n = 10000 | B | /1/without_mtl_total + /wire_size/1/without_mtl_total_bytes | target/wire-report.json + target/climb-results.json |
| Medium — wire total, C (MTL amortised) | 2743352 | B | MEASURED | n = 10000 | C | /1/with_mtl_total + /wire_size/1/with_mtl_total_bytes | target/wire-report.json + target/climb-results.json |
| Medium — plain SLH-DSA signature length | 7856 | B | MEASURED | n = 10000 | A | /1/plain/signature_len + /wire_size/1/plain_signature_len_bytes | target/wire-report.json + target/climb-results.json |
| Medium — ladder bytes | 8056 | B | MEASURED | n = 10000 | C | /1/ladder_len + /wire_size/1/ladder_len | target/wire-report.json + target/climb-results.json |
| Medium — condensed signature, min | 140 | B | MEASURED | n = 10000 | C | /1/condensed/min + /wire_size/1/condensed_min | target/wire-report.json + target/climb-results.json |
| Medium — condensed signature, max | 284 | B | MEASURED | n = 10000 | C | /1/condensed/max + /wire_size/1/condensed_max | target/wire-report.json + target/climb-results.json |
| Medium — condensed signature, mean | 273.53 | B | MEASURED | n = 10000 | C | /1/condensed/mean + /wire_size/1/condensed_mean | target/wire-report.json + target/climb-results.json |
| Medium — wire reduction, C vs B | 96.706 | % | DERIVED | n = 10000 | B | /1/reduction_percent + /wire_size/1/reduction_percent + /comparisons/2/wire_reduction_c_vs_b_percent | target/wire-report.json + target/climb-results.json |
| Small — wire reduction, C vs A | 96.891 | % | DERIVED | n = 100 | A | /comparisons/0/wire_reduction_c_vs_a_percent | target/climb-results.json |
| Medium — wire reduction, C vs A | 96.508 | % | DERIVED | n = 10000 | A | /comparisons/2/wire_reduction_c_vs_a_percent | target/climb-results.json |
| Overall wire reduction (mean across sizes) | 96.856 | % | DERIVED | n = 2 | B | /derived/overall_wire_reduction_percent | target/climb-results.json |
| Largest wire reduction (across sizes) | 97.006 | % | DERIVED | n = 2 | B | /derived/largest_wire_reduction_percent | target/climb-results.json |

## Signing time

| Label | Value | Unit | Evidence class | n / k | Baseline | JSON pointer | Source |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Small — signing per batch, C (MTL amortised) | 595.8 | ms | MEASURED | n = 100 | C | /benchmark/signing/0/mean_ns | target/climb-results.json |
| Small — signing per message, C (MTL amortised) | 6.0 | ms | DERIVED | n = 100 | C | /benchmark/signing/0/mean_ns | target/climb-results.json |
| Small — signing per batch, B (MTL, no amortisation) | 1.21 | min | MEASURED | n = 100 | B | /benchmark/signing/1/mean_ns | target/climb-results.json |
| Small — signing per message, B (MTL, no amortisation) | 726.0 | ms | DERIVED | n = 100 | B | /benchmark/signing/1/mean_ns | target/climb-results.json |
| OneK — signing per batch, C (MTL amortised) | 716.2 | ms | MEASURED | n = 1000 | C | /benchmark/signing/2/mean_ns | target/climb-results.json |
| OneK — signing per message, C (MTL amortised) | 716.2 | us | DERIVED | n = 1000 | C | /benchmark/signing/2/mean_ns | target/climb-results.json |
| OneK — signing per batch, B (MTL, no amortisation) | 11.47 | min | MEASURED | n = 1000 | B | /benchmark/signing/3/mean_ns | target/climb-results.json |
| OneK — signing per message, B (MTL, no amortisation) | 688.2 | ms | DERIVED | n = 1000 | B | /benchmark/signing/3/mean_ns | target/climb-results.json |
| Medium — signing per batch, C (MTL amortised) | 716.5 | ms | MEASURED | n = 10000 | C | /benchmark/signing/4/mean_ns | target/climb-results.json |
| Medium — signing per message, C (MTL amortised) | 71.6 | us | DERIVED | n = 10000 | C | /benchmark/signing/4/mean_ns | target/climb-results.json |
| Large — signing per batch, C (MTL amortised) | 6.70 | s | MEASURED | n = 1000000 | C | /benchmark/signing/5/mean_ns | target/climb-results.json |
| Large — signing per message, C (MTL amortised) | 6.7 | us | DERIVED | n = 1000000 | C | /benchmark/signing/5/mean_ns | target/climb-results.json |
| Small — signing per message, A (plain SLH-DSA) | 737.9 | ms | MEASURED | n = 1 | A | /plain/0/signing_mean_ns | target/climb-results.json |
| Small — signing per batch, A (plain SLH-DSA) | ≈1.23 | min | EXTRAPOLATED | k = 1 | A | /plain/0/signing_mean_ns | target/climb-results.json |
| OneK — signing per message, A (plain SLH-DSA) | ≈713.9 | ms | EXTRAPOLATED | k = 2 | A | /plain/1/signing_mean_ns | target/climb-results.json |
| OneK — signing per batch, A (plain SLH-DSA) | ≈11.90 | min | EXTRAPOLATED | k = 2 | A | /plain/1/signing_mean_ns | target/climb-results.json |
| Medium — signing per message, A (plain SLH-DSA) | 689.9 | ms | MEASURED | n = 1 | A | /plain/2/signing_mean_ns | target/climb-results.json |
| Medium — signing per batch, A (plain SLH-DSA) | ≈1.92 | h | EXTRAPOLATED | k = 1 | A | /plain/2/signing_mean_ns | target/climb-results.json |
| Large — signing per message, A (plain SLH-DSA) | ≈713.9 | ms | EXTRAPOLATED | k = 2 | A | /plain/3/signing_mean_ns | target/climb-results.json |
| Large — signing per batch, A (plain SLH-DSA) | ≈198.31 | h | EXTRAPOLATED | k = 2 | A | /plain/3/signing_mean_ns | target/climb-results.json |
| Small — signing speed-up, C vs A | 123.9 | × | DERIVED | n = 100 | A | /comparisons/0/signing_speedup_c_vs_a | target/climb-results.json |
| Small — signing speed-up, C vs B | 121.9 | × | DERIVED | n = 100 | B | /comparisons/0/signing_speedup_c_vs_b | target/climb-results.json |
| OneK — signing speed-up, C vs A | ≈996.8 | × | EXTRAPOLATED | n = 1000 | A | /comparisons/1/signing_speedup_c_vs_a | target/climb-results.json |
| OneK — signing speed-up, C vs B | ≈961.0 | × | EXTRAPOLATED | n = 1000 | B | /comparisons/1/signing_speedup_c_vs_b | target/climb-results.json |
| Medium — signing speed-up, C vs A | ≈9629 | × | EXTRAPOLATED | n = 10000 | A | /comparisons/2/signing_speedup_c_vs_a | target/climb-results.json |
| Medium — signing speed-up, C vs B | ≈9869 | × | EXTRAPOLATED | n = 10000 | B | /comparisons/2/signing_speedup_c_vs_b | target/climb-results.json |
| Large — signing speed-up, C vs A | ≈106598 | × | EXTRAPOLATED | n = 1000000 | A | /comparisons/3/signing_speedup_c_vs_a | target/climb-results.json |
| Large — signing speed-up, C vs B | ≈105582 | × | EXTRAPOLATED | n = 1000000 | B | /comparisons/3/signing_speedup_c_vs_b | target/climb-results.json |

## Verifying time

| Label | Value | Unit | Evidence class | n / k | Baseline | JSON pointer | Source |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Small — verifying per batch, C (trust cached ladder) | 515.5 | us | MEASURED | n = 100 | C | /benchmark/verifying/0/mean_ns | target/climb-results.json |
| Small — verifying per message, C (trust cached ladder) | 5.2 | us | DERIVED | n = 100 | C | /benchmark/verifying/0/mean_ns | target/climb-results.json |
| Small — verifying per batch, C (ladder re-validated) | 63.0 | ms | MEASURED | n = 100 | C | /benchmark/verifying/1/mean_ns | target/climb-results.json |
| Small — verifying per message, C (ladder re-validated) | 629.9 | us | DERIVED | n = 100 | C | /benchmark/verifying/1/mean_ns | target/climb-results.json |
| Small — verifying per batch, B (MTL, no amortisation) | 60.0 | ms | MEASURED | n = 100 | B | /benchmark/verifying/2/mean_ns | target/climb-results.json |
| Small — verifying per message, B (MTL, no amortisation) | 599.8 | us | DERIVED | n = 100 | B | /benchmark/verifying/2/mean_ns | target/climb-results.json |
| Medium — verifying per batch, C (trust cached ladder) | 86.9 | ms | MEASURED | n = 10000 | C | /benchmark/verifying/3/mean_ns | target/climb-results.json |
| Medium — verifying per message, C (trust cached ladder) | 8.7 | us | DERIVED | n = 10000 | C | /benchmark/verifying/3/mean_ns | target/climb-results.json |
| Medium — verifying per batch, C (ladder re-validated) | 6.06 | s | MEASURED | n = 10000 | C | /benchmark/verifying/4/mean_ns | target/climb-results.json |
| Medium — verifying per message, C (ladder re-validated) | 605.9 | us | DERIVED | n = 10000 | C | /benchmark/verifying/4/mean_ns | target/climb-results.json |
| Medium — verifying per batch, B (MTL, no amortisation) | 5.87 | s | MEASURED | n = 10000 | B | /benchmark/verifying/5/mean_ns | target/climb-results.json |
| Medium — verifying per message, B (MTL, no amortisation) | 587.2 | us | DERIVED | n = 10000 | B | /benchmark/verifying/5/mean_ns | target/climb-results.json |
| Large — verifying per batch, C (trust cached ladder) | 12.37 | s | MEASURED | n = 1000000 | C | /benchmark/verifying/6/mean_ns | target/climb-results.json |
| Large — verifying per message, C (trust cached ladder) | 12.4 | us | DERIVED | n = 1000000 | C | /benchmark/verifying/6/mean_ns | target/climb-results.json |
| Small — verifying per message, A (plain SLH-DSA) | 736.5 | us | MEASURED | n = 1 | A | /plain/0/verifying_mean_ns | target/climb-results.json |
| Small — verifying per batch, A (plain SLH-DSA) | ≈73.6 | ms | EXTRAPOLATED | k = 1 | A | /plain/0/verifying_mean_ns | target/climb-results.json |
| OneK — verifying per message, A (plain SLH-DSA) | ≈743.7 | us | EXTRAPOLATED | k = 2 | A | /plain/1/verifying_mean_ns | target/climb-results.json |
| OneK — verifying per batch, A (plain SLH-DSA) | ≈743.7 | ms | EXTRAPOLATED | k = 2 | A | /plain/1/verifying_mean_ns | target/climb-results.json |
| Medium — verifying per message, A (plain SLH-DSA) | 750.9 | us | MEASURED | n = 1 | A | /plain/2/verifying_mean_ns | target/climb-results.json |
| Medium — verifying per batch, A (plain SLH-DSA) | ≈7.51 | s | EXTRAPOLATED | k = 1 | A | /plain/2/verifying_mean_ns | target/climb-results.json |
| Large — verifying per message, A (plain SLH-DSA) | ≈743.7 | us | EXTRAPOLATED | k = 2 | A | /plain/3/verifying_mean_ns | target/climb-results.json |
| Large — verifying per batch, A (plain SLH-DSA) | ≈12.40 | min | EXTRAPOLATED | k = 2 | A | /plain/3/verifying_mean_ns | target/climb-results.json |
| Small — verifying speed-up (trust cached), C vs A | 142.9 | × | DERIVED | n = 100 | A | /comparisons/0/verifying_speedup_cached_vs_a | target/climb-results.json |
| Small — verifying speed-up (ladder re-validated), C vs A | 1.17 | × | DERIVED | n = 100 | A | /comparisons/0/verifying_speedup_cold_vs_a | target/climb-results.json |
| OneK — verifying speed-up (trust cached), C vs A | ≈85.09 | × | EXTRAPOLATED | n = 1000 | A | /comparisons/1/verifying_speedup_cached_vs_a | target/climb-results.json |
| OneK — verifying speed-up (ladder re-validated), C vs A | ≈1.20 | × | EXTRAPOLATED | n = 1000 | A | /comparisons/1/verifying_speedup_cold_vs_a | target/climb-results.json |
| Medium — verifying speed-up (trust cached), C vs A | ≈86.37 | × | EXTRAPOLATED | n = 10000 | A | /comparisons/2/verifying_speedup_cached_vs_a | target/climb-results.json |
| Medium — verifying speed-up (ladder re-validated), C vs A | ≈1.24 | × | EXTRAPOLATED | n = 10000 | A | /comparisons/2/verifying_speedup_cold_vs_a | target/climb-results.json |
| Large — verifying speed-up (trust cached), C vs A | ≈60.11 | × | EXTRAPOLATED | n = 1000000 | A | /comparisons/3/verifying_speedup_cached_vs_a | target/climb-results.json |
| Large — verifying speed-up (ladder re-validated), C vs A | ≈1.20 | × | EXTRAPOLATED | n = 1000000 | A | /comparisons/3/verifying_speedup_cold_vs_a | target/climb-results.json |
| Average trust-cached verify speed-up | 388.2 | × | DERIVED | n = 3 | n/a | /derived/overall_verifying_trust_cached_speedup_avg | target/climb-results.json |

## Memory

> measurement method: isolated-process

| Label | Value | Unit | Evidence class | n / k | Baseline | JSON pointer | Source |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Small — resident median (measured) | 4206592 | B | MEASURED | n = 7 | n/a | /runs/0/median_bytes + /memory/measured/0/median_bytes | target/memory-report.json + target/climb-results.json |
| Small — resident min (measured) | 4165632 | B | MEASURED | n = 7 | n/a | /runs/0/min_bytes + /memory/measured/0/measured_range_min_bytes | target/memory-report.json + target/climb-results.json |
| Small — resident max (measured) | 4251648 | B | MEASURED | n = 7 | n/a | /runs/0/max_bytes + /memory/measured/0/measured_range_max_bytes | target/memory-report.json + target/climb-results.json |
| Small — resident IQR (measured) | 65536 | B | MEASURED | n = 7 | n/a | /runs/0/iqr_bytes + /memory/measured/0/iqr_bytes | target/memory-report.json + target/climb-results.json |
| Medium — resident median (measured) | 14118912 | B | MEASURED | n = 7 | n/a | /runs/1/median_bytes + /memory/measured/1/median_bytes | target/memory-report.json + target/climb-results.json |
| Medium — resident min (measured) | 14012416 | B | MEASURED | n = 7 | n/a | /runs/1/min_bytes + /memory/measured/1/measured_range_min_bytes | target/memory-report.json + target/climb-results.json |
| Medium — resident max (measured) | 14188544 | B | MEASURED | n = 7 | n/a | /runs/1/max_bytes + /memory/measured/1/measured_range_max_bytes | target/memory-report.json + target/climb-results.json |
| Medium — resident IQR (measured) | 122880 | B | MEASURED | n = 7 | n/a | /runs/1/iqr_bytes + /memory/measured/1/iqr_bytes | target/memory-report.json + target/climb-results.json |
| Small — peak median (measured) | 4206592 | B | MEASURED | n = 7 | n/a | /memory/measured/0/measured_peak_bytes | target/climb-results.json |
| Small — measurement samples | 7 | — | MEASURED | n = 7 | n/a | /memory/measured/0/samples | target/climb-results.json |
| Small — analytical, libMTL pages | 2097152 | B | ANALYTICAL | — | n/a | /memory/measured/0/libmtl_analytical_bytes | target/climb-results.json |
| Small — analytical, harness buffers | 27656 | B | ANALYTICAL | — | n/a | /memory/measured/0/harness_buffers_bytes | target/climb-results.json |
| Small — analytical, dataset buffers | 53600 | B | ANALYTICAL | — | n/a | /memory/measured/0/dataset_buffers_bytes | target/climb-results.json |
| Small — analytical, keypair | 186 | B | ANALYTICAL | — | n/a | /memory/measured/0/keypair_bytes | target/climb-results.json |
| Small — analytical total | 2178594 | B | ANALYTICAL | — | n/a | /memory/measured/0/analytical_total_bytes | target/climb-results.json |
| Small — residual (measured − analytical) | 2027998 | B | DERIVED | n = 7 | n/a | /memory/measured/0/residual_bytes | target/climb-results.json |
| Small — measured / analytical ratio | 1.93 | × | DERIVED | n = 7 | n/a | /memory/measured/0/measured_analytical_total_ratio | target/climb-results.json |
| Medium — peak median (measured) | 14516224 | B | MEASURED | n = 7 | n/a | /memory/measured/1/measured_peak_bytes | target/climb-results.json |
| Medium — measurement samples | 7 | — | MEASURED | n = 7 | n/a | /memory/measured/1/samples | target/climb-results.json |
| Medium — analytical, libMTL pages | 2097152 | B | ANALYTICAL | — | n/a | /memory/measured/1/libmtl_analytical_bytes | target/climb-results.json |
| Medium — analytical, harness buffers | 3063384 | B | ANALYTICAL | — | n/a | /memory/measured/1/harness_buffers_bytes | target/climb-results.json |
| Medium — analytical, dataset buffers | 5360000 | B | ANALYTICAL | — | n/a | /memory/measured/1/dataset_buffers_bytes | target/climb-results.json |
| Medium — analytical, keypair | 186 | B | ANALYTICAL | — | n/a | /memory/measured/1/keypair_bytes | target/climb-results.json |
| Medium — analytical total | 10520722 | B | ANALYTICAL | — | n/a | /memory/measured/1/analytical_total_bytes | target/climb-results.json |
| Medium — residual (measured − analytical) | 3598190 | B | DERIVED | n = 7 | n/a | /memory/measured/1/residual_bytes | target/climb-results.json |
| Medium — measured / analytical ratio | 1.34 | × | DERIVED | n = 7 | n/a | /memory/measured/1/measured_analytical_total_ratio | target/climb-results.json |
| Analytical model — tree page size | 1048576 | B | ANALYTICAL | — | n/a | /memory/analytical_model/tree_page_size_bytes | target/climb-results.json |
| Analytical model — tree max pages | 8192 | — | ANALYTICAL | — | n/a | /memory/analytical_model/tree_max_pages | target/climb-results.json |
| Analytical model — randomizer pages | 8192 | — | ANALYTICAL | — | n/a | /memory/analytical_model/randomizer_pages | target/climb-results.json |
| Analytical model — hash size | 16 | B | ANALYTICAL | — | n/a | /memory/analytical_model/hash_size_bytes | target/climb-results.json |

## Ladder verification

| Label | Value | Unit | Evidence class | n / k | Baseline | JSON pointer | Source |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Ladder, N=1 — rung count | 1 | — | MEASURED | n = 1 | n/a | /sizes/0/rung_count + /ladder/sizes/0/rung_count | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=1 — ladder buffer bytes | 68 | B | MEASURED | n = 1 | n/a | /sizes/0/ladder_buffer_len + /ladder/sizes/0/ladder_buffer_len | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=1 — signed ladder bytes | 7928 | B | MEASURED | n = 1 | n/a | /sizes/0/signed_ladder_len + /ladder/sizes/0/signed_ladder_len | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=1 — condensed min | 76 | B | MEASURED | n = 1 | n/a | /sizes/0/condensed/min + /ladder/sizes/0/condensed_min | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=1 — condensed max | 76 | B | MEASURED | n = 1 | n/a | /sizes/0/condensed/max + /ladder/sizes/0/condensed_max | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=1 — condensed mean | 76.00 | B | MEASURED | n = 1 | n/a | /sizes/0/condensed/mean + /ladder/sizes/0/condensed_mean | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=2 — rung count | 1 | — | MEASURED | n = 2 | n/a | /sizes/1/rung_count + /ladder/sizes/1/rung_count | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=2 — ladder buffer bytes | 68 | B | MEASURED | n = 2 | n/a | /sizes/1/ladder_buffer_len + /ladder/sizes/1/ladder_buffer_len | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=2 — signed ladder bytes | 7928 | B | MEASURED | n = 2 | n/a | /sizes/1/signed_ladder_len + /ladder/sizes/1/signed_ladder_len | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=2 — condensed min | 92 | B | MEASURED | n = 2 | n/a | /sizes/1/condensed/min + /ladder/sizes/1/condensed_min | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=2 — condensed max | 92 | B | MEASURED | n = 2 | n/a | /sizes/1/condensed/max + /ladder/sizes/1/condensed_max | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=2 — condensed mean | 92.00 | B | MEASURED | n = 2 | n/a | /sizes/1/condensed/mean + /ladder/sizes/1/condensed_mean | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=4 — rung count | 1 | — | MEASURED | n = 4 | n/a | /sizes/2/rung_count + /ladder/sizes/2/rung_count | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=4 — ladder buffer bytes | 68 | B | MEASURED | n = 4 | n/a | /sizes/2/ladder_buffer_len + /ladder/sizes/2/ladder_buffer_len | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=4 — signed ladder bytes | 7928 | B | MEASURED | n = 4 | n/a | /sizes/2/signed_ladder_len + /ladder/sizes/2/signed_ladder_len | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=4 — condensed min | 108 | B | MEASURED | n = 4 | n/a | /sizes/2/condensed/min + /ladder/sizes/2/condensed_min | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=4 — condensed max | 108 | B | MEASURED | n = 4 | n/a | /sizes/2/condensed/max + /ladder/sizes/2/condensed_max | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=4 — condensed mean | 108.00 | B | MEASURED | n = 4 | n/a | /sizes/2/condensed/mean + /ladder/sizes/2/condensed_mean | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=5 — rung count | 2 | — | MEASURED | n = 5 | n/a | /sizes/3/rung_count + /ladder/sizes/3/rung_count | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=5 — ladder buffer bytes | 100 | B | MEASURED | n = 5 | n/a | /sizes/3/ladder_buffer_len + /ladder/sizes/3/ladder_buffer_len | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=5 — signed ladder bytes | 7960 | B | MEASURED | n = 5 | n/a | /sizes/3/signed_ladder_len + /ladder/sizes/3/signed_ladder_len | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=5 — condensed min | 76 | B | MEASURED | n = 5 | n/a | /sizes/3/condensed/min + /ladder/sizes/3/condensed_min | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=5 — condensed max | 108 | B | MEASURED | n = 5 | n/a | /sizes/3/condensed/max + /ladder/sizes/3/condensed_max | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=5 — condensed mean | 101.60 | B | MEASURED | n = 5 | n/a | /sizes/3/condensed/mean + /ladder/sizes/3/condensed_mean | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=13 — rung count | 3 | — | MEASURED | n = 13 | n/a | /sizes/4/rung_count + /ladder/sizes/4/rung_count | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=13 — ladder buffer bytes | 132 | B | MEASURED | n = 13 | n/a | /sizes/4/ladder_buffer_len + /ladder/sizes/4/ladder_buffer_len | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=13 — signed ladder bytes | 7992 | B | MEASURED | n = 13 | n/a | /sizes/4/signed_ladder_len + /ladder/sizes/4/signed_ladder_len | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=13 — condensed min | 76 | B | MEASURED | n = 13 | n/a | /sizes/4/condensed/min + /ladder/sizes/4/condensed_min | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=13 — condensed max | 124 | B | MEASURED | n = 13 | n/a | /sizes/4/condensed/max + /ladder/sizes/4/condensed_max | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=13 — condensed mean | 115.38 | B | MEASURED | n = 13 | n/a | /sizes/4/condensed/mean + /ladder/sizes/4/condensed_mean | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=1 — matches the closed-form model | yes | — | DERIVED | n = 1 | n/a | /ladder/sizes/0/model_matches | target/climb-results.json |
| Ladder, N=2 — matches the closed-form model | yes | — | DERIVED | n = 2 | n/a | /ladder/sizes/1/model_matches | target/climb-results.json |
| Ladder, N=4 — matches the closed-form model | yes | — | DERIVED | n = 4 | n/a | /ladder/sizes/2/model_matches | target/climb-results.json |
| Ladder, N=5 — matches the closed-form model | yes | — | DERIVED | n = 5 | n/a | /ladder/sizes/3/model_matches | target/climb-results.json |
| Ladder, N=13 — matches the closed-form model | yes | — | DERIVED | n = 13 | n/a | /ladder/sizes/4/model_matches | target/climb-results.json |
| Ladder — model matches every measured size | yes | — | DERIVED | — | n/a | /ladder/all_match | target/climb-results.json |
| Ladder — algorithm | SLH-DSA-SHA2-128s-MTL-SHA2-128 | — | MEASURED | — | n/a | /ladder/algorithm | target/climb-results.json |
| Ladder — recorded at | 2026-10-15T22:26:16+05:30 | — | MEASURED | — | n/a | /ladder/recorded_at | target/climb-results.json |

## DNS live

> per-section response anatomy: not recorded in this capture artifact

| Label | Value | Unit | Evidence class | n / k | Baseline | JSON pointer | Source |
| --- | --- | --- | --- | --- | --- | --- | --- |
| DNS, climb.example. SOA [full] — response bytes | 24465 | B | MEASURED | — | B | /queries/0/response_bytes | data/dns-capture.recorded.json |
| DNS, climb.example. SOA [full] — answer RRSIG bytes | 8069 | B | MEASURED | — | B | /queries/0/answer_rrsig_bytes | data/dns-capture.recorded.json |
| DNS, www.climb.example. A [full] — response bytes | 24438 | B | MEASURED | — | B | /queries/1/response_bytes + /dns_pipeline/full_signature_bytes_www_a | data/dns-capture.recorded.json + target/climb-results.json |
| DNS, www.climb.example. A [full] — answer RRSIG bytes | 8069 | B | MEASURED | — | B | /queries/1/answer_rrsig_bytes | data/dns-capture.recorded.json |
| DNS, mail.climb.example. MX [full] — response bytes | 32572 | B | MEASURED | — | B | /queries/2/response_bytes | data/dns-capture.recorded.json |
| DNS, mail.climb.example. MX [full] — answer RRSIG bytes | 8069 | B | MEASURED | — | B | /queries/2/answer_rrsig_bytes | data/dns-capture.recorded.json |
| DNS, climb.example. SOA [condensed] — response bytes | 685 | B | MEASURED | — | C | /queries/3/response_bytes | data/dns-capture.recorded.json |
| DNS, climb.example. SOA [condensed] — answer RRSIG bytes | 141 | B | MEASURED | — | C | /queries/3/answer_rrsig_bytes | data/dns-capture.recorded.json |
| DNS, www.climb.example. A [condensed] — response bytes | 658 | B | MEASURED | — | C | /queries/4/response_bytes + /dns_pipeline/condensed_proof_bytes_www_a | data/dns-capture.recorded.json + target/climb-results.json |
| DNS, www.climb.example. A [condensed] — answer RRSIG bytes | 141 | B | MEASURED | — | C | /queries/4/answer_rrsig_bytes | data/dns-capture.recorded.json |
| DNS, mail.climb.example. MX [condensed] — response bytes | 864 | B | MEASURED | — | C | /queries/5/response_bytes | data/dns-capture.recorded.json |
| DNS, mail.climb.example. MX [condensed] — answer RRSIG bytes | 141 | B | MEASURED | — | C | /queries/5/answer_rrsig_bytes | data/dns-capture.recorded.json |
| DNS, climb.example. SOA [unbound] — response bytes | 8207 | B | MEASURED | — | B | /queries/6/response_bytes | data/dns-capture.recorded.json |
| DNS, climb.example. SOA [unbound] — answer RRSIG bytes | 8069 | B | MEASURED | — | B | /queries/6/answer_rrsig_bytes | data/dns-capture.recorded.json |
| DNS, www.climb.example. A [unbound] — response bytes | 8176 | B | MEASURED | — | B | /queries/7/response_bytes | data/dns-capture.recorded.json |
| DNS, www.climb.example. A [unbound] — answer RRSIG bytes | 8069 | B | MEASURED | — | B | /queries/7/answer_rrsig_bytes | data/dns-capture.recorded.json |
| DNS, mail.climb.example. MX [unbound] — response bytes | 8180 | B | MEASURED | — | B | /queries/8/response_bytes | data/dns-capture.recorded.json |
| DNS, mail.climb.example. MX [unbound] — answer RRSIG bytes | 8069 | B | MEASURED | — | B | /queries/8/answer_rrsig_bytes | data/dns-capture.recorded.json |
| DNS — algorithm | SLH-DSA-SHA2-128s-MTL-SHA2-128 | — | MEASURED | — | n/a | /algorithm + /dns_pipeline/algorithm | data/dns-capture.recorded.json + target/climb-results.json |
| DNS — recorded at | 2026-10-12T14:24:26+05:30 | — | MEASURED | — | n/a | /recorded_at + /dns_pipeline/recorded_at | data/dns-capture.recorded.json + target/climb-results.json |
| DNS, www.climb.example. A — response reduction, condensed vs full | 97.307 | % | DERIVED | — | B | /dns_pipeline/reduction_percent_www_a | target/climb-results.json |
| DNS — capture source | recorded-fixture | — | MEASURED | — | n/a | /dns_pipeline/source | target/climb-results.json |

## Environment

| Label | Value | Unit | Evidence class | n / k | Baseline | JSON pointer | Source |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Environment — timestamp | 2026-10-15T22:26:02+05:30 | — | MEASURED | — | n/a | /timestamp + /environment/timestamp | environment.json + target/climb-results.json |
| Environment — rustc version | rustc 1.98.1 (48a229cea 2026-09-01) | — | MEASURED | — | n/a | /toolchain/rustc_version + /environment/toolchain/rustc_version | environment.json + target/climb-results.json |
| Environment — cargo version | cargo 1.98.1 (797e8a9bc 2026-08-05) | — | MEASURED | — | n/a | /toolchain/cargo_version + /environment/toolchain/cargo_version | environment.json + target/climb-results.json |
| Environment — kernel release | 7.2.5-3-omarchy | — | MEASURED | — | n/a | /system/kernel_release + /environment/system/kernel_release | environment.json + target/climb-results.json |
| Environment — CPU model | 11th Gen Intel(R) Core(TM) i3-1115G4 @ 3.00GHz | — | MEASURED | — | n/a | /cpu/model_name + /environment/cpu/model_name | environment.json + target/climb-results.json |
| Environment — CPU governor | powersave | — | MEASURED | — | n/a | /benchmark_runtime/cpu_governor + /environment/benchmark_runtime/cpu_governor | environment.json + target/climb-results.json |
| Environment — pin expression | taskset -c 2 | — | MEASURED | — | n/a | /benchmark_runtime/pin_expression + /environment/benchmark_runtime/pin_expression | environment.json + target/climb-results.json |
| Environment — logical CPUs | 4 | — | MEASURED | — | n/a | /cpu/logical_cpus + /environment/cpu/logical_cpus | environment.json + target/climb-results.json |
| Environment — physical cores | 2 | — | MEASURED | — | n/a | /cpu/physical_cores + /environment/cpu/physical_cores | environment.json + target/climb-results.json |
| Environment — pinned CPU | 2 | — | MEASURED | — | n/a | /benchmark_runtime/pin_cpu + /environment/benchmark_runtime/pin_cpu | environment.json + target/climb-results.json |
| Submodule third_party/MTL — commit SHA | 53ce25dcc35c15e051c5263e41aaeaede18a8aa7 | — | MEASURED | — | n/a | /submodules/0/sha + /environment/submodules/0/sha | environment.json + target/climb-results.json |
| Submodule third_party/liboqs — commit SHA | 5a1a854b0dc9f2141bdc771c555ee60c37950183 | — | MEASURED | — | n/a | /submodules/1/sha + /environment/submodules/1/sha | environment.json + target/climb-results.json |
| Repository — commit SHA | c5cf32249a73749fbd451f698bbf011403ab8ecf | — | UNCLASSIFIED | — | n/a | n/a | git rev-parse HEAD |

## Quality

| Label | Value | Unit | Evidence class | n / k | Baseline | JSON pointer | Source |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Workspace tests — passed | 531 | — | MEASURED | n = 28 | n/a | n/a | cargo test --workspace |
| Workspace tests — failed | 0 | — | MEASURED | n = 28 | n/a | n/a | cargo test --workspace |
| Workspace tests — ignored | 1 | — | MEASURED | n = 28 | n/a | n/a | cargo test --workspace |
| Workspace test targets (summary lines) | 28 | — | MEASURED | n = 28 | n/a | n/a | cargo test --workspace |

---

## Footer

- Rows rendered: 188 (from 262 collected facts; 74 duplicate copies merged).
- Evidence classes (rendered rows): MEASURED: 114, ANALYTICAL: 14, EXTRAPOLATED: 24, DERIVED: 35; UNCLASSIFIED: 1.
- UNCLASSIFIED rows: Repository — commit SHA.
- Inconsistencies: none.
