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

| Label | Value | Unit | Evidence class | n / k | Baseline | JSON pointer | Source |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Small — signing per batch, C (MTL amortised) | 507.9 | ms | MEASURED | n = 100 | C | /benchmark/signing/0/mean_ns | target/climb-results.json |
| Small — signing per message, C (MTL amortised) | 5.1 | ms | DERIVED | n = 100 | C | /benchmark/signing/0/mean_ns | target/climb-results.json |
| Small — signing per batch, B (MTL, no amortisation) | 507.0 | ms | MEASURED | n = 1 | B | /benchmark/signing/1/mean_ns | target/climb-results.json |
| Small — signing per message, B (MTL, no amortisation) | 507.0 | ms | DERIVED | n = 1 | B | /benchmark/signing/1/mean_ns | target/climb-results.json |
| OneK — signing per batch, C (MTL amortised) | 511.7 | ms | MEASURED | n = 1000 | C | /benchmark/signing/2/mean_ns | target/climb-results.json |
| OneK — signing per message, C (MTL amortised) | 511.7 | us | DERIVED | n = 1000 | C | /benchmark/signing/2/mean_ns | target/climb-results.json |
| OneK — signing per batch, B (MTL, no amortisation) | 512.9 | ms | MEASURED | n = 1 | B | /benchmark/signing/3/mean_ns | target/climb-results.json |
| OneK — signing per message, B (MTL, no amortisation) | 512.9 | ms | DERIVED | n = 1 | B | /benchmark/signing/3/mean_ns | target/climb-results.json |
| Medium — signing per batch, C (MTL amortised) | 538.4 | ms | MEASURED | n = 10000 | C | /benchmark/signing/4/mean_ns | target/climb-results.json |
| Medium — signing per message, C (MTL amortised) | 53.8 | us | DERIVED | n = 10000 | C | /benchmark/signing/4/mean_ns | target/climb-results.json |
| Medium — signing per batch, B (MTL, no amortisation) | 507.6 | ms | MEASURED | n = 1 | B | /benchmark/signing/5/mean_ns | target/climb-results.json |
| Medium — signing per message, B (MTL, no amortisation) | 507.6 | ms | DERIVED | n = 1 | B | /benchmark/signing/5/mean_ns | target/climb-results.json |
| Large — signing per batch, C (MTL amortised) | 4.21 | s | MEASURED | n = 1000000 | C | /benchmark/signing/6/mean_ns | target/climb-results.json |
| Large — signing per message, C (MTL amortised) | 4.2 | us | DERIVED | n = 1000000 | C | /benchmark/signing/6/mean_ns | target/climb-results.json |
| Large — signing per batch, B (MTL, no amortisation) | 507.4 | ms | MEASURED | n = 1 | B | /benchmark/signing/7/mean_ns | target/climb-results.json |
| Large — signing per message, B (MTL, no amortisation) | 507.4 | ms | DERIVED | n = 1 | B | /benchmark/signing/7/mean_ns | target/climb-results.json |
| Small — signing per message, A (plain SLH-DSA) | 507.6 | ms | MEASURED | n = 1 | A | /plain/0/signing_mean_ns | target/climb-results.json |
| Small — signing per batch, A (plain SLH-DSA) | ≈50.76 | s | EXTRAPOLATED | k = 1 | A | /plain/0/signing_mean_ns | target/climb-results.json |
| OneK — signing per message, A (plain SLH-DSA) | ≈507.8 | ms | EXTRAPOLATED | k = 2 | A | /plain/1/signing_mean_ns | target/climb-results.json |
| OneK — signing per batch, A (plain SLH-DSA) | ≈8.46 | min | EXTRAPOLATED | k = 2 | A | /plain/1/signing_mean_ns | target/climb-results.json |
| Medium — signing per message, A (plain SLH-DSA) | 508.0 | ms | MEASURED | n = 1 | A | /plain/2/signing_mean_ns | target/climb-results.json |
| Medium — signing per batch, A (plain SLH-DSA) | ≈1.41 | h | EXTRAPOLATED | k = 1 | A | /plain/2/signing_mean_ns | target/climb-results.json |
| Large — signing per message, A (plain SLH-DSA) | ≈507.8 | ms | EXTRAPOLATED | k = 2 | A | /plain/3/signing_mean_ns | target/climb-results.json |
| Large — signing per batch, A (plain SLH-DSA) | ≈141.05 | h | EXTRAPOLATED | k = 2 | A | /plain/3/signing_mean_ns | target/climb-results.json |
| Small — signing speed-up, C vs A | 99.94 | × | DERIVED | n = 100 | A | /comparisons/0/signing_speedup_c_vs_a | target/climb-results.json |
| Small — signing speed-up, C vs B | 99.81 | × | DERIVED | n = 100 | B | /comparisons/0/signing_speedup_c_vs_b | target/climb-results.json |
| OneK — signing speed-up, C vs A | ≈992.4 | × | EXTRAPOLATED | n = 1000 | A | /comparisons/1/signing_speedup_c_vs_a | target/climb-results.json |
| OneK — signing speed-up, C vs B | ≈1,002 | × | EXTRAPOLATED | n = 1000 | B | /comparisons/1/signing_speedup_c_vs_b | target/climb-results.json |
| Medium — signing speed-up, C vs A | 9,434 | × | DERIVED | n = 10000 | A | /comparisons/2/signing_speedup_c_vs_a | target/climb-results.json |
| Medium — signing speed-up, C vs B | 9,427 | × | DERIVED | n = 10000 | B | /comparisons/2/signing_speedup_c_vs_b | target/climb-results.json |
| Large — signing speed-up, C vs A | ≈120,621 | × | EXTRAPOLATED | n = 1000000 | A | /comparisons/3/signing_speedup_c_vs_a | target/climb-results.json |
| Large — signing speed-up, C vs B | ≈120,536 | × | EXTRAPOLATED | n = 1000000 | B | /comparisons/3/signing_speedup_c_vs_b | target/climb-results.json |
| Medium — signing speed-up model, C vs A — spread median | 9407.3 | × | DERIVED | n = 5 | A | /derived/stability/runs/0/median | target/climb-results.json |
| Medium — signing speed-up model, C vs A — spread minimum | 9394.2 | × | DERIVED | n = 5 | A | /derived/stability/runs/0/min | target/climb-results.json |
| Medium — signing speed-up model, C vs A — spread maximum | 9419.4 | × | DERIVED | n = 5 | A | /derived/stability/runs/0/max | target/climb-results.json |
| OneK — signing speed-up model, C vs A — spread median | 993.6 | × | DERIVED | n = 5 | A | /derived/stability/runs/1/median | target/climb-results.json |
| OneK — signing speed-up model, C vs A — spread minimum | 993.0 | × | DERIVED | n = 5 | A | /derived/stability/runs/1/min | target/climb-results.json |
| OneK — signing speed-up model, C vs A — spread maximum | 994.6 | × | DERIVED | n = 5 | A | /derived/stability/runs/1/max | target/climb-results.json |
| Small — signing speed-up model, C vs A — spread median | 99.8 | × | DERIVED | n = 5 | A | /derived/stability/runs/2/median | target/climb-results.json |
| Small — signing speed-up model, C vs A — spread minimum | 99.6 | × | DERIVED | n = 5 | A | /derived/stability/runs/2/min | target/climb-results.json |
| Small — signing speed-up model, C vs A — spread maximum | 99.9 | × | DERIVED | n = 5 | A | /derived/stability/runs/2/max | target/climb-results.json |
| Medium — signing speed-up model, B vs C — spread median | 9411.7 | × | DERIVED | n = 5 | B | /derived/stability/runs/3/median | target/climb-results.json |
| Medium — signing speed-up model, B vs C — spread minimum | 9406.1 | × | DERIVED | n = 5 | B | /derived/stability/runs/3/min | target/climb-results.json |
| Medium — signing speed-up model, B vs C — spread maximum | 9426.1 | × | DERIVED | n = 5 | B | /derived/stability/runs/3/max | target/climb-results.json |
| OneK — signing speed-up model, B vs C — spread median | 994.3 | × | DERIVED | n = 5 | B | /derived/stability/runs/4/median | target/climb-results.json |
| OneK — signing speed-up model, B vs C — spread minimum | 993.5 | × | DERIVED | n = 5 | B | /derived/stability/runs/4/min | target/climb-results.json |
| OneK — signing speed-up model, B vs C — spread maximum | 996.3 | × | DERIVED | n = 5 | B | /derived/stability/runs/4/max | target/climb-results.json |
| Small — signing speed-up model, B vs C — spread median | 99.9 | × | DERIVED | n = 5 | B | /derived/stability/runs/5/median | target/climb-results.json |
| Small — signing speed-up model, B vs C — spread minimum | 99.7 | × | DERIVED | n = 5 | B | /derived/stability/runs/5/min | target/climb-results.json |
| Small — signing speed-up model, B vs C — spread maximum | 100.1 | × | DERIVED | n = 5 | B | /derived/stability/runs/5/max | target/climb-results.json |

