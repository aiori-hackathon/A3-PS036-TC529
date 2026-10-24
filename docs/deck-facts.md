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

> no wire rows recorded for: OneK

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
| Large — wire total, A (plain SLH-DSA) | 7856000000 | B | MEASURED | n = 1000000 | A | /2/plain/total + /wire_size/2/plain_total_bytes | target/wire-report.json + target/climb-results.json |
| Large — wire total, B (MTL, no amortisation) | 8487059712 | B | MEASURED | n = 1000000 | B | /2/without_mtl_total + /wire_size/2/without_mtl_total_bytes | target/wire-report.json + target/climb-results.json |
| Large — wire total, C (MTL amortised) | 367067832 | B | MEASURED | n = 1000000 | C | /2/with_mtl_total + /wire_size/2/with_mtl_total_bytes | target/wire-report.json + target/climb-results.json |
| Large — plain SLH-DSA signature length | 7856 | B | MEASURED | n = 1000000 | A | /2/plain/signature_len + /wire_size/2/plain_signature_len_bytes | target/wire-report.json + target/climb-results.json |
| Large — ladder bytes | 8120 | B | MEASURED | n = 1000000 | C | /2/ladder_len + /wire_size/2/ladder_len | target/wire-report.json + target/climb-results.json |
| Large — condensed signature, min | 172 | B | MEASURED | n = 1000000 | C | /2/condensed/min + /wire_size/2/condensed_min | target/wire-report.json + target/climb-results.json |
| Large — condensed signature, max | 380 | B | MEASURED | n = 1000000 | C | /2/condensed/max + /wire_size/2/condensed_max | target/wire-report.json + target/climb-results.json |
| Large — condensed signature, mean | 367.06 | B | MEASURED | n = 1000000 | C | /2/condensed/mean + /wire_size/2/condensed_mean | target/wire-report.json + target/climb-results.json |
| Large — wire reduction, C vs B | 95.675 | % | DERIVED | n = 1000000 | B | /2/reduction_percent + /wire_size/2/reduction_percent + /comparisons/3/wire_reduction_c_vs_b_percent | target/wire-report.json + target/climb-results.json |
| Small — wire reduction, C vs A | 96.891 | % | DERIVED | n = 100 | A | /comparisons/0/wire_reduction_c_vs_a_percent | target/climb-results.json |
| Medium — wire reduction, C vs A | 96.508 | % | DERIVED | n = 10000 | A | /comparisons/2/wire_reduction_c_vs_a_percent | target/climb-results.json |
| Large — wire reduction, C vs A | 95.328 | % | DERIVED | n = 1000000 | A | /comparisons/3/wire_reduction_c_vs_a_percent | target/climb-results.json |
| Overall wire reduction (mean across sizes) | 96.462 | % | DERIVED | n = 3 | B | /derived/overall_wire_reduction_percent | target/climb-results.json |
| Largest wire reduction (across sizes) | 97.006 | % | DERIVED | n = 3 | B | /derived/largest_wire_reduction_percent | target/climb-results.json |

## Signing time

> Large — signing speed-up, C vs A: spread pending

| Label | Value | Unit | Evidence class | n / k | Baseline | JSON pointer | Source |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Small — signing per batch, C (MTL amortised) | 586.8 | ms | MEASURED | n = 100 | C | /benchmark/signing/0/mean_ns | target/climb-results.json |
| Small — signing per message, C (MTL amortised) | 5.9 | ms | DERIVED | n = 100 | C | /benchmark/signing/0/mean_ns | target/climb-results.json |
| Small — signing per batch, B (MTL, no amortisation) | 520.5 | ms | MEASURED | n = 1 | B | /benchmark/signing/1/mean_ns | target/climb-results.json |
| Small — signing per message, B (MTL, no amortisation) | 520.5 | ms | DERIVED | n = 1 | B | /benchmark/signing/1/mean_ns | target/climb-results.json |
| OneK — signing per batch, C (MTL amortised) | 520.4 | ms | MEASURED | n = 1000 | C | /benchmark/signing/2/mean_ns | target/climb-results.json |
| OneK — signing per message, C (MTL amortised) | 520.4 | us | DERIVED | n = 1000 | C | /benchmark/signing/2/mean_ns | target/climb-results.json |
| OneK — signing per batch, B (MTL, no amortisation) | 516.6 | ms | MEASURED | n = 1 | B | /benchmark/signing/3/mean_ns | target/climb-results.json |
| OneK — signing per message, B (MTL, no amortisation) | 516.6 | ms | DERIVED | n = 1 | B | /benchmark/signing/3/mean_ns | target/climb-results.json |
| Medium — signing per batch, C (MTL amortised) | 550.2 | ms | MEASURED | n = 10000 | C | /benchmark/signing/4/mean_ns | target/climb-results.json |
| Medium — signing per message, C (MTL amortised) | 55.0 | us | DERIVED | n = 10000 | C | /benchmark/signing/4/mean_ns | target/climb-results.json |
| Medium — signing per batch, B (MTL, no amortisation) | 519.1 | ms | MEASURED | n = 1 | B | /benchmark/signing/5/mean_ns | target/climb-results.json |
| Medium — signing per message, B (MTL, no amortisation) | 519.1 | ms | DERIVED | n = 1 | B | /benchmark/signing/5/mean_ns | target/climb-results.json |
| Large — signing per batch, C (MTL amortised) | 4.33 | s | MEASURED | n = 1000000 | C | /benchmark/signing/6/mean_ns | target/climb-results.json |
| Large — signing per message, C (MTL amortised) | 4.3 | us | DERIVED | n = 1000000 | C | /benchmark/signing/6/mean_ns | target/climb-results.json |
| Large — signing per batch, B (MTL, no amortisation) | 523.3 | ms | MEASURED | n = 1 | B | /benchmark/signing/7/mean_ns | target/climb-results.json |
| Large — signing per message, B (MTL, no amortisation) | 523.3 | ms | DERIVED | n = 1 | B | /benchmark/signing/7/mean_ns | target/climb-results.json |
| Small — signing per message, A (plain SLH-DSA) | 518.2 | ms | MEASURED | n = 1 | A | /plain/0/signing_mean_ns | target/climb-results.json |
| Small — signing per batch, A (plain SLH-DSA) | ≈51.82 | s | EXTRAPOLATED | k = 1 | A | /plain/0/signing_mean_ns | target/climb-results.json |
| OneK — signing per message, A (plain SLH-DSA) | ≈517.9 | ms | EXTRAPOLATED | k = 2 | A | /plain/1/signing_mean_ns | target/climb-results.json |
| OneK — signing per batch, A (plain SLH-DSA) | ≈8.63 | min | EXTRAPOLATED | k = 2 | A | /plain/1/signing_mean_ns | target/climb-results.json |
| Medium — signing per message, A (plain SLH-DSA) | 517.5 | ms | MEASURED | n = 1 | A | /plain/2/signing_mean_ns | target/climb-results.json |
| Medium — signing per batch, A (plain SLH-DSA) | ≈1.44 | h | EXTRAPOLATED | k = 1 | A | /plain/2/signing_mean_ns | target/climb-results.json |
| Large — signing per message, A (plain SLH-DSA) | ≈517.9 | ms | EXTRAPOLATED | k = 2 | A | /plain/3/signing_mean_ns | target/climb-results.json |
| Large — signing per batch, A (plain SLH-DSA) | ≈143.85 | h | EXTRAPOLATED | k = 2 | A | /plain/3/signing_mean_ns | target/climb-results.json |
| Small — signing speed-up, C vs A (spread) | 88.31 | × | DERIVED | n = 5 | A | /comparisons/0/signing_speedup_c_vs_a | target/climb-results.json |
| Small — signing speed-up, C vs B | 88.70 | × | DERIVED | n = 100 | B | /comparisons/0/signing_speedup_c_vs_b | target/climb-results.json |
| OneK — signing speed-up, C vs A (spread) | ≈995.2 | × | EXTRAPOLATED | n = 5 | A | /comparisons/1/signing_speedup_c_vs_a | target/climb-results.json |
| OneK — signing speed-up, C vs B | ≈992.8 | × | EXTRAPOLATED | n = 1000 | B | /comparisons/1/signing_speedup_c_vs_b | target/climb-results.json |
| Medium — signing speed-up, C vs A (spread) | 9405 | × | DERIVED | n = 5 | A | /comparisons/2/signing_speedup_c_vs_a | target/climb-results.json |
| Medium — signing speed-up, C vs B | 9433 | × | DERIVED | n = 10000 | B | /comparisons/2/signing_speedup_c_vs_b | target/climb-results.json |
| Large — signing speed-up, C vs A | ≈119657 | × | EXTRAPOLATED | n = 1000000 | A | /comparisons/3/signing_speedup_c_vs_a | target/climb-results.json |
| Large — signing speed-up, C vs B | ≈120899 | × | EXTRAPOLATED | n = 1000000 | B | /comparisons/3/signing_speedup_c_vs_b | target/climb-results.json |

