//! Public-API smoke tests.
//!
//! The real-subprocess path is exercised only when `cargo` is on PATH
//! and `CARGO_TARGET_DIR` points outside the workspace (see the
//! `#[ignore]`d test at the bottom for the workaround documentation).

use dev_flaky::{Classification, FlakyResult, FlakyRun, TestReliability};

fn t(name: &str, passes: u32, failures: u32) -> TestReliability {
    TestReliability {
        name: name.into(),
        passes,
        failures,
    }
}

fn make_result(tests: Vec<TestReliability>) -> FlakyResult {
    FlakyResult {
        name: "x".into(),
        version: "0.1.0".into(),
        iterations: 10,
        tests,
        reliability_threshold_pct: None,
    }
}

#[test]
fn default_iterations_is_ten() {
    let r = FlakyRun::new("x", "0.1.0");
    assert_eq!(r.iteration_count(), 10);
}

#[test]
fn iterations_clamped_to_min_two() {
    let r = FlakyRun::new("x", "0.1.0").iterations(0);
    assert_eq!(r.iteration_count(), 2);
    let r = FlakyRun::new("x", "0.1.0").iterations(1);
    assert_eq!(r.iteration_count(), 2);
}

#[test]
fn run_builder_chains() {
    let _r = FlakyRun::new("x", "0.1.0")
        .iterations(20)
        .workspace()
        .features("foo")
        .test_filter("integration::")
        .allow("known_flaky")
        .allow_all(["a", "b"])
        .reliability_threshold(99.0);
}

#[test]
fn stable_test_classified_correctly() {
    let r = t("ok", 10, 0);
    assert!(r.is_stable());
    assert_eq!(r.reliability(), 1.0);
    assert_eq!(r.classification(None), Classification::Stable);
}

#[test]
fn broken_test_classified_correctly() {
    let r = t("bad", 0, 10);
    assert!(r.is_broken());
    assert_eq!(r.reliability(), 0.0);
    assert_eq!(r.classification(None), Classification::Broken);
}

#[test]
fn flaky_test_classified_correctly() {
    let r = t("flaky", 6, 4);
    assert!(r.is_flaky());
    assert!((r.reliability() - 0.6).abs() < 0.0001);
    assert_eq!(r.classification(None), Classification::Flaky);
}

#[test]
fn empty_result_passes() {
    let r = make_result(Vec::new());
    assert!(r.into_report().passed());
}

#[test]
fn broken_test_produces_failing_report() {
    let r = make_result(vec![t("broken", 0, 10)]);
    assert!(r.into_report().failed());
}

#[test]
fn flaky_count_works() {
    let r = make_result(vec![t("stable", 10, 0), t("flaky", 5, 5)]);
    assert_eq!(r.flaky_count(), 1);
    assert_eq!(r.stable_count(), 1);
    assert_eq!(r.broken_count(), 0);
    assert_eq!(r.total_count(), 2);
}

/// Real subprocess test. Skipped by default.
///
/// Run with `cargo` available and `CARGO_TARGET_DIR` pointing outside
/// the workspace so the inner `cargo test` invocations don't fight the
/// outer one for the target-dir lock:
///
/// ```text
/// CARGO_TARGET_DIR=/tmp/flaky-target cargo test -- --ignored
/// ```
#[test]
#[ignore = "requires cargo + CARGO_TARGET_DIR outside the workspace"]
fn execute_against_real_cargo() {
    let run = FlakyRun::new("dev-flaky", "0.9.0").iterations(2);
    let res = run.execute().expect("cargo on PATH");
    let _ = res.into_report();
}