## Verifying time

| Label | Value | Unit | Evidence class | n / k | Baseline | JSON pointer | Source |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Small — verifying per batch, C (trust cached ladder) | 334.9 | us | MEASURED | n = 100 | C | /benchmark/verifying/0/mean_ns | target/climb-results.json |
| Small — verifying per message, C (trust cached ladder) | 3.3 | us | DERIVED | n = 100 | C | /benchmark/verifying/0/mean_ns | target/climb-results.json |
| Small — verifying per batch, C (ladder re-validated) | 50.1 | ms | MEASURED | n = 100 | C | /benchmark/verifying/1/mean_ns | target/climb-results.json |
| Small — verifying per message, C (ladder re-validated) | 501.0 | us | DERIVED | n = 100 | C | /benchmark/verifying/1/mean_ns | target/climb-results.json |
| Small — verifying per batch, B (MTL, no amortisation) | 47.4 | ms | MEASURED | n = 100 | B | /benchmark/verifying/2/mean_ns | target/climb-results.json |
| Small — verifying per message, B (MTL, no amortisation) | 474.3 | us | DERIVED | n = 100 | B | /benchmark/verifying/2/mean_ns | target/climb-results.json |
| OneK — verifying per batch, C (trust cached ladder) | 4.6 | ms | MEASURED | n = 1000 | C | /benchmark/verifying/3/mean_ns | target/climb-results.json |
| OneK — verifying per message, C (trust cached ladder) | 4.6 | us | DERIVED | n = 1000 | C | /benchmark/verifying/3/mean_ns | target/climb-results.json |
| OneK — verifying per batch, C (ladder re-validated) | 526.4 | ms | MEASURED | n = 1000 | C | /benchmark/verifying/4/mean_ns | target/climb-results.json |
| OneK — verifying per message, C (ladder re-validated) | 526.4 | us | DERIVED | n = 1000 | C | /benchmark/verifying/4/mean_ns | target/climb-results.json |
| OneK — verifying per batch, B (MTL, no amortisation) | 533.3 | ms | MEASURED | n = 1000 | B | /benchmark/verifying/5/mean_ns | target/climb-results.json |
| OneK — verifying per message, B (MTL, no amortisation) | 533.3 | us | DERIVED | n = 1000 | B | /benchmark/verifying/5/mean_ns | target/climb-results.json |
| Medium — verifying per batch, C (trust cached ladder) | 65.1 | ms | MEASURED | n = 10000 | C | /benchmark/verifying/6/mean_ns | target/climb-results.json |
| Medium — verifying per message, C (trust cached ladder) | 6.5 | us | DERIVED | n = 10000 | C | /benchmark/verifying/6/mean_ns | target/climb-results.json |
| Medium — verifying per batch, C (ladder re-validated) | 5.45 | s | MEASURED | n = 10000 | C | /benchmark/verifying/7/mean_ns | target/climb-results.json |
| Medium — verifying per message, C (ladder re-validated) | 544.9 | us | DERIVED | n = 10000 | C | /benchmark/verifying/7/mean_ns | target/climb-results.json |
| Medium — verifying per batch, B (MTL, no amortisation) | 5.08 | s | MEASURED | n = 10000 | B | /benchmark/verifying/8/mean_ns | target/climb-results.json |
| Medium — verifying per message, B (MTL, no amortisation) | 508.5 | us | DERIVED | n = 10000 | B | /benchmark/verifying/8/mean_ns | target/climb-results.json |
| Large — verifying per batch, C (trust cached ladder) | 9.12 | s | MEASURED | n = 1000000 | C | /benchmark/verifying/9/mean_ns | target/climb-results.json |
| Large — verifying per message, C (trust cached ladder) | 9.1 | us | DERIVED | n = 1000000 | C | /benchmark/verifying/9/mean_ns | target/climb-results.json |
| Small — verifying per message, A (plain SLH-DSA) | 483.2 | us | MEASURED | n = 1 | A | /plain/0/verifying_mean_ns | target/climb-results.json |
| Small — verifying per batch, A (plain SLH-DSA) | ≈48.3 | ms | EXTRAPOLATED | k = 1 | A | /plain/0/verifying_mean_ns | target/climb-results.json |
| OneK — verifying per message, A (plain SLH-DSA) | ≈493.3 | us | EXTRAPOLATED | k = 2 | A | /plain/1/verifying_mean_ns | target/climb-results.json |
| OneK — verifying per batch, A (plain SLH-DSA) | ≈493.3 | ms | EXTRAPOLATED | k = 2 | A | /plain/1/verifying_mean_ns | target/climb-results.json |
| Medium — verifying per message, A (plain SLH-DSA) | 503.3 | us | MEASURED | n = 1 | A | /plain/2/verifying_mean_ns | target/climb-results.json |
| Medium — verifying per batch, A (plain SLH-DSA) | ≈5.03 | s | EXTRAPOLATED | k = 1 | A | /plain/2/verifying_mean_ns | target/climb-results.json |
| Large — verifying per message, A (plain SLH-DSA) | ≈493.3 | us | EXTRAPOLATED | k = 2 | A | /plain/3/verifying_mean_ns | target/climb-results.json |
| Large — verifying per batch, A (plain SLH-DSA) | ≈8.22 | min | EXTRAPOLATED | k = 2 | A | /plain/3/verifying_mean_ns | target/climb-results.json |
| Small — verifying speed-up (trust cached), C vs A | 144.3 | × | DERIVED | n = 100 | A | /comparisons/0/verifying_speedup_cached_vs_a | target/climb-results.json |
| Small — verifying speed-up (ladder re-validated), C vs A | 0.9645 | × | DERIVED | n = 100 | A | /comparisons/0/verifying_speedup_cold_vs_a | target/climb-results.json |
| OneK — verifying speed-up (trust cached), C vs A | ≈106.7 | × | EXTRAPOLATED | n = 1000 | A | /comparisons/1/verifying_speedup_cached_vs_a | target/climb-results.json |
| OneK — verifying speed-up (ladder re-validated), C vs A | ≈0.937 | × | EXTRAPOLATED | n = 1000 | A | /comparisons/1/verifying_speedup_cold_vs_a | target/climb-results.json |
| Medium — verifying speed-up (trust cached), C vs A | 77.34 | × | DERIVED | n = 10000 | A | /comparisons/2/verifying_speedup_cached_vs_a | target/climb-results.json |
| Medium — verifying speed-up (ladder re-validated), C vs A | 0.9236 | × | DERIVED | n = 10000 | A | /comparisons/2/verifying_speedup_cold_vs_a | target/climb-results.json |
| Large — verifying speed-up (trust cached), C vs A | ≈54.09 | × | EXTRAPOLATED | n = 1000000 | A | /comparisons/3/verifying_speedup_cached_vs_a | target/climb-results.json |
| Large — verifying speed-up (ladder re-validated), C vs A | ≈0.9411 | × | EXTRAPOLATED | n = 1000000 | A | /comparisons/3/verifying_speedup_cold_vs_a | target/climb-results.json |
| Medium — verifying speed-up model (trust cached), C vs A — spread median | 81.4 | × | DERIVED | n = 5 | A | /derived/stability/runs/6/median | target/climb-results.json |
| Medium — verifying speed-up model (trust cached), C vs A — spread minimum | 76.7 | × | DERIVED | n = 5 | A | /derived/stability/runs/6/min | target/climb-results.json |
| Medium — verifying speed-up model (trust cached), C vs A — spread maximum | 84.3 | × | DERIVED | n = 5 | A | /derived/stability/runs/6/max | target/climb-results.json |
| OneK — verifying speed-up model (trust cached), C vs A — spread median | 113.2 | × | DERIVED | n = 5 | A | /derived/stability/runs/7/median | target/climb-results.json |
| OneK — verifying speed-up model (trust cached), C vs A — spread minimum | 106.0 | × | DERIVED | n = 5 | A | /derived/stability/runs/7/min | target/climb-results.json |
| OneK — verifying speed-up model (trust cached), C vs A — spread maximum | 115.8 | × | DERIVED | n = 5 | A | /derived/stability/runs/7/max | target/climb-results.json |
| Small — verifying speed-up model (trust cached), C vs A — spread median | 152.1 | × | DERIVED | n = 5 | A | /derived/stability/runs/8/median | target/climb-results.json |
| Small — verifying speed-up model (trust cached), C vs A — spread minimum | 141.4 | × | DERIVED | n = 5 | A | /derived/stability/runs/8/min | target/climb-results.json |
| Small — verifying speed-up model (trust cached), C vs A — spread maximum | 160.5 | × | DERIVED | n = 5 | A | /derived/stability/runs/8/max | target/climb-results.json |
| Average trust-cached verify speed-up | 409 | × | DERIVED | n = 4 | n/a | /derived/overall_verifying_trust_cached_speedup_avg | target/climb-results.json |