## Verifying time

> Small — verifying speed-up (trust cached), C vs A: spread pending

> OneK — verifying speed-up (trust cached), C vs A: spread pending

> Medium — verifying speed-up (trust cached), C vs A: spread pending

> Large — verifying speed-up (trust cached), C vs A: spread pending

| Label | Value | Unit | Evidence class | n / k | Baseline | JSON pointer | Source |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Small — verifying per batch, C (trust cached ladder) | 345.3 | us | MEASURED | n = 100 | C | /benchmark/verifying/0/mean_ns | target/climb-results.json |
| Small — verifying per message, C (trust cached ladder) | 3.5 | us | DERIVED | n = 100 | C | /benchmark/verifying/0/mean_ns | target/climb-results.json |
| Small — verifying per batch, C (ladder re-validated) | 150.1 | ms | MEASURED | n = 100 | C | /benchmark/verifying/1/mean_ns | target/climb-results.json |
| Small — verifying per message, C (ladder re-validated) | 1.5 | ms | DERIVED | n = 100 | C | /benchmark/verifying/1/mean_ns | target/climb-results.json |
| Small — verifying per batch, B (MTL, no amortisation) | 149.3 | ms | MEASURED | n = 100 | B | /benchmark/verifying/2/mean_ns | target/climb-results.json |
| Small — verifying per message, B (MTL, no amortisation) | 1.5 | ms | DERIVED | n = 100 | B | /benchmark/verifying/2/mean_ns | target/climb-results.json |
| Medium — verifying per batch, C (trust cached ladder) | 66.1 | ms | MEASURED | n = 10000 | C | /benchmark/verifying/3/mean_ns | target/climb-results.json |
| Medium — verifying per message, C (trust cached ladder) | 6.6 | us | DERIVED | n = 10000 | C | /benchmark/verifying/3/mean_ns | target/climb-results.json |
| Medium — verifying per batch, C (ladder re-validated) | 14.93 | s | MEASURED | n = 10000 | C | /benchmark/verifying/4/mean_ns | target/climb-results.json |
| Medium — verifying per message, C (ladder re-validated) | 1.5 | ms | DERIVED | n = 10000 | C | /benchmark/verifying/4/mean_ns | target/climb-results.json |
| Medium — verifying per batch, B (MTL, no amortisation) | 14.51 | s | MEASURED | n = 10000 | B | /benchmark/verifying/5/mean_ns | target/climb-results.json |
| Medium — verifying per message, B (MTL, no amortisation) | 1.5 | ms | DERIVED | n = 10000 | B | /benchmark/verifying/5/mean_ns | target/climb-results.json |
| Large — verifying per batch, C (trust cached ladder) | 16.89 | s | MEASURED | n = 1000000 | C | /benchmark/verifying/6/mean_ns | target/climb-results.json |
| Large — verifying per message, C (trust cached ladder) | 16.9 | us | DERIVED | n = 1000000 | C | /benchmark/verifying/6/mean_ns | target/climb-results.json |
| Small — verifying per message, A (plain SLH-DSA) | 1.4 | ms | MEASURED | n = 1 | A | /plain/0/verifying_mean_ns | target/climb-results.json |
| Small — verifying per batch, A (plain SLH-DSA) | ≈138.9 | ms | EXTRAPOLATED | k = 1 | A | /plain/0/verifying_mean_ns | target/climb-results.json |
| OneK — verifying per message, A (plain SLH-DSA) | ≈1.4 | ms | EXTRAPOLATED | k = 2 | A | /plain/1/verifying_mean_ns | target/climb-results.json |
| OneK — verifying per batch, A (plain SLH-DSA) | ≈1.41 | s | EXTRAPOLATED | k = 2 | A | /plain/1/verifying_mean_ns | target/climb-results.json |
| Medium — verifying per message, A (plain SLH-DSA) | 1.4 | ms | MEASURED | n = 1 | A | /plain/2/verifying_mean_ns | target/climb-results.json |
| Medium — verifying per batch, A (plain SLH-DSA) | ≈14.24 | s | EXTRAPOLATED | k = 1 | A | /plain/2/verifying_mean_ns | target/climb-results.json |
| Large — verifying per message, A (plain SLH-DSA) | ≈1.4 | ms | EXTRAPOLATED | k = 2 | A | /plain/3/verifying_mean_ns | target/climb-results.json |
| Large — verifying per batch, A (plain SLH-DSA) | ≈23.44 | min | EXTRAPOLATED | k = 2 | A | /plain/3/verifying_mean_ns | target/climb-results.json |
| Small — verifying speed-up (trust cached), C vs A | 402.2 | × | DERIVED | n = 100 | A | /comparisons/0/verifying_speedup_cached_vs_a | target/climb-results.json |
| Small — verifying speed-up (ladder re-validated), C vs A | 0.93 | × | DERIVED | n = 100 | A | /comparisons/0/verifying_speedup_cold_vs_a | target/climb-results.json |
| OneK — verifying speed-up (trust cached), C vs A | ≈156.5 | × | EXTRAPOLATED | n = 1000 | A | /comparisons/1/verifying_speedup_cached_vs_a | target/climb-results.json |
| OneK — verifying speed-up (ladder re-validated), C vs A | ≈0.94 | × | EXTRAPOLATED | n = 1000 | A | /comparisons/1/verifying_speedup_cold_vs_a | target/climb-results.json |
| Medium — verifying speed-up (trust cached), C vs A | 215.3 | × | DERIVED | n = 10000 | A | /comparisons/2/verifying_speedup_cached_vs_a | target/climb-results.json |
| Medium — verifying speed-up (ladder re-validated), C vs A | 0.95 | × | DERIVED | n = 10000 | A | /comparisons/2/verifying_speedup_cold_vs_a | target/climb-results.json |
| Large — verifying speed-up (trust cached), C vs A | ≈83.28 | × | EXTRAPOLATED | n = 1000000 | A | /comparisons/3/verifying_speedup_cached_vs_a | target/climb-results.json |
| Large — verifying speed-up (ladder re-validated), C vs A | ≈0.94 | × | EXTRAPOLATED | n = 1000000 | A | /comparisons/3/verifying_speedup_cold_vs_a | target/climb-results.json |
| Average trust-cached verify speed-up | 569.3 | × | DERIVED | n = 3 | n/a | /derived/overall_verifying_trust_cached_speedup_avg | target/climb-results.json |

## Memory

