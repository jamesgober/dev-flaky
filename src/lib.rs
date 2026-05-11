//! # dev-flaky
//!
//! Flaky-test detection for Rust. Part of the `dev-*` verification suite.
//!
//! Runs the test suite N times and tracks which tests pass every run vs
//! which fail sometimes. Emits per-test reliability as
//! `dev-report::Report`.
//!
//! ## Why
//!
//! Flaky tests rot trust in your test suite. After enough false alarms,
//! developers start ignoring failures, and then real failures get missed.
//! `dev-flaky` detects flakiness automatically so you can quarantine or
//! fix the worst offenders.
//!
//! ## Quick example
//!
//! ```no_run
//! use dev_flaky::FlakyRun;
//!
//! let run = FlakyRun::new("my-crate", "0.1.0").iterations(20);
//! let result = run.execute().unwrap();
//! let report = result.into_report();
//! ```
//!
//! ## Status
//!
//! Pre-1.0. API shape defined; subprocess integration lands in `0.9.1`.

#![cfg_attr(docsrs, feature(doc_cfg))]
#![warn(missing_docs)]
#![warn(rust_2018_idioms)]

use dev_report::{CheckResult, Evidence, Report, Severity};

/// Configuration for a flaky-test detection run.
#[derive(Debug, Clone)]
pub struct FlakyRun {
    name: String,
    version: String,
    iterations: u32,
}

impl FlakyRun {
    /// Begin a new flaky-test run. Defaults to 10 iterations.
    pub fn new(name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
            iterations: 10,
        }
    }

    /// Set how many iterations to run.
    pub fn iterations(mut self, n: u32) -> Self {
        self.iterations = n.max(2);
        self
    }

    /// Configured iteration count.
    pub fn iteration_count(&self) -> u32 {
        self.iterations
    }

    /// Execute the run.
    ///
    /// In `0.9.0` this is a stub; subprocess integration lands in `0.9.1`.
    pub fn execute(&self) -> Result<FlakyResult, FlakyError> {
        Ok(FlakyResult {
            name: self.name.clone(),
            version: self.version.clone(),
            iterations: self.iterations,
            tests: Vec::new(),
        })
    }
}

/// Per-test reliability record.
#[derive(Debug, Clone)]
pub struct TestReliability {
    /// Full test path (e.g. `crate::module::test_name`).
    pub name: String,
    /// Number of times this test passed.
    pub passes: u32,
    /// Number of times this test failed.
    pub failures: u32,
}

impl TestReliability {
    /// Fraction of runs that passed, in the range `[0.0, 1.0]`.
    pub fn reliability(&self) -> f64 {
        let total = self.passes + self.failures;
        if total == 0 {
            return 0.0;
        }
        self.passes as f64 / total as f64
    }

    /// `true` if this test is fully reliable (no failures).
    pub fn is_stable(&self) -> bool {
        self.failures == 0
    }

    /// `true` if this test is fully broken (no passes).
    pub fn is_broken(&self) -> bool {
        self.passes == 0
    }

    /// `true` if this test is flaky (mixed pass/fail history).
    pub fn is_flaky(&self) -> bool {
        self.passes > 0 && self.failures > 0
    }
}

/// Result of a flaky-test run.
#[derive(Debug, Clone)]
pub struct FlakyResult {
    /// Crate name.
    pub name: String,
    /// Crate version.
    pub version: String,
    /// Iterations completed.
    pub iterations: u32,
    /// Per-test reliability records.
    pub tests: Vec<TestReliability>,
}

impl FlakyResult {
    /// Count of tests that are flaky (mixed pass/fail).
    pub fn flaky_count(&self) -> usize {
        self.tests.iter().filter(|t| t.is_flaky()).count()
    }

    /// Convert this result into a `dev-report::Report`.
    ///
    /// Stable tests pass. Flaky tests warn (reliability is reported
    /// as evidence). Broken tests (zero passes) fail.
    pub fn into_report(self) -> Report {
        let mut report = Report::new(&self.name, &self.version).with_producer("dev-flaky");
        if self.tests.is_empty() {
            report.push(CheckResult::pass("flaky::scan"));
        } else {
            for t in &self.tests {
                let name = format!("flaky::{}", t.name);
                let reliability_pct = t.reliability() * 100.0;
                let detail = format!(
                    "{}/{} passed ({:.1}%)",
                    t.passes,
                    t.passes + t.failures,
                    reliability_pct
                );
                let check = if t.is_broken() {
                    CheckResult::fail(name, Severity::Error).with_detail(detail)
                } else if t.is_flaky() {
                    CheckResult::warn(name, Severity::Warning).with_detail(detail)
                } else {
                    CheckResult::pass(name).with_detail(detail)
                };
                report.push(
                    check.with_evidence(Evidence::numeric("reliability_pct", reliability_pct)),
                );
            }
        }
        report.finish();
        report
    }
}

/// Errors that can arise during a flaky-test run.
#[derive(Debug)]
pub enum FlakyError {
    /// `cargo test` subprocess failed.
    SubprocessFailed(String),
    /// Output parsing failure.
    ParseError(String),
}

impl std::fmt::Display for FlakyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SubprocessFailed(s) => write!(f, "subprocess failed: {s}"),
            Self::ParseError(s) => write!(f, "parse error: {s}"),
        }
    }
}

impl std::error::Error for FlakyError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iterations_min_two() {
        let r = FlakyRun::new("x", "0.1.0").iterations(1);
        assert_eq!(r.iteration_count(), 2);
    }

    #[test]
    fn stable_test_classified_correctly() {
        let t = TestReliability {
            name: "a".into(),
            passes: 10,
            failures: 0,
        };
        assert!(t.is_stable());
        assert!(!t.is_flaky());
        assert!(!t.is_broken());
        assert_eq!(t.reliability(), 1.0);
    }

    #[test]
    fn broken_test_classified_correctly() {
        let t = TestReliability {
            name: "b".into(),
            passes: 0,
            failures: 10,
        };
        assert!(t.is_broken());
        assert!(!t.is_flaky());
        assert_eq!(t.reliability(), 0.0);
    }

    #[test]
    fn flaky_test_classified_correctly() {
        let t = TestReliability {
            name: "c".into(),
            passes: 7,
            failures: 3,
        };
        assert!(t.is_flaky());
        assert!(!t.is_stable());
        assert!(!t.is_broken());
        assert!((t.reliability() - 0.7).abs() < 0.0001);
    }

    #[test]
    fn empty_result_passes() {
        let r = FlakyResult {
            name: "x".into(),
            version: "0.1.0".into(),
            iterations: 10,
            tests: Vec::new(),
        };
        let report = r.into_report();
        assert!(report.passed());
    }

    #[test]
    fn broken_test_produces_failing_report() {
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
    fn flaky_count_correct() {
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
                    name: "flaky_a".into(),
                    passes: 7,
                    failures: 3,
                },
                TestReliability {
                    name: "flaky_b".into(),
                    passes: 5,
                    failures: 5,
                },
            ],
        };
        assert_eq!(r.flaky_count(), 2);
    }
}