## Memory

> measurement method: isolated-process

| Label | Value | Unit | Evidence class | n / k | Baseline | JSON pointer | Source |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Small — resident median (measured) | 3547136 | B | MEASURED | n = 7 | n/a | /runs/0/median_bytes + /memory/measured/0/median_bytes | target/memory-report.json + target/climb-results.json |
| Small — resident min (measured) | 3502080 | B | MEASURED | n = 7 | n/a | /runs/0/min_bytes + /memory/measured/0/measured_range_min_bytes | target/memory-report.json + target/climb-results.json |
| Small — resident max (measured) | 3645440 | B | MEASURED | n = 7 | n/a | /runs/0/max_bytes + /memory/measured/0/measured_range_max_bytes | target/memory-report.json + target/climb-results.json |
| Small — resident IQR (measured) | 81920 | B | MEASURED | n = 7 | n/a | /runs/0/iqr_bytes + /memory/measured/0/iqr_bytes | target/memory-report.json + target/climb-results.json |
| Medium — resident median (measured) | 13504512 | B | MEASURED | n = 7 | n/a | /runs/1/median_bytes + /memory/measured/1/median_bytes | target/memory-report.json + target/climb-results.json |
| Medium — resident min (measured) | 13422592 | B | MEASURED | n = 7 | n/a | /runs/1/min_bytes + /memory/measured/1/measured_range_min_bytes | target/memory-report.json + target/climb-results.json |
| Medium — resident max (measured) | 13586432 | B | MEASURED | n = 7 | n/a | /runs/1/max_bytes + /memory/measured/1/measured_range_max_bytes | target/memory-report.json + target/climb-results.json |
| Medium — resident IQR (measured) | 69632 | B | MEASURED | n = 7 | n/a | /runs/1/iqr_bytes + /memory/measured/1/iqr_bytes | target/memory-report.json + target/climb-results.json |
| Large — resident median (measured) | 1086554112 | B | MEASURED | n = 7 | n/a | /runs/2/median_bytes + /memory/measured/2/median_bytes | target/memory-report.json + target/climb-results.json |
| Large — resident min (measured) | 1086509056 | B | MEASURED | n = 7 | n/a | /runs/2/min_bytes + /memory/measured/2/measured_range_min_bytes | target/memory-report.json + target/climb-results.json |
| Large — resident max (measured) | 1086644224 | B | MEASURED | n = 7 | n/a | /runs/2/max_bytes + /memory/measured/2/measured_range_max_bytes | target/memory-report.json + target/climb-results.json |
| Large — resident IQR (measured) | 94208 | B | MEASURED | n = 7 | n/a | /runs/2/iqr_bytes + /memory/measured/2/iqr_bytes | target/memory-report.json + target/climb-results.json |
| Small — peak median (measured) | 3547136 | B | MEASURED | n = 7 | n/a | /memory/measured/0/measured_peak_bytes | target/climb-results.json |
| Small — measurement samples | 7 | — | MEASURED | n = 7 | n/a | /memory/measured/0/samples | target/climb-results.json |
| Small — analytical, libMTL pages | 2097152 | B | ANALYTICAL | — | n/a | /memory/measured/0/libmtl_analytical_bytes | target/climb-results.json |
| Small — analytical, harness buffers | 27656 | B | ANALYTICAL | — | n/a | /memory/measured/0/harness_buffers_bytes | target/climb-results.json |
| Small — analytical, dataset buffers | 53600 | B | ANALYTICAL | — | n/a | /memory/measured/0/dataset_buffers_bytes | target/climb-results.json |
| Small — analytical, keypair | 186 | B | ANALYTICAL | — | n/a | /memory/measured/0/keypair_bytes | target/climb-results.json |
| Small — analytical total | 2178594 | B | ANALYTICAL | — | n/a | /memory/measured/0/analytical_total_bytes | target/climb-results.json |
| Small — residual (measured − analytical) | 1368542 | B | DERIVED | n = 7 | n/a | /memory/measured/0/residual_bytes | target/climb-results.json |
| Small — measured / analytical ratio | 1.628 | × | DERIVED | n = 7 | n/a | /memory/measured/0/measured_analytical_total_ratio | target/climb-results.json |
| Medium — peak median (measured) | 13938688 | B | MEASURED | n = 7 | n/a | /memory/measured/1/measured_peak_bytes | target/climb-results.json |
| Medium — measurement samples | 7 | — | MEASURED | n = 7 | n/a | /memory/measured/1/samples | target/climb-results.json |
| Medium — analytical, libMTL pages | 2097152 | B | ANALYTICAL | — | n/a | /memory/measured/1/libmtl_analytical_bytes | target/climb-results.json |
| Medium — analytical, harness buffers | 3063384 | B | ANALYTICAL | — | n/a | /memory/measured/1/harness_buffers_bytes | target/climb-results.json |
| Medium — analytical, dataset buffers | 5360000 | B | ANALYTICAL | — | n/a | /memory/measured/1/dataset_buffers_bytes | target/climb-results.json |
| Medium — analytical, keypair | 186 | B | ANALYTICAL | — | n/a | /memory/measured/1/keypair_bytes | target/climb-results.json |
| Medium — analytical total | 10520722 | B | ANALYTICAL | — | n/a | /memory/measured/1/analytical_total_bytes | target/climb-results.json |
| Medium — residual (measured − analytical) | 2983790 | B | DERIVED | n = 7 | n/a | /memory/measured/1/residual_bytes | target/climb-results.json |
| Medium — measured / analytical ratio | 1.284 | × | DERIVED | n = 7 | n/a | /memory/measured/1/measured_analytical_total_ratio | target/climb-results.json |
| Large — peak median (measured) | 1142620160 | B | MEASURED | n = 7 | n/a | /memory/measured/2/measured_peak_bytes | target/climb-results.json |
| Large — measurement samples | 7 | — | MEASURED | n = 7 | n/a | /memory/measured/2/samples | target/climb-results.json |
| Large — analytical, libMTL pages | 49283072 | B | ANALYTICAL | — | n/a | /memory/measured/2/libmtl_analytical_bytes | target/climb-results.json |
| Large — analytical, harness buffers | 399067864 | B | ANALYTICAL | — | n/a | /memory/measured/2/harness_buffers_bytes | target/climb-results.json |
| Large — analytical, dataset buffers | 536000000 | B | ANALYTICAL | — | n/a | /memory/measured/2/dataset_buffers_bytes | target/climb-results.json |
| Large — analytical, keypair | 186 | B | ANALYTICAL | — | n/a | /memory/measured/2/keypair_bytes | target/climb-results.json |
| Large — analytical total | 984351122 | B | ANALYTICAL | — | n/a | /memory/measured/2/analytical_total_bytes | target/climb-results.json |
| Large — residual (measured − analytical) | 102202990 | B | DERIVED | n = 7 | n/a | /memory/measured/2/residual_bytes | target/climb-results.json |
| Large — measured / analytical ratio | 1.104 | × | DERIVED | n = 7 | n/a | /memory/measured/2/measured_analytical_total_ratio | target/climb-results.json |
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
| Ladder — recorded at | 2026-11-12T14:21:27+05:30 | — | MEASURED | — | n/a | /ladder/recorded_at | target/climb-results.json |

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
| DNS — recorded at | 2026-11-12T18:05:00+05:30 | — | MEASURED | — | n/a | /recorded_at + /dns_pipeline/recorded_at | target/dns-capture.json + target/climb-results.json |
| DNS, www.climb.example. A — reduction, condensed vs full | 97.307 | % | DERIVED | — | B | /dns_pipeline/reduction_percent_www_a | target/climb-results.json |
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
| UDP fit, aggregate — A (plain SLH-DSA), modelled responses fitting the 1,232-byte UDP payload | 0 of 12 | rows | ANALYTICAL | k = 0 / n = 12 | A | /derived/udp_fit/aggregate/0/fit_rows | target/climb-results.json |
| UDP fit, aggregate — B (MTL, no amortisation), modelled responses fitting the 1,232-byte UDP payload | 0 of 12 | rows | ANALYTICAL | k = 0 / n = 12 | B | /derived/udp_fit/aggregate/1/fit_rows | target/climb-results.json |
| UDP fit, aggregate — C (MTL amortised), modelled responses fitting the 1,232-byte UDP payload | 11 of 12 | rows | ANALYTICAL | k = 11 / n = 12 | C | /derived/udp_fit/aggregate/2/fit_rows | target/climb-results.json |
| UDP fit, leaves=100 — A (plain SLH-DSA) Positive A (www) fit ≤1232 B | 0.000 | % | ANALYTICAL | n = 100 | A | /derived/udp_fit/rows/0/fit_1232_percent | target/climb-results.json |
| UDP fit, leaves=1000 — A (plain SLH-DSA) Positive A (www) fit ≤1232 B | 0.000 | % | ANALYTICAL | n = 1000 | A | /derived/udp_fit/rows/1/fit_1232_percent | target/climb-results.json |
| UDP fit, leaves=10000 — A (plain SLH-DSA) Positive A (www) fit ≤1232 B | 0.000 | % | ANALYTICAL | n = 10000 | A | /derived/udp_fit/rows/2/fit_1232_percent | target/climb-results.json |
| UDP fit, leaves=1000000 — A (plain SLH-DSA) Positive A (www) fit ≤1232 B | 0.000 | % | ANALYTICAL | n = 1000000 | A | /derived/udp_fit/rows/3/fit_1232_percent | target/climb-results.json |
| UDP fit, leaves=100 — B (MTL non-amortised) Positive A (www) fit ≤1232 B | 0.000 | % | ANALYTICAL | n = 100 | B | /derived/udp_fit/rows/4/fit_1232_percent | target/climb-results.json |
| UDP fit, leaves=1000 — B (MTL non-amortised) Positive A (www) fit ≤1232 B | 0.000 | % | ANALYTICAL | n = 1000 | B | /derived/udp_fit/rows/5/fit_1232_percent | target/climb-results.json |
| UDP fit, leaves=10000 — B (MTL non-amortised) Positive A (www) fit ≤1232 B | 0.000 | % | ANALYTICAL | n = 10000 | B | /derived/udp_fit/rows/6/fit_1232_percent | target/climb-results.json |
| UDP fit, leaves=1000000 — B (MTL non-amortised) Positive A (www) fit ≤1232 B | 0.000 | % | ANALYTICAL | n = 1000000 | B | /derived/udp_fit/rows/7/fit_1232_percent | target/climb-results.json |
| UDP fit, leaves=100 — C (MTL amortised) Positive A (www) fit ≤1232 B | 100.000 | % | ANALYTICAL | n = 100 | C | /derived/udp_fit/rows/8/fit_1232_percent | target/climb-results.json |
| UDP fit, leaves=1000 — C (MTL amortised) Positive A (www) fit ≤1232 B | 100.000 | % | ANALYTICAL | n = 1000 | C | /derived/udp_fit/rows/9/fit_1232_percent | target/climb-results.json |
| UDP fit, leaves=10000 — C (MTL amortised) Positive A (www) fit ≤1232 B | 100.000 | % | ANALYTICAL | n = 10000 | C | /derived/udp_fit/rows/10/fit_1232_percent | target/climb-results.json |
| UDP fit, leaves=1000000 — C (MTL amortised) Positive A (www) fit ≤1232 B | 100.000 | % | ANALYTICAL | n = 1000000 | C | /derived/udp_fit/rows/11/fit_1232_percent | target/climb-results.json |
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
| UDP fit, leaves=100 — A (plain SLH-DSA) Positive A with NS fit ≤1232 B | 0.000 | % | ANALYTICAL | n = 100 | A | /derived/udp_fit/rows/24/fit_1232_percent | target/climb-results.json |
| UDP fit, leaves=1000 — A (plain SLH-DSA) Positive A with NS fit ≤1232 B | 0.000 | % | ANALYTICAL | n = 1000 | A | /derived/udp_fit/rows/25/fit_1232_percent | target/climb-results.json |
| UDP fit, leaves=10000 — A (plain SLH-DSA) Positive A with NS fit ≤1232 B | 0.000 | % | ANALYTICAL | n = 10000 | A | /derived/udp_fit/rows/26/fit_1232_percent | target/climb-results.json |
| UDP fit, leaves=1000000 — A (plain SLH-DSA) Positive A with NS fit ≤1232 B | 0.000 | % | ANALYTICAL | n = 1000000 | A | /derived/udp_fit/rows/27/fit_1232_percent | target/climb-results.json |
| UDP fit, leaves=100 — B (MTL non-amortised) Positive A with NS fit ≤1232 B | 0.000 | % | ANALYTICAL | n = 100 | B | /derived/udp_fit/rows/28/fit_1232_percent | target/climb-results.json |
| UDP fit, leaves=1000 — B (MTL non-amortised) Positive A with NS fit ≤1232 B | 0.000 | % | ANALYTICAL | n = 1000 | B | /derived/udp_fit/rows/29/fit_1232_percent | target/climb-results.json |
| UDP fit, leaves=10000 — B (MTL non-amortised) Positive A with NS fit ≤1232 B | 0.000 | % | ANALYTICAL | n = 10000 | B | /derived/udp_fit/rows/30/fit_1232_percent | target/climb-results.json |
| UDP fit, leaves=1000000 — B (MTL non-amortised) Positive A with NS fit ≤1232 B | 0.000 | % | ANALYTICAL | n = 1000000 | B | /derived/udp_fit/rows/31/fit_1232_percent | target/climb-results.json |
| UDP fit, leaves=100 — C (MTL amortised) Positive A with NS fit ≤1232 B | 100.000 | % | ANALYTICAL | n = 100 | C | /derived/udp_fit/rows/32/fit_1232_percent | target/climb-results.json |
| UDP fit, leaves=1000 — C (MTL amortised) Positive A with NS fit ≤1232 B | 100.000 | % | ANALYTICAL | n = 1000 | C | /derived/udp_fit/rows/33/fit_1232_percent | target/climb-results.json |
| UDP fit, leaves=10000 — C (MTL amortised) Positive A with NS fit ≤1232 B | 100.000 | % | ANALYTICAL | n = 10000 | C | /derived/udp_fit/rows/34/fit_1232_percent | target/climb-results.json |
| UDP fit, leaves=1000000 — C (MTL amortised) Positive A with NS fit ≤1232 B | 0.358 | % | ANALYTICAL | n = 1000000 | C | /derived/udp_fit/rows/35/fit_1232_percent | target/climb-results.json |
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