> measurement method: isolated-process

| Label | Value | Unit | Evidence class | n / k | Baseline | JSON pointer | Source |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Small — resident median (measured) | 4145152 | B | MEASURED | n = 7 | n/a | /runs/0/median_bytes + /memory/measured/0/median_bytes | target/memory-report.json + target/climb-results.json |
| Small — resident min (measured) | 4087808 | B | MEASURED | n = 7 | n/a | /runs/0/min_bytes + /memory/measured/0/measured_range_min_bytes | target/memory-report.json + target/climb-results.json |
| Small — resident max (measured) | 4227072 | B | MEASURED | n = 7 | n/a | /runs/0/max_bytes + /memory/measured/0/measured_range_max_bytes | target/memory-report.json + target/climb-results.json |
| Small — resident IQR (measured) | 94208 | B | MEASURED | n = 7 | n/a | /runs/0/iqr_bytes + /memory/measured/0/iqr_bytes | target/memory-report.json + target/climb-results.json |
| Medium — resident median (measured) | 14032896 | B | MEASURED | n = 7 | n/a | /runs/1/median_bytes + /memory/measured/1/median_bytes | target/memory-report.json + target/climb-results.json |
| Medium — resident min (measured) | 13987840 | B | MEASURED | n = 7 | n/a | /runs/1/min_bytes + /memory/measured/1/measured_range_min_bytes | target/memory-report.json + target/climb-results.json |
| Medium — resident max (measured) | 14213120 | B | MEASURED | n = 7 | n/a | /runs/1/max_bytes + /memory/measured/1/measured_range_max_bytes | target/memory-report.json + target/climb-results.json |
| Medium — resident IQR (measured) | 94208 | B | MEASURED | n = 7 | n/a | /runs/1/iqr_bytes + /memory/measured/1/iqr_bytes | target/memory-report.json + target/climb-results.json |
| Large — resident median (measured) | 1087139840 | B | MEASURED | n = 7 | n/a | /runs/2/median_bytes + /memory/measured/2/median_bytes | target/memory-report.json + target/climb-results.json |
| Large — resident min (measured) | 1087107072 | B | MEASURED | n = 7 | n/a | /runs/2/min_bytes + /memory/measured/2/measured_range_min_bytes | target/memory-report.json + target/climb-results.json |
| Large — resident max (measured) | 1087217664 | B | MEASURED | n = 7 | n/a | /runs/2/max_bytes + /memory/measured/2/measured_range_max_bytes | target/memory-report.json + target/climb-results.json |
| Large — resident IQR (measured) | 81920 | B | MEASURED | n = 7 | n/a | /runs/2/iqr_bytes + /memory/measured/2/iqr_bytes | target/memory-report.json + target/climb-results.json |
| Small — peak median (measured) | 4145152 | B | MEASURED | n = 7 | n/a | /memory/measured/0/measured_peak_bytes | target/climb-results.json |
| Small — measurement samples | 7 | — | MEASURED | n = 7 | n/a | /memory/measured/0/samples | target/climb-results.json |
| Small — analytical, libMTL pages | 2097152 | B | ANALYTICAL | — | n/a | /memory/measured/0/libmtl_analytical_bytes | target/climb-results.json |
| Small — analytical, harness buffers | 27656 | B | ANALYTICAL | — | n/a | /memory/measured/0/harness_buffers_bytes | target/climb-results.json |
| Small — analytical, dataset buffers | 53600 | B | ANALYTICAL | — | n/a | /memory/measured/0/dataset_buffers_bytes | target/climb-results.json |
| Small — analytical, keypair | 186 | B | ANALYTICAL | — | n/a | /memory/measured/0/keypair_bytes | target/climb-results.json |
| Small — analytical total | 2178594 | B | ANALYTICAL | — | n/a | /memory/measured/0/analytical_total_bytes | target/climb-results.json |
| Small — residual (measured − analytical) | 1966558 | B | DERIVED | n = 7 | n/a | /memory/measured/0/residual_bytes | target/climb-results.json |
| Small — measured / analytical ratio | 1.90 | × | DERIVED | n = 7 | n/a | /memory/measured/0/measured_analytical_total_ratio | target/climb-results.json |
| Medium — peak median (measured) | 14446592 | B | MEASURED | n = 7 | n/a | /memory/measured/1/measured_peak_bytes | target/climb-results.json |
| Medium — measurement samples | 7 | — | MEASURED | n = 7 | n/a | /memory/measured/1/samples | target/climb-results.json |
| Medium — analytical, libMTL pages | 2097152 | B | ANALYTICAL | — | n/a | /memory/measured/1/libmtl_analytical_bytes | target/climb-results.json |
| Medium — analytical, harness buffers | 3063384 | B | ANALYTICAL | — | n/a | /memory/measured/1/harness_buffers_bytes | target/climb-results.json |
| Medium — analytical, dataset buffers | 5360000 | B | ANALYTICAL | — | n/a | /memory/measured/1/dataset_buffers_bytes | target/climb-results.json |
| Medium — analytical, keypair | 186 | B | ANALYTICAL | — | n/a | /memory/measured/1/keypair_bytes | target/climb-results.json |
| Medium — analytical total | 10520722 | B | ANALYTICAL | — | n/a | /memory/measured/1/analytical_total_bytes | target/climb-results.json |
| Medium — residual (measured − analytical) | 3512174 | B | DERIVED | n = 7 | n/a | /memory/measured/1/residual_bytes | target/climb-results.json |
| Medium — measured / analytical ratio | 1.33 | × | DERIVED | n = 7 | n/a | /memory/measured/1/measured_analytical_total_ratio | target/climb-results.json |
| Large — peak median (measured) | 1143099392 | B | MEASURED | n = 7 | n/a | /memory/measured/2/measured_peak_bytes | target/climb-results.json |
| Large — measurement samples | 7 | — | MEASURED | n = 7 | n/a | /memory/measured/2/samples | target/climb-results.json |
| Large — analytical, libMTL pages | 49283072 | B | ANALYTICAL | — | n/a | /memory/measured/2/libmtl_analytical_bytes | target/climb-results.json |
| Large — analytical, harness buffers | 399067864 | B | ANALYTICAL | — | n/a | /memory/measured/2/harness_buffers_bytes | target/climb-results.json |
| Large — analytical, dataset buffers | 536000000 | B | ANALYTICAL | — | n/a | /memory/measured/2/dataset_buffers_bytes | target/climb-results.json |
| Large — analytical, keypair | 186 | B | ANALYTICAL | — | n/a | /memory/measured/2/keypair_bytes | target/climb-results.json |
| Large — analytical total | 984351122 | B | ANALYTICAL | — | n/a | /memory/measured/2/analytical_total_bytes | target/climb-results.json |
| Large — residual (measured − analytical) | 102788718 | B | DERIVED | n = 7 | n/a | /memory/measured/2/residual_bytes | target/climb-results.json |
| Large — measured / analytical ratio | 1.10 | × | DERIVED | n = 7 | n/a | /memory/measured/2/measured_analytical_total_ratio | target/climb-results.json |
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
| Ladder, N=3 — rung count | 2 | — | MEASURED | n = 3 | n/a | /sizes/2/rung_count + /ladder/sizes/2/rung_count | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=3 — ladder buffer bytes | 100 | B | MEASURED | n = 3 | n/a | /sizes/2/ladder_buffer_len + /ladder/sizes/2/ladder_buffer_len | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=3 — signed ladder bytes | 7960 | B | MEASURED | n = 3 | n/a | /sizes/2/signed_ladder_len + /ladder/sizes/2/signed_ladder_len | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=3 — condensed min | 76 | B | MEASURED | n = 3 | n/a | /sizes/2/condensed/min + /ladder/sizes/2/condensed_min | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=3 — condensed max | 92 | B | MEASURED | n = 3 | n/a | /sizes/2/condensed/max + /ladder/sizes/2/condensed_max | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=3 — condensed mean | 86.67 | B | MEASURED | n = 3 | n/a | /sizes/2/condensed/mean + /ladder/sizes/2/condensed_mean | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=4 — rung count | 1 | — | MEASURED | n = 4 | n/a | /sizes/3/rung_count + /ladder/sizes/3/rung_count | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=4 — ladder buffer bytes | 68 | B | MEASURED | n = 4 | n/a | /sizes/3/ladder_buffer_len + /ladder/sizes/3/ladder_buffer_len | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=4 — signed ladder bytes | 7928 | B | MEASURED | n = 4 | n/a | /sizes/3/signed_ladder_len + /ladder/sizes/3/signed_ladder_len | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=4 — condensed min | 108 | B | MEASURED | n = 4 | n/a | /sizes/3/condensed/min + /ladder/sizes/3/condensed_min | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=4 — condensed max | 108 | B | MEASURED | n = 4 | n/a | /sizes/3/condensed/max + /ladder/sizes/3/condensed_max | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=4 — condensed mean | 108.00 | B | MEASURED | n = 4 | n/a | /sizes/3/condensed/mean + /ladder/sizes/3/condensed_mean | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=5 — rung count | 2 | — | MEASURED | n = 5 | n/a | /sizes/4/rung_count + /ladder/sizes/4/rung_count | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=5 — ladder buffer bytes | 100 | B | MEASURED | n = 5 | n/a | /sizes/4/ladder_buffer_len + /ladder/sizes/4/ladder_buffer_len | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=5 — signed ladder bytes | 7960 | B | MEASURED | n = 5 | n/a | /sizes/4/signed_ladder_len + /ladder/sizes/4/signed_ladder_len | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=5 — condensed min | 76 | B | MEASURED | n = 5 | n/a | /sizes/4/condensed/min + /ladder/sizes/4/condensed_min | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=5 — condensed max | 108 | B | MEASURED | n = 5 | n/a | /sizes/4/condensed/max + /ladder/sizes/4/condensed_max | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=5 — condensed mean | 101.60 | B | MEASURED | n = 5 | n/a | /sizes/4/condensed/mean + /ladder/sizes/4/condensed_mean | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=6 — rung count | 2 | — | MEASURED | n = 6 | n/a | /sizes/5/rung_count + /ladder/sizes/5/rung_count | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=6 — ladder buffer bytes | 100 | B | MEASURED | n = 6 | n/a | /sizes/5/ladder_buffer_len + /ladder/sizes/5/ladder_buffer_len | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=6 — signed ladder bytes | 7960 | B | MEASURED | n = 6 | n/a | /sizes/5/signed_ladder_len + /ladder/sizes/5/signed_ladder_len | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=6 — condensed min | 92 | B | MEASURED | n = 6 | n/a | /sizes/5/condensed/min + /ladder/sizes/5/condensed_min | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=6 — condensed max | 108 | B | MEASURED | n = 6 | n/a | /sizes/5/condensed/max + /ladder/sizes/5/condensed_max | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=6 — condensed mean | 102.67 | B | MEASURED | n = 6 | n/a | /sizes/5/condensed/mean + /ladder/sizes/5/condensed_mean | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=7 — rung count | 3 | — | MEASURED | n = 7 | n/a | /sizes/6/rung_count + /ladder/sizes/6/rung_count | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=7 — ladder buffer bytes | 132 | B | MEASURED | n = 7 | n/a | /sizes/6/ladder_buffer_len + /ladder/sizes/6/ladder_buffer_len | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=7 — signed ladder bytes | 7992 | B | MEASURED | n = 7 | n/a | /sizes/6/signed_ladder_len + /ladder/sizes/6/signed_ladder_len | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=7 — condensed min | 76 | B | MEASURED | n = 7 | n/a | /sizes/6/condensed/min + /ladder/sizes/6/condensed_min | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=7 — condensed max | 108 | B | MEASURED | n = 7 | n/a | /sizes/6/condensed/max + /ladder/sizes/6/condensed_max | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=7 — condensed mean | 98.86 | B | MEASURED | n = 7 | n/a | /sizes/6/condensed/mean + /ladder/sizes/6/condensed_mean | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=8 — rung count | 1 | — | MEASURED | n = 8 | n/a | /sizes/7/rung_count + /ladder/sizes/7/rung_count | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=8 — ladder buffer bytes | 68 | B | MEASURED | n = 8 | n/a | /sizes/7/ladder_buffer_len + /ladder/sizes/7/ladder_buffer_len | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=8 — signed ladder bytes | 7928 | B | MEASURED | n = 8 | n/a | /sizes/7/signed_ladder_len + /ladder/sizes/7/signed_ladder_len | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=8 — condensed min | 124 | B | MEASURED | n = 8 | n/a | /sizes/7/condensed/min + /ladder/sizes/7/condensed_min | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=8 — condensed max | 124 | B | MEASURED | n = 8 | n/a | /sizes/7/condensed/max + /ladder/sizes/7/condensed_max | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=8 — condensed mean | 124.00 | B | MEASURED | n = 8 | n/a | /sizes/7/condensed/mean + /ladder/sizes/7/condensed_mean | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=9 — rung count | 2 | — | MEASURED | n = 9 | n/a | /sizes/8/rung_count + /ladder/sizes/8/rung_count | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=9 — ladder buffer bytes | 100 | B | MEASURED | n = 9 | n/a | /sizes/8/ladder_buffer_len + /ladder/sizes/8/ladder_buffer_len | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=9 — signed ladder bytes | 7960 | B | MEASURED | n = 9 | n/a | /sizes/8/signed_ladder_len + /ladder/sizes/8/signed_ladder_len | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=9 — condensed min | 76 | B | MEASURED | n = 9 | n/a | /sizes/8/condensed/min + /ladder/sizes/8/condensed_min | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=9 — condensed max | 124 | B | MEASURED | n = 9 | n/a | /sizes/8/condensed/max + /ladder/sizes/8/condensed_max | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=9 — condensed mean | 118.67 | B | MEASURED | n = 9 | n/a | /sizes/8/condensed/mean + /ladder/sizes/8/condensed_mean | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=13 — rung count | 3 | — | MEASURED | n = 13 | n/a | /sizes/9/rung_count + /ladder/sizes/9/rung_count | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=13 — ladder buffer bytes | 132 | B | MEASURED | n = 13 | n/a | /sizes/9/ladder_buffer_len + /ladder/sizes/9/ladder_buffer_len | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=13 — signed ladder bytes | 7992 | B | MEASURED | n = 13 | n/a | /sizes/9/signed_ladder_len + /ladder/sizes/9/signed_ladder_len | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=13 — condensed min | 76 | B | MEASURED | n = 13 | n/a | /sizes/9/condensed/min + /ladder/sizes/9/condensed_min | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=13 — condensed max | 124 | B | MEASURED | n = 13 | n/a | /sizes/9/condensed/max + /ladder/sizes/9/condensed_max | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=13 — condensed mean | 115.38 | B | MEASURED | n = 13 | n/a | /sizes/9/condensed/mean + /ladder/sizes/9/condensed_mean | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=16 — rung count | 1 | — | MEASURED | n = 16 | n/a | /sizes/10/rung_count + /ladder/sizes/10/rung_count | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=16 — ladder buffer bytes | 68 | B | MEASURED | n = 16 | n/a | /sizes/10/ladder_buffer_len + /ladder/sizes/10/ladder_buffer_len | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=16 — signed ladder bytes | 7928 | B | MEASURED | n = 16 | n/a | /sizes/10/signed_ladder_len + /ladder/sizes/10/signed_ladder_len | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=16 — condensed min | 140 | B | MEASURED | n = 16 | n/a | /sizes/10/condensed/min + /ladder/sizes/10/condensed_min | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=16 — condensed max | 140 | B | MEASURED | n = 16 | n/a | /sizes/10/condensed/max + /ladder/sizes/10/condensed_max | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=16 — condensed mean | 140.00 | B | MEASURED | n = 16 | n/a | /sizes/10/condensed/mean + /ladder/sizes/10/condensed_mean | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=31 — rung count | 5 | — | MEASURED | n = 31 | n/a | /sizes/11/rung_count + /ladder/sizes/11/rung_count | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=31 — ladder buffer bytes | 196 | B | MEASURED | n = 31 | n/a | /sizes/11/ladder_buffer_len + /ladder/sizes/11/ladder_buffer_len | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=31 — signed ladder bytes | 8056 | B | MEASURED | n = 31 | n/a | /sizes/11/signed_ladder_len + /ladder/sizes/11/signed_ladder_len | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=31 — condensed min | 76 | B | MEASURED | n = 31 | n/a | /sizes/11/condensed/min + /ladder/sizes/11/condensed_min | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=31 — condensed max | 140 | B | MEASURED | n = 31 | n/a | /sizes/11/condensed/max + /ladder/sizes/11/condensed_max | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=31 — condensed mean | 126.58 | B | MEASURED | n = 31 | n/a | /sizes/11/condensed/mean + /ladder/sizes/11/condensed_mean | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=32 — rung count | 1 | — | MEASURED | n = 32 | n/a | /sizes/12/rung_count + /ladder/sizes/12/rung_count | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=32 — ladder buffer bytes | 68 | B | MEASURED | n = 32 | n/a | /sizes/12/ladder_buffer_len + /ladder/sizes/12/ladder_buffer_len | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=32 — signed ladder bytes | 7928 | B | MEASURED | n = 32 | n/a | /sizes/12/signed_ladder_len + /ladder/sizes/12/signed_ladder_len | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=32 — condensed min | 156 | B | MEASURED | n = 32 | n/a | /sizes/12/condensed/min + /ladder/sizes/12/condensed_min | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=32 — condensed max | 156 | B | MEASURED | n = 32 | n/a | /sizes/12/condensed/max + /ladder/sizes/12/condensed_max | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=32 — condensed mean | 156.00 | B | MEASURED | n = 32 | n/a | /sizes/12/condensed/mean + /ladder/sizes/12/condensed_mean | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=33 — rung count | 2 | — | MEASURED | n = 33 | n/a | /sizes/13/rung_count + /ladder/sizes/13/rung_count | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=33 — ladder buffer bytes | 100 | B | MEASURED | n = 33 | n/a | /sizes/13/ladder_buffer_len + /ladder/sizes/13/ladder_buffer_len | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=33 — signed ladder bytes | 7960 | B | MEASURED | n = 33 | n/a | /sizes/13/signed_ladder_len + /ladder/sizes/13/signed_ladder_len | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=33 — condensed min | 76 | B | MEASURED | n = 33 | n/a | /sizes/13/condensed/min + /ladder/sizes/13/condensed_min | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=33 — condensed max | 156 | B | MEASURED | n = 33 | n/a | /sizes/13/condensed/max + /ladder/sizes/13/condensed_max | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=33 — condensed mean | 153.58 | B | MEASURED | n = 33 | n/a | /sizes/13/condensed/mean + /ladder/sizes/13/condensed_mean | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=64 — rung count | 1 | — | MEASURED | n = 64 | n/a | /sizes/14/rung_count + /ladder/sizes/14/rung_count | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=64 — ladder buffer bytes | 68 | B | MEASURED | n = 64 | n/a | /sizes/14/ladder_buffer_len + /ladder/sizes/14/ladder_buffer_len | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=64 — signed ladder bytes | 7928 | B | MEASURED | n = 64 | n/a | /sizes/14/signed_ladder_len + /ladder/sizes/14/signed_ladder_len | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=64 — condensed min | 172 | B | MEASURED | n = 64 | n/a | /sizes/14/condensed/min + /ladder/sizes/14/condensed_min | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=64 — condensed max | 172 | B | MEASURED | n = 64 | n/a | /sizes/14/condensed/max + /ladder/sizes/14/condensed_max | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=64 — condensed mean | 172.00 | B | MEASURED | n = 64 | n/a | /sizes/14/condensed/mean + /ladder/sizes/14/condensed_mean | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=100 — rung count | 3 | — | MEASURED | n = 100 | n/a | /sizes/15/rung_count + /ladder/sizes/15/rung_count | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=100 — ladder buffer bytes | 132 | B | MEASURED | n = 100 | n/a | /sizes/15/ladder_buffer_len + /ladder/sizes/15/ladder_buffer_len | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=100 — signed ladder bytes | 7992 | B | MEASURED | n = 100 | n/a | /sizes/15/signed_ladder_len + /ladder/sizes/15/signed_ladder_len | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=100 — condensed min | 108 | B | MEASURED | n = 100 | n/a | /sizes/15/condensed/min + /ladder/sizes/15/condensed_min | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=100 — condensed max | 172 | B | MEASURED | n = 100 | n/a | /sizes/15/condensed/max + /ladder/sizes/15/condensed_max | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=100 — condensed mean | 164.32 | B | MEASURED | n = 100 | n/a | /sizes/15/condensed/mean + /ladder/sizes/15/condensed_mean | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=1000 — rung count | 6 | — | MEASURED | n = 1000 | n/a | /sizes/16/rung_count + /ladder/sizes/16/rung_count | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=1000 — ladder buffer bytes | 228 | B | MEASURED | n = 1000 | n/a | /sizes/16/ladder_buffer_len + /ladder/sizes/16/ladder_buffer_len | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=1000 — signed ladder bytes | 8088 | B | MEASURED | n = 1000 | n/a | /sizes/16/signed_ladder_len + /ladder/sizes/16/signed_ladder_len | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=1000 — condensed min | 124 | B | MEASURED | n = 1000 | n/a | /sizes/16/condensed/min + /ladder/sizes/16/condensed_min | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=1000 — condensed max | 220 | B | MEASURED | n = 1000 | n/a | /sizes/16/condensed/max + /ladder/sizes/16/condensed_max | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=1000 — condensed mean | 205.92 | B | MEASURED | n = 1000 | n/a | /sizes/16/condensed/mean + /ladder/sizes/16/condensed_mean | target/ladder-verification.json + target/climb-results.json |
| Ladder, N=1 — matches the closed-form model | yes | — | DERIVED | n = 1 | n/a | /ladder/sizes/0/model_matches | target/climb-results.json |
| Ladder, N=2 — matches the closed-form model | yes | — | DERIVED | n = 2 | n/a | /ladder/sizes/1/model_matches | target/climb-results.json |
| Ladder, N=3 — matches the closed-form model | yes | — | DERIVED | n = 3 | n/a | /ladder/sizes/2/model_matches | target/climb-results.json |
| Ladder, N=4 — matches the closed-form model | yes | — | DERIVED | n = 4 | n/a | /ladder/sizes/3/model_matches | target/climb-results.json |
| Ladder, N=5 — matches the closed-form model | yes | — | DERIVED | n = 5 | n/a | /ladder/sizes/4/model_matches | target/climb-results.json |
| Ladder, N=6 — matches the closed-form model | yes | — | DERIVED | n = 6 | n/a | /ladder/sizes/5/model_matches | target/climb-results.json |
| Ladder, N=7 — matches the closed-form model | yes | — | DERIVED | n = 7 | n/a | /ladder/sizes/6/model_matches | target/climb-results.json |
| Ladder, N=8 — matches the closed-form model | yes | — | DERIVED | n = 8 | n/a | /ladder/sizes/7/model_matches | target/climb-results.json |
| Ladder, N=9 — matches the closed-form model | yes | — | DERIVED | n = 9 | n/a | /ladder/sizes/8/model_matches | target/climb-results.json |
| Ladder, N=13 — matches the closed-form model | yes | — | DERIVED | n = 13 | n/a | /ladder/sizes/9/model_matches | target/climb-results.json |
| Ladder, N=16 — matches the closed-form model | yes | — | DERIVED | n = 16 | n/a | /ladder/sizes/10/model_matches | target/climb-results.json |
| Ladder, N=31 — matches the closed-form model | yes | — | DERIVED | n = 31 | n/a | /ladder/sizes/11/model_matches | target/climb-results.json |
| Ladder, N=32 — matches the closed-form model | yes | — | DERIVED | n = 32 | n/a | /ladder/sizes/12/model_matches | target/climb-results.json |
| Ladder, N=33 — matches the closed-form model | yes | — | DERIVED | n = 33 | n/a | /ladder/sizes/13/model_matches | target/climb-results.json |
| Ladder, N=64 — matches the closed-form model | yes | — | DERIVED | n = 64 | n/a | /ladder/sizes/14/model_matches | target/climb-results.json |
| Ladder, N=100 — matches the closed-form model | yes | — | DERIVED | n = 100 | n/a | /ladder/sizes/15/model_matches | target/climb-results.json |
| Ladder, N=1000 — matches the closed-form model | yes | — | DERIVED | n = 1000 | n/a | /ladder/sizes/16/model_matches | target/climb-results.json |
| Ladder — model matches every measured size | yes | — | DERIVED | — | n/a | /ladder/all_match | target/climb-results.json |
| Ladder — algorithm | SLH-DSA-SHA2-128s-MTL-SHA2-128 | — | MEASURED | — | n/a | /ladder/algorithm | target/climb-results.json |
| Ladder — recorded at | 2026-10-20T14:30:14+05:30 | — | MEASURED | — | n/a | /ladder/recorded_at | target/climb-results.json |

