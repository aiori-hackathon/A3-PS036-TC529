//! Unit tests for `climb_report::record` (moved to integration-test level
//! because the module exceeded the 250-LOC ceiling).
//!
//! Uses temporary directories for file-based tests.

use climb_report::{read_leaf, scan_criterion, ReportError};
use std::path::{Path, PathBuf};

fn temp_root() -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir =
        std::env::temp_dir().join(format!("climb-report-rectest-{}-{}", std::process::id(), n));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn make_leaf(dir: &Path, benchmark_json: &str, estimates_json: &str) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(dir.join("benchmark.json"), benchmark_json).unwrap();
    std::fs::write(dir.join("estimates.json"), estimates_json).unwrap();
}

const BENCH_JSON_ELEMENTS: &str = r#"{"group_id":"signing_small_with_mtl","function_id":"sign_batch","value_str":"100","throughput":{"Elements":100},"full_id":"signing_small_with_mtl/sign_batch/100","directory_name":"signing_small_with_mtl/sign_batch/100","title":"signing_small_with_mtl/sign_batch/100"}"#;

const EST_JSON: &str = r#"{"mean":{"confidence_interval":{"confidence_level":0.95,"lower_bound":667213713.59225,"upper_bound":701589033.68},"point_estimate":682919952.51,"standard_error":8847633.63015166},"median":{"point_estimate":670836473.8,"confidence_interval":{"confidence_level":0.95,"lower_bound":654598000.0,"upper_bound":672576324.96}},"median_abs_dev":{"point_estimate":5771345.559999999,"confidence_interval":{"confidence_level":0.95,"lower_bound":230128.0,"upper_bound":17172101.019999996}},"slope":null,"std_dev":{"point_estimate":47786218.50616897,"confidence_interval":{"confidence_level":0.95,"lower_bound":37305846.547646426,"upper_bound":57952360.33718598}}}"#;

#[test]
fn read_leaf_parses_realistic_benchmark_and_estimates() {
    let root = temp_root();
    let leaf = root.join("new");
    make_leaf(&leaf, BENCH_JSON_ELEMENTS, EST_JSON);

    let record = read_leaf(&leaf).unwrap();

    assert_eq!(record.group_id, "signing_small_with_mtl");
    assert_eq!(record.function_id, "sign_batch");
    assert_eq!(record.value_str, "100");
    assert_eq!(record.full_id, "signing_small_with_mtl/sign_batch/100");
    assert_eq!(record.elements, Some(100));
    assert!((record.mean_ns - 682_919_952.51).abs() < 1.0);
    assert!((record.mean_ci_lower_ns - 667_213_713.592_25).abs() < 1.0);
    assert!((record.mean_ci_upper_ns - 701_589_033.68).abs() < 1.0);
    assert!((record.confidence_level - 0.95).abs() < 0.001);
}

#[test]
fn read_leaf_throughput_bytes() {
    let ben = r#"{"group_id":"g","function_id":"f","value_str":"1","throughput":{"Bytes":4096},"full_id":"g/f/1","directory_name":"g/f/1","title":"g/f/1"}"#;
    let root = temp_root();
    let leaf = root.join("new");
    make_leaf(&leaf, ben, EST_JSON);

    let record = read_leaf(&leaf).unwrap();
    assert_eq!(record.elements, Some(4096));
}

#[test]
fn read_leaf_throughput_absent() {
    let ben = r#"{"group_id":"g","function_id":"f","value_str":"1","full_id":"g/f/1","directory_name":"g/f/1","title":"g/f/1"}"#;
    let root = temp_root();
    let leaf = root.join("new");
    make_leaf(&leaf, ben, EST_JSON);

    let record = read_leaf(&leaf).unwrap();
    assert_eq!(record.elements, None);
}

#[test]
fn read_leaf_missing_benchmark_json_returns_error() {
    let root = temp_root();
    let leaf = root.join("new");
    std::fs::create_dir_all(&leaf).unwrap();
    std::fs::write(leaf.join("estimates.json"), EST_JSON).unwrap();

    let err = read_leaf(&leaf).unwrap_err();
    assert!(matches!(err, ReportError::MissingFile { .. }));
}

#[test]
fn read_leaf_missing_estimates_json_returns_error() {
    let root = temp_root();
    let leaf = root.join("new");
    std::fs::create_dir_all(&leaf).unwrap();
    std::fs::write(leaf.join("benchmark.json"), BENCH_JSON_ELEMENTS).unwrap();

    let err = read_leaf(&leaf).unwrap_err();
    assert!(matches!(err, ReportError::MissingFile { .. }));
}

#[test]
fn scan_criterion_finds_baseline_leaves() {
    let root = temp_root();

    let ben_a = r#"{"group_id":"group_a","function_id":"func_a","value_str":"100","throughput":{"Elements":100},"full_id":"group_a/func_a/100","directory_name":"group_a/func_a/100","title":"group_a/func_a/100"}"#;
    let leaf_a = root.join("group_a").join("func_a").join("100").join("new");
    make_leaf(&leaf_a, ben_a, EST_JSON);

    let ben_b = r#"{"group_id":"group_b","function_id":"func_b","value_str":"200","throughput":{"Elements":200},"full_id":"group_b/func_b/200","directory_name":"group_b/func_b/200","title":"group_b/func_b/200"}"#;
    let leaf_b = root.join("group_b").join("func_b").join("200").join("new");
    make_leaf(&leaf_b, ben_b, EST_JSON);

    let old_leaf = root.join("group_a").join("func_a").join("100").join("base");
    make_leaf(&old_leaf, ben_a, EST_JSON);

    let records = scan_criterion(&root, "new").unwrap();
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].group_id, "group_a");
    assert_eq!(records[1].group_id, "group_b");
}

#[test]
fn scan_criterion_ordering_is_deterministic() {
    let root = temp_root();

    for (gid, vid) in [("z_group", "100"), ("a_group", "200")] {
        let ben = format!(
            r#"{{"group_id":"{}","function_id":"f","value_str":"{}","throughput":{{"Elements":1}},"full_id":"{}/f/{}","directory_name":"{}/f/{}","title":"{}/f/{}"}}"#,
            gid, vid, gid, vid, gid, vid, gid, vid
        );
        let leaf = root.join(gid).join("f").join(vid).join("new");
        make_leaf(&leaf, &ben, EST_JSON);
    }

    let records = scan_criterion(&root, "new").unwrap();
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].group_id, "a_group");
    assert_eq!(records[1].group_id, "z_group");
}

#[test]
fn scan_criterion_missing_root_returns_error() {
    let err = scan_criterion(Path::new("/nonexistent/path/for/test"), "new").unwrap_err();
    assert!(matches!(err, ReportError::MissingRoot { .. }));
}