## Classical baselines

| Label | Value | Unit | Evidence class | n / k | Baseline | JSON pointer | Source |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Small — Ed25519 signature bytes | 64 | B | MEASURED | — | n/a | /classical/0/signature_len_bytes | target/climb-results.json |
| Small — Ed25519 signing per message | 15.90 | µs | MEASURED | — | n/a | /classical/0/signing_mean_ns | target/climb-results.json |
| Small — Ed25519 verify per message | 28.57 | µs | MEASURED | — | n/a | /classical/0/verifying_mean_ns | target/climb-results.json |
| Medium — Ed25519 signature bytes | 64 | B | MEASURED | — | n/a | /classical/1/signature_len_bytes | target/climb-results.json |
| Medium — Ed25519 signing per message | 15.95 | µs | MEASURED | — | n/a | /classical/1/signing_mean_ns | target/climb-results.json |
| Medium — Ed25519 verify per message | 28.59 | µs | MEASURED | — | n/a | /classical/1/verifying_mean_ns | target/climb-results.json |
| Small — ECDSA P-256 signature bytes | 64 | B | MEASURED | — | n/a | /classical/2/signature_len_bytes | target/climb-results.json |
| Small — ECDSA P-256 signing per message | 153.95 | µs | MEASURED | — | n/a | /classical/2/signing_mean_ns | target/climb-results.json |
| Small — ECDSA P-256 verify per message | 260.91 | µs | MEASURED | — | n/a | /classical/2/verifying_mean_ns | target/climb-results.json |
| Medium — ECDSA P-256 signature bytes | 64 | B | MEASURED | — | n/a | /classical/3/signature_len_bytes | target/climb-results.json |
| Medium — ECDSA P-256 signing per message | 154.10 | µs | MEASURED | — | n/a | /classical/3/signing_mean_ns | target/climb-results.json |
| Medium — ECDSA P-256 verify per message | 261.46 | µs | MEASURED | — | n/a | /classical/3/verifying_mean_ns | target/climb-results.json |
| Small — RSA-2048 signature bytes | 256 | B | MEASURED | — | n/a | /classical/4/signature_len_bytes | target/climb-results.json |
| Small — RSA-2048 signing per message | 1541.04 | µs | MEASURED | — | n/a | /classical/4/signing_mean_ns | target/climb-results.json |
| Small — RSA-2048 verify per message | 197.57 | µs | MEASURED | — | n/a | /classical/4/verifying_mean_ns | target/climb-results.json |
| Medium — RSA-2048 signature bytes | 256 | B | MEASURED | — | n/a | /classical/5/signature_len_bytes | target/climb-results.json |
| Medium — RSA-2048 signing per message | 1533.33 | µs | MEASURED | — | n/a | /classical/5/signing_mean_ns | target/climb-results.json |
| Medium — RSA-2048 verify per message | 198.53 | µs | MEASURED | — | n/a | /classical/5/verifying_mean_ns | target/climb-results.json |