## DNS live

> per-section response anatomy: not recorded in this capture artifact

| Label | Value | Unit | Evidence class | n / k | Baseline | JSON pointer | Source |
| --- | --- | --- | --- | --- | --- | --- | --- |
| DNS, climb.example. SOA [full] — response bytes | 24465 | B | MEASURED | — | B | /queries/0/response_bytes | target/dns-capture.json |
| DNS, climb.example. SOA [full] — answer RRSIG bytes | 8069 | B | MEASURED | — | B | /queries/0/answer_rrsig_bytes | target/dns-capture.json |
| DNS, www.climb.example. A [full] — response bytes | 24438 | B | MEASURED | — | B | /queries/1/response_bytes + /dns_pipeline/full_signature_bytes_www_a | target/dns-capture.json + target/climb-results.json |
| DNS, www.climb.example. A [full] — answer RRSIG bytes | 8069 | B | MEASURED | — | B | /queries/1/answer_rrsig_bytes | target/dns-capture.json |
| DNS, mail.climb.example. MX [full] — response bytes | 32572 | B | MEASURED | — | B | /queries/2/response_bytes | target/dns-capture.json |
| DNS, mail.climb.example. MX [full] — answer RRSIG bytes | 8069 | B | MEASURED | — | B | /queries/2/answer_rrsig_bytes | target/dns-capture.json |
| DNS, climb.example. SOA [condensed] — response bytes | 685 | B | MEASURED | — | C | /queries/3/response_bytes | target/dns-capture.json |
| DNS, climb.example. SOA [condensed] — answer RRSIG bytes | 141 | B | MEASURED | — | C | /queries/3/answer_rrsig_bytes | target/dns-capture.json |
| DNS, www.climb.example. A [condensed] — response bytes | 658 | B | MEASURED | — | C | /queries/4/response_bytes + /dns_pipeline/condensed_proof_bytes_www_a | target/dns-capture.json + target/climb-results.json |
| DNS, www.climb.example. A [condensed] — answer RRSIG bytes | 141 | B | MEASURED | — | C | /queries/4/answer_rrsig_bytes | target/dns-capture.json |
| DNS, mail.climb.example. MX [condensed] — response bytes | 864 | B | MEASURED | — | C | /queries/5/response_bytes | target/dns-capture.json |
| DNS, mail.climb.example. MX [condensed] — answer RRSIG bytes | 141 | B | MEASURED | — | C | /queries/5/answer_rrsig_bytes | target/dns-capture.json |
| DNS, climb.example. SOA [unbound] — response bytes | 8207 | B | MEASURED | — | B | /queries/6/response_bytes | target/dns-capture.json |
| DNS, climb.example. SOA [unbound] — answer RRSIG bytes | 8069 | B | MEASURED | — | B | /queries/6/answer_rrsig_bytes | target/dns-capture.json |
| DNS, www.climb.example. A [unbound] — response bytes | 8176 | B | MEASURED | — | B | /queries/7/response_bytes | target/dns-capture.json |
| DNS, www.climb.example. A [unbound] — answer RRSIG bytes | 8069 | B | MEASURED | — | B | /queries/7/answer_rrsig_bytes | target/dns-capture.json |
| DNS, mail.climb.example. MX [unbound] — response bytes | 8180 | B | MEASURED | — | B | /queries/8/response_bytes | target/dns-capture.json |
| DNS, mail.climb.example. MX [unbound] — answer RRSIG bytes | 8069 | B | MEASURED | — | B | /queries/8/answer_rrsig_bytes | target/dns-capture.json |
| DNS — algorithm | SLH-DSA-SHA2-128s-MTL-SHA2-128 | — | MEASURED | — | n/a | /algorithm + /dns_pipeline/algorithm | target/dns-capture.json + target/climb-results.json |
| DNS — recorded at | 2026-10-20T14:30:35+05:30 | — | MEASURED | — | n/a | /recorded_at + /dns_pipeline/recorded_at | target/dns-capture.json + target/climb-results.json |
| DNS, www.climb.example. A — response reduction, condensed vs full | 97.307 | % | DERIVED | — | B | /dns_pipeline/reduction_percent_www_a | target/climb-results.json |
| DNS — capture source | live | — | MEASURED | — | n/a | /dns_pipeline/source | target/climb-results.json |

