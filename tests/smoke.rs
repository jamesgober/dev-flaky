use dev_flaky::{FlakyResult, FlakyRun, TestReliability};

#[test]
fn smoke_default_iterations() {
    let r = FlakyRun::new("x", "0.1.0");
    assert_eq!(r.iteration_count(), 10);
}

#[test]
fn smoke_iterations_minimum_is_two() {
    let r = FlakyRun::new("x", "0.1.0").iterations(0);
    assert_eq!(r.iteration_count(), 2);
    let r = FlakyRun::new("x", "0.1.0").iterations(1);
    assert_eq!(r.iteration_count(), 2);
}

#[test]
fn smoke_stable_test() {
    let t = TestReliability {
        name: "ok".into(),
        passes: 10,
        failures: 0,
    };
    assert!(t.is_stable());
    assert_eq!(t.reliability(), 1.0);
}

#[test]
fn smoke_broken_test() {
    let t = TestReliability {
        name: "bad".into(),
        passes: 0,
        failures: 10,
    };
    assert!(t.is_broken());
    assert_eq!(t.reliability(), 0.0);
}

#[test]
fn smoke_flaky_test() {
    let t = TestReliability {
        name: "flaky".into(),
        passes: 6,
        failures: 4,
    };
    assert!(t.is_flaky());
    assert!((t.reliability() - 0.6).abs() < 0.0001);
}

#[test]
fn smoke_empty_result_passes() {
    let r = FlakyResult {
        name: "x".into(),
        version: "0.1.0".into(),
        iterations: 10,
        tests: Vec::new(),
    };
    assert!(r.into_report().passed());
}

#[test]
fn smoke_broken_test_produces_failing_report() {
    let r = FlakyResult {
        name: "x".into(),
        version: "0.1.0".into(),
        iterations: 10,
        tests: vec![TestReliability {
            name: "broken".into(),
            passes: 0,
            failures: 10,
        }],
    };
    assert!(r.into_report().failed());
}

#[test]
fn smoke_flaky_count() {
    let r = FlakyResult {
        name: "x".into(),
        version: "0.1.0".into(),
        iterations: 10,
        tests: vec![
            TestReliability {
                name: "stable".into(),
                passes: 10,
                failures: 0,
            },
            TestReliability {
                name: "flaky".into(),
                passes: 5,
                failures: 5,
            },
        ],
    };
    assert_eq!(r.flaky_count(), 1);
}