## Replication

> one host recorded; awaiting a second host's climb-results.json and environment.json under data/replication/<host-hash>/

| Label | Value | Unit | Evidence class | n / k | Baseline | JSON pointer | Source |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Host efe1a17f1912 — CPU | 11th Gen Intel(R) Core(TM) i3-1115G4 @ 3.00GHz | — | DERIVED | — | n/a | /replication/hosts/0/cpu_model | target/climb-results.json |
| Host efe1a17f1912 — governor | performance | — | DERIVED | — | n/a | /replication/hosts/0/cpu_governor | target/climb-results.json |
| Host efe1a17f1912 — kernel | 7.2.14-arch1-1 | — | DERIVED | — | n/a | /replication/hosts/0/kernel_release | target/climb-results.json |
| Host efe1a17f1912 — rustc | rustc 1.99.1 (58cf8f9c3 2026-10-13) (Arch Linux rust 1:1.99.1-2) | — | DERIVED | — | n/a | /replication/hosts/0/rustc_version | target/climb-results.json |
| Host efe1a17f1912 — headline ratios | 13 | n | DERIVED | — | n/a | /replication/hosts/0/ratios | target/climb-results.json |
| Replication — declared tolerance | 10 | % | DERIVED | — | n/a | /replication/tolerance_percent | target/climb-results.json |