## Reuse

| Label | Value | Unit | Evidence class | n / k | Baseline | JSON pointer | Source |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Small — reuse break-even (time) | 2 | × | DERIVED | n = 100 | n/a | /derived/reuse/0/breakeven_reuse_time | target/climb-results.json |
| Small — reuse break-even (bytes) | 2 | × | DERIVED | n = 100 | n/a | /derived/reuse/0/breakeven_reuse_bytes | target/climb-results.json |
| Small — r* (time) | 2 | × | DERIVED | n = 100 | n/a | /derived/reuse/0/r_star_time | target/climb-results.json |
| Small — r* (bytes) | 2 | × | DERIVED | n = 100 | n/a | /derived/reuse/0/r_star_bytes | target/climb-results.json |
| Medium — reuse break-even (time) | 2 | × | DERIVED | n = 10000 | n/a | /derived/reuse/1/breakeven_reuse_time | target/climb-results.json |
| Medium — reuse break-even (bytes) | 2 | × | DERIVED | n = 10000 | n/a | /derived/reuse/1/breakeven_reuse_bytes | target/climb-results.json |
| Medium — r* (time) | 2 | × | DERIVED | n = 10000 | n/a | /derived/reuse/1/r_star_time | target/climb-results.json |
| Medium — r* (bytes) | 2 | × | DERIVED | n = 10000 | n/a | /derived/reuse/1/r_star_bytes | target/climb-results.json |
| Large — reuse break-even (time) | ≈2 | × | EXTRAPOLATED | n = 1000000 | n/a | /derived/reuse/2/breakeven_reuse_time | target/climb-results.json |
| Large — reuse break-even (bytes) | ≈2 | × | EXTRAPOLATED | n = 1000000 | n/a | /derived/reuse/2/breakeven_reuse_bytes | target/climb-results.json |
| Large — r* (time) | ≈2 | × | EXTRAPOLATED | n = 1000000 | n/a | /derived/reuse/2/r_star_time | target/climb-results.json |
| Large — r* (bytes) | ≈2 | × | EXTRAPOLATED | n = 1000000 | n/a | /derived/reuse/2/r_star_bytes | target/climb-results.json |