## DNS latency (emulated RTT)

> emulated RTT, local bridge

> kernel netem: half the target RTT on client egress and half on the queried server's egress, filtered to the client address

| Label | Value | Unit | Evidence class | n / k | Baseline | JSON pointer | Source |
| --- | --- | --- | --- | --- | --- | --- | --- |
| nsd / plain / 0 ms — median | 0.096 | ms | MEASURED | n = 200 | n/a | /dns_rtt/cells/0/median_ms | target/climb-results.json |
| nsd / plain / 0 ms — p95 | 0.124 | ms | MEASURED | n = 200 | n/a | /dns_rtt/cells/0/p95_ms | target/climb-results.json |
| nsd / plain / 0 ms — samples | 200 | n | MEASURED | — | n/a | /dns_rtt/cells/0/samples | target/climb-results.json |
| nsd / plain / 0 ms — truncated | 200 | n | MEASURED | — | n/a | /dns_rtt/cells/0/truncated | target/climb-results.json |
| nsd / mtl-condensed / 0 ms — median | 0.068 | ms | MEASURED | n = 200 | n/a | /dns_rtt/cells/1/median_ms | target/climb-results.json |
| nsd / mtl-condensed / 0 ms — p95 | 0.075 | ms | MEASURED | n = 200 | n/a | /dns_rtt/cells/1/p95_ms | target/climb-results.json |
| nsd / mtl-condensed / 0 ms — samples | 200 | n | MEASURED | — | n/a | /dns_rtt/cells/1/samples | target/climb-results.json |
| nsd / mtl-condensed / 0 ms — truncated | 0 | n | MEASURED | — | n/a | /dns_rtt/cells/1/truncated | target/climb-results.json |
| unbound / plain / 0 ms — median | 0.067 | ms | MEASURED | n = 200 | n/a | /dns_rtt/cells/2/median_ms | target/climb-results.json |
| unbound / plain / 0 ms — p95 | 0.079 | ms | MEASURED | n = 200 | n/a | /dns_rtt/cells/2/p95_ms | target/climb-results.json |
| unbound / plain / 0 ms — samples | 200 | n | MEASURED | — | n/a | /dns_rtt/cells/2/samples | target/climb-results.json |
| unbound / plain / 0 ms — truncated | 200 | n | MEASURED | — | n/a | /dns_rtt/cells/2/truncated | target/climb-results.json |
| unbound / mtl-condensed / 0 ms — median | 0.092 | ms | MEASURED | n = 200 | n/a | /dns_rtt/cells/3/median_ms | target/climb-results.json |
| unbound / mtl-condensed / 0 ms — p95 | 0.100 | ms | MEASURED | n = 200 | n/a | /dns_rtt/cells/3/p95_ms | target/climb-results.json |
| unbound / mtl-condensed / 0 ms — samples | 200 | n | MEASURED | — | n/a | /dns_rtt/cells/3/samples | target/climb-results.json |
| unbound / mtl-condensed / 0 ms — truncated | 200 | n | MEASURED | — | n/a | /dns_rtt/cells/3/truncated | target/climb-results.json |
| nsd / plain / 20 ms — median | 81.018 | ms | MEASURED | n = 200 | n/a | /dns_rtt/cells/4/median_ms | target/climb-results.json |
| nsd / plain / 20 ms — p95 | 81.442 | ms | MEASURED | n = 200 | n/a | /dns_rtt/cells/4/p95_ms | target/climb-results.json |
| nsd / plain / 20 ms — samples | 200 | n | MEASURED | — | n/a | /dns_rtt/cells/4/samples | target/climb-results.json |
| nsd / plain / 20 ms — truncated | 200 | n | MEASURED | — | n/a | /dns_rtt/cells/4/truncated | target/climb-results.json |
| nsd / mtl-condensed / 20 ms — median | 20.470 | ms | MEASURED | n = 200 | n/a | /dns_rtt/cells/5/median_ms | target/climb-results.json |
| nsd / mtl-condensed / 20 ms — p95 | 20.658 | ms | MEASURED | n = 200 | n/a | /dns_rtt/cells/5/p95_ms | target/climb-results.json |
| nsd / mtl-condensed / 20 ms — samples | 200 | n | MEASURED | — | n/a | /dns_rtt/cells/5/samples | target/climb-results.json |
| nsd / mtl-condensed / 20 ms — truncated | 0 | n | MEASURED | — | n/a | /dns_rtt/cells/5/truncated | target/climb-results.json |
| unbound / plain / 20 ms — median | 60.799 | ms | MEASURED | n = 200 | n/a | /dns_rtt/cells/6/median_ms | target/climb-results.json |
| unbound / plain / 20 ms — p95 | 61.141 | ms | MEASURED | n = 200 | n/a | /dns_rtt/cells/6/p95_ms | target/climb-results.json |
| unbound / plain / 20 ms — samples | 200 | n | MEASURED | — | n/a | /dns_rtt/cells/6/samples | target/climb-results.json |
| unbound / plain / 20 ms — truncated | 200 | n | MEASURED | — | n/a | /dns_rtt/cells/6/truncated | target/climb-results.json |
| unbound / mtl-condensed / 20 ms — median | 60.843 | ms | MEASURED | n = 200 | n/a | /dns_rtt/cells/7/median_ms | target/climb-results.json |
| unbound / mtl-condensed / 20 ms — p95 | 61.141 | ms | MEASURED | n = 200 | n/a | /dns_rtt/cells/7/p95_ms | target/climb-results.json |
| unbound / mtl-condensed / 20 ms — samples | 200 | n | MEASURED | — | n/a | /dns_rtt/cells/7/samples | target/climb-results.json |
| unbound / mtl-condensed / 20 ms — truncated | 200 | n | MEASURED | — | n/a | /dns_rtt/cells/7/truncated | target/climb-results.json |
| nsd / plain / 50 ms — median | 201.056 | ms | MEASURED | n = 200 | n/a | /dns_rtt/cells/8/median_ms | target/climb-results.json |
| nsd / plain / 50 ms — p95 | 201.417 | ms | MEASURED | n = 200 | n/a | /dns_rtt/cells/8/p95_ms | target/climb-results.json |
| nsd / plain / 50 ms — samples | 200 | n | MEASURED | — | n/a | /dns_rtt/cells/8/samples | target/climb-results.json |
| nsd / plain / 50 ms — truncated | 200 | n | MEASURED | — | n/a | /dns_rtt/cells/8/truncated | target/climb-results.json |
| nsd / mtl-condensed / 50 ms — median | 50.475 | ms | MEASURED | n = 200 | n/a | /dns_rtt/cells/9/median_ms | target/climb-results.json |
| nsd / mtl-condensed / 50 ms — p95 | 50.718 | ms | MEASURED | n = 200 | n/a | /dns_rtt/cells/9/p95_ms | target/climb-results.json |
| nsd / mtl-condensed / 50 ms — samples | 200 | n | MEASURED | — | n/a | /dns_rtt/cells/9/samples | target/climb-results.json |
| nsd / mtl-condensed / 50 ms — truncated | 0 | n | MEASURED | — | n/a | /dns_rtt/cells/9/truncated | target/climb-results.json |
| unbound / plain / 50 ms — median | 150.826 | ms | MEASURED | n = 200 | n/a | /dns_rtt/cells/10/median_ms | target/climb-results.json |
| unbound / plain / 50 ms — p95 | 151.166 | ms | MEASURED | n = 200 | n/a | /dns_rtt/cells/10/p95_ms | target/climb-results.json |
| unbound / plain / 50 ms — samples | 200 | n | MEASURED | — | n/a | /dns_rtt/cells/10/samples | target/climb-results.json |
| unbound / plain / 50 ms — truncated | 200 | n | MEASURED | — | n/a | /dns_rtt/cells/10/truncated | target/climb-results.json |
| unbound / mtl-condensed / 50 ms — median | 150.772 | ms | MEASURED | n = 200 | n/a | /dns_rtt/cells/11/median_ms | target/climb-results.json |
| unbound / mtl-condensed / 50 ms — p95 | 151.061 | ms | MEASURED | n = 200 | n/a | /dns_rtt/cells/11/p95_ms | target/climb-results.json |
| unbound / mtl-condensed / 50 ms — samples | 200 | n | MEASURED | — | n/a | /dns_rtt/cells/11/samples | target/climb-results.json |
| unbound / mtl-condensed / 50 ms — truncated | 200 | n | MEASURED | — | n/a | /dns_rtt/cells/11/truncated | target/climb-results.json |
| nsd / plain / 100 ms — median | 401.021 | ms | MEASURED | n = 200 | n/a | /dns_rtt/cells/12/median_ms | target/climb-results.json |
| nsd / plain / 100 ms — p95 | 401.483 | ms | MEASURED | n = 200 | n/a | /dns_rtt/cells/12/p95_ms | target/climb-results.json |
| nsd / plain / 100 ms — samples | 200 | n | MEASURED | — | n/a | /dns_rtt/cells/12/samples | target/climb-results.json |
| nsd / plain / 100 ms — truncated | 200 | n | MEASURED | — | n/a | /dns_rtt/cells/12/truncated | target/climb-results.json |
| nsd / mtl-condensed / 100 ms — median | 100.416 | ms | MEASURED | n = 200 | n/a | /dns_rtt/cells/13/median_ms | target/climb-results.json |
| nsd / mtl-condensed / 100 ms — p95 | 100.819 | ms | MEASURED | n = 200 | n/a | /dns_rtt/cells/13/p95_ms | target/climb-results.json |
| nsd / mtl-condensed / 100 ms — samples | 200 | n | MEASURED | — | n/a | /dns_rtt/cells/13/samples | target/climb-results.json |
| nsd / mtl-condensed / 100 ms — truncated | 0 | n | MEASURED | — | n/a | /dns_rtt/cells/13/truncated | target/climb-results.json |
| unbound / plain / 100 ms — median | 300.777 | ms | MEASURED | n = 200 | n/a | /dns_rtt/cells/14/median_ms | target/climb-results.json |
| unbound / plain / 100 ms — p95 | 301.223 | ms | MEASURED | n = 200 | n/a | /dns_rtt/cells/14/p95_ms | target/climb-results.json |
| unbound / plain / 100 ms — samples | 200 | n | MEASURED | — | n/a | /dns_rtt/cells/14/samples | target/climb-results.json |
| unbound / plain / 100 ms — truncated | 200 | n | MEASURED | — | n/a | /dns_rtt/cells/14/truncated | target/climb-results.json |
| unbound / mtl-condensed / 100 ms — median | 300.774 | ms | MEASURED | n = 200 | n/a | /dns_rtt/cells/15/median_ms | target/climb-results.json |
| unbound / mtl-condensed / 100 ms — p95 | 301.320 | ms | MEASURED | n = 200 | n/a | /dns_rtt/cells/15/p95_ms | target/climb-results.json |
| unbound / mtl-condensed / 100 ms — samples | 200 | n | MEASURED | — | n/a | /dns_rtt/cells/15/samples | target/climb-results.json |
| unbound / mtl-condensed / 100 ms — truncated | 200 | n | MEASURED | — | n/a | /dns_rtt/cells/15/truncated | target/climb-results.json |

## Resolver cache

| Label | Value | Unit | Evidence class | n / k | Baseline | JSON pointer | Source |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Small — resolver cache bytes, MTL-condensed (measured) | 921471 | B | MEASURED | n = 100 | n/a | /cache_use/analytical/0/measured_bytes | target/climb-results.json |
| Small — resolver cache bytes, MTL-condensed (analytical) | 24392 | B | ANALYTICAL | n = 100 | n/a | /cache_use/analytical/0/analytical_bytes | target/climb-results.json |
| Small — resolver cache measured / analytical, MTL-condensed | 37.78 | × | DERIVED | n = 100 | n/a | /cache_use/analytical/0/measured_over_analytical | target/climb-results.json |
| Small — resolver cache bytes, plain (measured) | 921471 | B | MEASURED | n = 100 | n/a | /cache_use/analytical/1/measured_bytes | target/climb-results.json |
| Small — resolver cache bytes, plain (analytical) | 815600 | B | ANALYTICAL | n = 100 | n/a | /cache_use/analytical/1/analytical_bytes | target/climb-results.json |
| Small — resolver cache measured / analytical, plain | 1.13 | × | DERIVED | n = 100 | n/a | /cache_use/analytical/1/measured_over_analytical | target/climb-results.json |
| Medium — resolver cache bytes, MTL-condensed (measured) | 8429932 | B | MEASURED | n = 10000 | n/a | /cache_use/analytical/2/measured_bytes | target/climb-results.json |
| Medium — resolver cache bytes, MTL-condensed (analytical) | 2738056 | B | ANALYTICAL | n = 10000 | n/a | /cache_use/analytical/2/analytical_bytes | target/climb-results.json |
| Medium — resolver cache measured / analytical, MTL-condensed | 3.079 | × | DERIVED | n = 10000 | n/a | /cache_use/analytical/2/measured_over_analytical | target/climb-results.json |
| Medium — resolver cache bytes, plain (measured) | 8429932 | B | MEASURED | n = 10000 | n/a | /cache_use/analytical/3/measured_bytes | target/climb-results.json |
| Medium — resolver cache bytes, plain (analytical) | 83290000 | B | ANALYTICAL | n = 10000 | n/a | /cache_use/analytical/3/analytical_bytes | target/climb-results.json |
| Medium — resolver cache measured / analytical, plain | 0.1012 | × | DERIVED | n = 10000 | n/a | /cache_use/analytical/3/measured_over_analytical | target/climb-results.json |