## UDP fit

| Label | Value | Unit | Evidence class | n / k | Baseline | JSON pointer | Source |
| --- | --- | --- | --- | --- | --- | --- | --- |
| UDP fit — model matches capture | yes | — | DERIVED | — | n/a | /derived/udp_fit/validation/matches | target/climb-results.json |
| UDP fit, leaves=100 — A (plain SLH-DSA) Positive A (www) fit ≤1232 B | 0.000 | % | ANALYTICAL | n = 100 | n/a | /derived/udp_fit/rows/0/fit_1232_percent | target/climb-results.json |
| UDP fit, leaves=1000 — A (plain SLH-DSA) Positive A (www) fit ≤1232 B | 0.000 | % | ANALYTICAL | n = 1000 | n/a | /derived/udp_fit/rows/1/fit_1232_percent | target/climb-results.json |
| UDP fit, leaves=10000 — A (plain SLH-DSA) Positive A (www) fit ≤1232 B | 0.000 | % | ANALYTICAL | n = 10000 | n/a | /derived/udp_fit/rows/2/fit_1232_percent | target/climb-results.json |
| UDP fit, leaves=1000000 — A (plain SLH-DSA) Positive A (www) fit ≤1232 B | 0.000 | % | ANALYTICAL | n = 1000000 | n/a | /derived/udp_fit/rows/3/fit_1232_percent | target/climb-results.json |
| UDP fit, leaves=100 — B (MTL non-amortised) Positive A (www) fit ≤1232 B | 0.000 | % | ANALYTICAL | n = 100 | n/a | /derived/udp_fit/rows/4/fit_1232_percent | target/climb-results.json |
| UDP fit, leaves=1000 — B (MTL non-amortised) Positive A (www) fit ≤1232 B | 0.000 | % | ANALYTICAL | n = 1000 | n/a | /derived/udp_fit/rows/5/fit_1232_percent | target/climb-results.json |
| UDP fit, leaves=10000 — B (MTL non-amortised) Positive A (www) fit ≤1232 B | 0.000 | % | ANALYTICAL | n = 10000 | n/a | /derived/udp_fit/rows/6/fit_1232_percent | target/climb-results.json |
| UDP fit, leaves=1000000 — B (MTL non-amortised) Positive A (www) fit ≤1232 B | 0.000 | % | ANALYTICAL | n = 1000000 | n/a | /derived/udp_fit/rows/7/fit_1232_percent | target/climb-results.json |
| UDP fit, leaves=100 — C (MTL amortised) Positive A (www) fit ≤1232 B | 100.000 | % | ANALYTICAL | n = 100 | n/a | /derived/udp_fit/rows/8/fit_1232_percent | target/climb-results.json |
| UDP fit, leaves=1000 — C (MTL amortised) Positive A (www) fit ≤1232 B | 100.000 | % | ANALYTICAL | n = 1000 | n/a | /derived/udp_fit/rows/9/fit_1232_percent | target/climb-results.json |
| UDP fit, leaves=10000 — C (MTL amortised) Positive A (www) fit ≤1232 B | 100.000 | % | ANALYTICAL | n = 10000 | n/a | /derived/udp_fit/rows/10/fit_1232_percent | target/climb-results.json |
| UDP fit, leaves=1000000 — C (MTL amortised) Positive A (www) fit ≤1232 B | 100.000 | % | ANALYTICAL | n = 1000000 | n/a | /derived/udp_fit/rows/11/fit_1232_percent | target/climb-results.json |
| UDP fit, leaves=100 — E (Ed25519, context) Positive A (www) fit ≤1232 B | 100.000 | % | ANALYTICAL | n = 100 | n/a | /derived/udp_fit/rows/12/fit_1232_percent | target/climb-results.json |
| UDP fit, leaves=1000 — E (Ed25519, context) Positive A (www) fit ≤1232 B | 100.000 | % | ANALYTICAL | n = 1000 | n/a | /derived/udp_fit/rows/13/fit_1232_percent | target/climb-results.json |
| UDP fit, leaves=10000 — E (Ed25519, context) Positive A (www) fit ≤1232 B | 100.000 | % | ANALYTICAL | n = 10000 | n/a | /derived/udp_fit/rows/14/fit_1232_percent | target/climb-results.json |
| UDP fit, leaves=1000000 — E (Ed25519, context) Positive A (www) fit ≤1232 B | 100.000 | % | ANALYTICAL | n = 1000000 | n/a | /derived/udp_fit/rows/15/fit_1232_percent | target/climb-results.json |
| UDP fit, leaves=100 — P (ECDSA P-256, context) Positive A (www) fit ≤1232 B | 100.000 | % | ANALYTICAL | n = 100 | n/a | /derived/udp_fit/rows/16/fit_1232_percent | target/climb-results.json |
| UDP fit, leaves=1000 — P (ECDSA P-256, context) Positive A (www) fit ≤1232 B | 100.000 | % | ANALYTICAL | n = 1000 | n/a | /derived/udp_fit/rows/17/fit_1232_percent | target/climb-results.json |
| UDP fit, leaves=10000 — P (ECDSA P-256, context) Positive A (www) fit ≤1232 B | 100.000 | % | ANALYTICAL | n = 10000 | n/a | /derived/udp_fit/rows/18/fit_1232_percent | target/climb-results.json |
| UDP fit, leaves=1000000 — P (ECDSA P-256, context) Positive A (www) fit ≤1232 B | 100.000 | % | ANALYTICAL | n = 1000000 | n/a | /derived/udp_fit/rows/19/fit_1232_percent | target/climb-results.json |
| UDP fit, leaves=100 — R (RSA-2048, context) Positive A (www) fit ≤1232 B | 100.000 | % | ANALYTICAL | n = 100 | n/a | /derived/udp_fit/rows/20/fit_1232_percent | target/climb-results.json |
| UDP fit, leaves=1000 — R (RSA-2048, context) Positive A (www) fit ≤1232 B | 100.000 | % | ANALYTICAL | n = 1000 | n/a | /derived/udp_fit/rows/21/fit_1232_percent | target/climb-results.json |
| UDP fit, leaves=10000 — R (RSA-2048, context) Positive A (www) fit ≤1232 B | 100.000 | % | ANALYTICAL | n = 10000 | n/a | /derived/udp_fit/rows/22/fit_1232_percent | target/climb-results.json |
| UDP fit, leaves=1000000 — R (RSA-2048, context) Positive A (www) fit ≤1232 B | 100.000 | % | ANALYTICAL | n = 1000000 | n/a | /derived/udp_fit/rows/23/fit_1232_percent | target/climb-results.json |
| UDP fit, leaves=100 — A (plain SLH-DSA) Positive A with NS fit ≤1232 B | 0.000 | % | ANALYTICAL | n = 100 | n/a | /derived/udp_fit/rows/24/fit_1232_percent | target/climb-results.json |
| UDP fit, leaves=1000 — A (plain SLH-DSA) Positive A with NS fit ≤1232 B | 0.000 | % | ANALYTICAL | n = 1000 | n/a | /derived/udp_fit/rows/25/fit_1232_percent | target/climb-results.json |
| UDP fit, leaves=10000 — A (plain SLH-DSA) Positive A with NS fit ≤1232 B | 0.000 | % | ANALYTICAL | n = 10000 | n/a | /derived/udp_fit/rows/26/fit_1232_percent | target/climb-results.json |
| UDP fit, leaves=1000000 — A (plain SLH-DSA) Positive A with NS fit ≤1232 B | 0.000 | % | ANALYTICAL | n = 1000000 | n/a | /derived/udp_fit/rows/27/fit_1232_percent | target/climb-results.json |
| UDP fit, leaves=100 — B (MTL non-amortised) Positive A with NS fit ≤1232 B | 0.000 | % | ANALYTICAL | n = 100 | n/a | /derived/udp_fit/rows/28/fit_1232_percent | target/climb-results.json |
| UDP fit, leaves=1000 — B (MTL non-amortised) Positive A with NS fit ≤1232 B | 0.000 | % | ANALYTICAL | n = 1000 | n/a | /derived/udp_fit/rows/29/fit_1232_percent | target/climb-results.json |
| UDP fit, leaves=10000 — B (MTL non-amortised) Positive A with NS fit ≤1232 B | 0.000 | % | ANALYTICAL | n = 10000 | n/a | /derived/udp_fit/rows/30/fit_1232_percent | target/climb-results.json |
| UDP fit, leaves=1000000 — B (MTL non-amortised) Positive A with NS fit ≤1232 B | 0.000 | % | ANALYTICAL | n = 1000000 | n/a | /derived/udp_fit/rows/31/fit_1232_percent | target/climb-results.json |
| UDP fit, leaves=100 — C (MTL amortised) Positive A with NS fit ≤1232 B | 100.000 | % | ANALYTICAL | n = 100 | n/a | /derived/udp_fit/rows/32/fit_1232_percent | target/climb-results.json |
| UDP fit, leaves=1000 — C (MTL amortised) Positive A with NS fit ≤1232 B | 100.000 | % | ANALYTICAL | n = 1000 | n/a | /derived/udp_fit/rows/33/fit_1232_percent | target/climb-results.json |
| UDP fit, leaves=10000 — C (MTL amortised) Positive A with NS fit ≤1232 B | 100.000 | % | ANALYTICAL | n = 10000 | n/a | /derived/udp_fit/rows/34/fit_1232_percent | target/climb-results.json |
| UDP fit, leaves=1000000 — C (MTL amortised) Positive A with NS fit ≤1232 B | 0.358 | % | ANALYTICAL | n = 1000000 | n/a | /derived/udp_fit/rows/35/fit_1232_percent | target/climb-results.json |
| UDP fit, leaves=100 — E (Ed25519, context) Positive A with NS fit ≤1232 B | 100.000 | % | ANALYTICAL | n = 100 | n/a | /derived/udp_fit/rows/36/fit_1232_percent | target/climb-results.json |
| UDP fit, leaves=1000 — E (Ed25519, context) Positive A with NS fit ≤1232 B | 100.000 | % | ANALYTICAL | n = 1000 | n/a | /derived/udp_fit/rows/37/fit_1232_percent | target/climb-results.json |
| UDP fit, leaves=10000 — E (Ed25519, context) Positive A with NS fit ≤1232 B | 100.000 | % | ANALYTICAL | n = 10000 | n/a | /derived/udp_fit/rows/38/fit_1232_percent | target/climb-results.json |
| UDP fit, leaves=1000000 — E (Ed25519, context) Positive A with NS fit ≤1232 B | 100.000 | % | ANALYTICAL | n = 1000000 | n/a | /derived/udp_fit/rows/39/fit_1232_percent | target/climb-results.json |
| UDP fit, leaves=100 — P (ECDSA P-256, context) Positive A with NS fit ≤1232 B | 100.000 | % | ANALYTICAL | n = 100 | n/a | /derived/udp_fit/rows/40/fit_1232_percent | target/climb-results.json |
| UDP fit, leaves=1000 — P (ECDSA P-256, context) Positive A with NS fit ≤1232 B | 100.000 | % | ANALYTICAL | n = 1000 | n/a | /derived/udp_fit/rows/41/fit_1232_percent | target/climb-results.json |
| UDP fit, leaves=10000 — P (ECDSA P-256, context) Positive A with NS fit ≤1232 B | 100.000 | % | ANALYTICAL | n = 10000 | n/a | /derived/udp_fit/rows/42/fit_1232_percent | target/climb-results.json |
| UDP fit, leaves=1000000 — P (ECDSA P-256, context) Positive A with NS fit ≤1232 B | 100.000 | % | ANALYTICAL | n = 1000000 | n/a | /derived/udp_fit/rows/43/fit_1232_percent | target/climb-results.json |
| UDP fit, leaves=100 — R (RSA-2048, context) Positive A with NS fit ≤1232 B | 100.000 | % | ANALYTICAL | n = 100 | n/a | /derived/udp_fit/rows/44/fit_1232_percent | target/climb-results.json |
| UDP fit, leaves=1000 — R (RSA-2048, context) Positive A with NS fit ≤1232 B | 100.000 | % | ANALYTICAL | n = 1000 | n/a | /derived/udp_fit/rows/45/fit_1232_percent | target/climb-results.json |
| UDP fit, leaves=10000 — R (RSA-2048, context) Positive A with NS fit ≤1232 B | 100.000 | % | ANALYTICAL | n = 10000 | n/a | /derived/udp_fit/rows/46/fit_1232_percent | target/climb-results.json |
| UDP fit, leaves=1000000 — R (RSA-2048, context) Positive A with NS fit ≤1232 B | 100.000 | % | ANALYTICAL | n = 1000000 | n/a | /derived/udp_fit/rows/47/fit_1232_percent | target/climb-results.json |
| UDP fit — worst case fit ≤1232 B | 0.000 | % | ANALYTICAL | — | n/a | /derived/udp_fit/rows/*/fit_1232_percent | target/climb-results.json |

## Environment

| Label | Value | Unit | Evidence class | n / k | Baseline | JSON pointer | Source |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Environment — timestamp | 2026-10-20T13:13:45+05:30 | — | MEASURED | — | n/a | /timestamp + /environment/timestamp | environment.json + target/climb-results.json |
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
| Repository — commit SHA | ef8d86f40fb809e2c5ac7642b0e2127e97c25d00 | — | UNCLASSIFIED | — | n/a | n/a | git rev-parse HEAD |

## Quality

| Label | Value | Unit | Evidence class | n / k | Baseline | JSON pointer | Source |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Workspace tests — passed | 988 | — | MEASURED | n = 27 | n/a | n/a | cargo test --workspace |
| Workspace tests — failed | 6 | — | MEASURED | n = 27 | n/a | n/a | cargo test --workspace |
| Workspace tests — ignored | 1 | — | MEASURED | n = 27 | n/a | n/a | cargo test --workspace |
| Workspace test targets (summary lines) | 27 | — | MEASURED | n = 27 | n/a | n/a | cargo test --workspace |

---

## Footer

- Rows rendered: 361 (from 521 collected facts; 160 duplicate copies merged).
- Evidence classes (rendered rows): MEASURED: 202, ANALYTICAL: 68, EXTRAPOLATED: 24, DERIVED: 66; UNCLASSIFIED: 1.
- UNCLASSIFIED rows: Repository — commit SHA.
- Inconsistencies: none.