## Environment

| Label | Value | Unit | Evidence class | n / k | Baseline | JSON pointer | Source |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Environment — timestamp | 2026-11-12T14:18:05+05:30 | — | MEASURED | — | n/a | /timestamp + /environment/timestamp | environment.json + target/climb-results.json |
| Environment — rustc version | rustc 1.99.1 (58cf8f9c3 2026-10-13) (Arch Linux rust 1:1.99.1-2) | — | MEASURED | — | n/a | /toolchain/rustc_version + /environment/toolchain/rustc_version | environment.json + target/climb-results.json |
| Environment — cargo version | cargo 1.99.1 (5f94df478 2026-08-27) (Arch Linux rust 1:1.99.1-2) | — | MEASURED | — | n/a | /toolchain/cargo_version + /environment/toolchain/cargo_version | environment.json + target/climb-results.json |
| Environment — kernel release | 7.2.14-arch1-1 | — | MEASURED | — | n/a | /system/kernel_release + /environment/system/kernel_release | environment.json + target/climb-results.json |
| Environment — CPU model | 11th Gen Intel(R) Core(TM) i3-1115G4 @ 3.00GHz | — | MEASURED | — | n/a | /cpu/model_name + /environment/cpu/model_name | environment.json + target/climb-results.json |
| Environment — CPU governor | performance | — | MEASURED | — | n/a | /benchmark_runtime/cpu_governor + /environment/benchmark_runtime/cpu_governor | environment.json + target/climb-results.json |
| Environment — pin expression | taskset -c 2 | — | MEASURED | — | n/a | /benchmark_runtime/pin_expression + /environment/benchmark_runtime/pin_expression | environment.json + target/climb-results.json |
| Environment — logical CPUs | 4 | — | MEASURED | — | n/a | /cpu/logical_cpus + /environment/cpu/logical_cpus | environment.json + target/climb-results.json |
| Environment — physical cores | 2 | — | MEASURED | — | n/a | /cpu/physical_cores + /environment/cpu/physical_cores | environment.json + target/climb-results.json |
| Environment — pinned CPU | 2 | — | MEASURED | — | n/a | /benchmark_runtime/pin_cpu + /environment/benchmark_runtime/pin_cpu | environment.json + target/climb-results.json |
| Submodule third_party/MTL — commit SHA | 53ce25dcc35c15e051c5263e41aaeaede18a8aa7 | — | MEASURED | — | n/a | /submodules/0/sha + /environment/submodules/0/sha | environment.json + target/climb-results.json |
| Submodule third_party/liboqs — commit SHA | 5a1a854b0dc9f2141bdc771c555ee60c37950183 | — | MEASURED | — | n/a | /submodules/1/sha + /environment/submodules/1/sha | environment.json + target/climb-results.json |
| Repository — commit SHA | 0ca25cd6758bc77a63b87d4d2b64754d6a8b223d | — | UNCLASSIFIED | — | n/a | n/a | git rev-parse HEAD |

## Quality

| Label | Value | Unit | Evidence class | n / k | Baseline | JSON pointer | Source |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Workspace tests — passed | 1420 | — | MEASURED | n = 49 | n/a | n/a | cargo test --workspace |
| Workspace tests — failed | 0 | — | MEASURED | n = 49 | n/a | n/a | cargo test --workspace |
| Workspace tests — ignored | 3 | — | MEASURED | n = 49 | n/a | n/a | cargo test --workspace |
| Workspace test targets (summary lines) | 49 | — | MEASURED | n = 49 | n/a | n/a | cargo test --workspace |

---

## Footer

- Rows rendered: 497 (from 657 collected facts; 160 duplicate copies merged).
- Evidence classes (rendered rows): MEASURED: 291, ANALYTICAL: 75, EXTRAPOLATED: 24, DERIVED: 106; UNCLASSIFIED: 1.
- UNCLASSIFIED rows: Repository — commit SHA.
- Inconsistencies: none.
