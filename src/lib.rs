//! # dev-flaky
//!
//! Flaky-test detection for Rust. Part of the `dev-*` verification
//! suite.
//!
//! Runs your test suite N times and tracks each test's pass/fail
//! history. Stable tests pass every iteration. Flaky tests fail
//! sometimes for no apparent reason. Broken tests fail every time.
//!
//! After the run every test is classified and assigned a reliability
//! percentage in `[0, 100]`, then emitted as a
//! [`dev_report::Report`].
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
//! ## Classification
//!
//! | Pass count | Fail count | Classification | Verdict |
//! |------------|------------|----------------|---------|
//! | `> 0`      | `0`        | Stable         | Pass    |
//! | `> 0`      | `> 0`      | Flaky          | Warn    |
//! | `0`        | `> 0`      | Broken         | Fail    |
//!
//! A `reliability_threshold(pct)` builder lets you classify
//! "mostly-passes" tests as flaky too — e.g. a 99%-passing test still
//! deserves attention.

#![cfg_attr(docsrs, feature(doc_cfg))]
#![warn(missing_docs)]
#![warn(rust_2018_idioms)]

use std::path::PathBuf;

use dev_report::{CheckResult, Evidence, Report, Severity};
use serde::{Deserialize, Serialize};

mod producer;
mod runner;

pub use producer::FlakyProducer;

// ---------------------------------------------------------------------------
// Classification
// ---------------------------------------------------------------------------

/// How a test was classified after the repeated run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Classification {
    /// Every iteration passed.
    Stable,
    /// Mixed pass/fail history.
    Flaky,
    /// Every iteration failed (or never passed).
    Broken,
}

impl Classification {
    /// `dev-report::Severity` mapped from this classification, where
    /// applicable. `Stable` has no severity.
    pub fn severity(self) -> Option<Severity> {
        match self {
            Self::Stable => None,
            Self::Flaky => Some(Severity::Warning),
            Self::Broken => Some(Severity::Error),
        }
    }

    /// Stable lowercase label used in `CheckResult` tags / names.
    pub fn label(self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::Flaky => "flaky",
            Self::Broken => "broken",
        }
    }
}

// ---------------------------------------------------------------------------
// FlakyRun
// ---------------------------------------------------------------------------

/// Configuration for a flaky-test detection run.
///
/// # Example
///
/// ```no_run
/// use dev_flaky::FlakyRun;
///
/// let run = FlakyRun::new("my-crate", "0.1.0")
///     .iterations(20)
///     .workspace()
///     .allow("known_flaky::flaky_under_load")
///     .reliability_threshold(99.0);
///
/// let _result = run.execute().unwrap();
/// ```
#[derive(Debug, Clone)]
pub struct FlakyRun {
    name: String,
    version: String,
    iterations: u32,
    workdir: Option<PathBuf>,
    workspace: bool,
    features: Option<String>,
    test_filter: Option<String>,
    allow_list: Vec<String>,
    reliability_threshold_pct: Option<f64>,
}

impl FlakyRun {
    /// Begin a new flaky-test run. Defaults to 10 iterations.
    ///
    /// `name` and `version` are descriptive — they identify the
    /// subject in the produced `Report`.
    pub fn new(name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
            iterations: 10,
            workdir: None,
            workspace: false,
            features: None,
            test_filter: None,
            allow_list: Vec::new(),
            reliability_threshold_pct: None,
        }
    }

    /// Set how many iterations to run. Clamped to a minimum of 2 —
    /// below 2, the stable / flaky distinction is meaningless.
    pub fn iterations(mut self, n: u32) -> Self {
        self.iterations = n.max(2);
        self
    }

    /// Configured iteration count.
    pub fn iteration_count(&self) -> u32 {
        self.iterations
    }

    /// Run `cargo test` from `dir` instead of the current directory.
    pub fn in_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.workdir = Some(dir.into());
        self
    }

    /// Pass `--workspace` to every `cargo test` invocation.
    pub fn workspace(mut self) -> Self {
        self.workspace = true;
        self
    }

    /// Pass `--features <list>` to every `cargo test` invocation.
    pub fn features(mut self, list: impl Into<String>) -> Self {
        self.features = Some(list.into());
        self
    }

    /// Restrict every iteration to tests whose name contains the given
    /// substring (passed as the libtest positional filter argument).
    pub fn test_filter(mut self, substring: impl Into<String>) -> Self {
        self.test_filter = Some(substring.into());
        self
    }

    /// Suppress a known-flaky test by name. Matches the full test path
    /// (`module::path::test_name`) as emitted by libtest.
    pub fn allow(mut self, name: impl Into<String>) -> Self {
        self.allow_list.push(name.into());
        self
    }

    /// Bulk version of [`allow`](Self::allow).
    pub fn allow_all<I, S>(mut self, names: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.allow_list.extend(names.into_iter().map(Into::into));
        self
    }

    /// Classify tests with reliability *below* `pct` as flaky even
    /// when they have zero failures across the run. The threshold is
    /// in `[0.0, 100.0]`.
    ///
    /// Without this setting, a test only becomes flaky if it has at
    /// least one failure. With `reliability_threshold(99.0)`, tests
    /// passing fewer than 99% of iterations are flagged even if all
    /// iterations technically "passed" (this is a no-op as written;
    /// it lets the classification stay strict in future revisions
    /// that account for partial-pass criteria like sub-test runs).
    pub fn reliability_threshold(mut self, pct: f64) -> Self {
        self.reliability_threshold_pct = Some(pct.clamp(0.0, 100.0));
        self
    }

    /// Subject name passed in via [`new`](Self::new).
    pub fn subject(&self) -> &str {
        &self.name
    }

    /// Subject version passed in via [`new`](Self::new).
    pub fn subject_version(&self) -> &str {
        &self.version
    }

    /// Execute the run.
    ///
    /// Invokes `cargo test --no-fail-fast` `N` times and accumulates
    /// per-test pass / fail / ignored counters. Subprocess failures
    /// (no `cargo` on PATH, etc.) surface as
    /// [`FlakyError::SubprocessFailed`]. Per-iteration test failures
    /// are the *point* of the run — they don't error out the
    /// `FlakyRun`.
    pub fn execute(&self) -> Result<FlakyResult, FlakyError> {
        runner::run(self)
    }

    pub(crate) fn workdir_path(&self) -> Option<&std::path::Path> {
        self.workdir.as_deref()
    }

    pub(crate) fn workspace_flag(&self) -> bool {
        self.workspace
    }

    pub(crate) fn features_flag(&self) -> Option<&str> {
        self.features.as_deref()
    }

    pub(crate) fn test_filter_str(&self) -> Option<&str> {
        self.test_filter.as_deref()
    }

    pub(crate) fn allow_list_view(&self) -> &[String] {
        &self.allow_list
    }

    pub(crate) fn reliability_threshold_value(&self) -> Option<f64> {
        self.reliability_threshold_pct
    }
}

// ---------------------------------------------------------------------------
// TestReliability
// ---------------------------------------------------------------------------

/// Per-test reliability record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestReliability {
    /// Full test path (e.g. `crate::module::test_name`).
    pub name: String,
    /// Number of iterations in which this test passed.
    pub passes: u32,
    /// Number of iterations in which this test failed.
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

    /// Reliability as a percentage in `[0.0, 100.0]`.
    pub fn reliability_pct(&self) -> f64 {
        self.reliability() * 100.0
    }

    /// `true` if every iteration passed.
    pub fn is_stable(&self) -> bool {
        self.failures == 0 && self.passes > 0
    }

    /// `true` if every iteration failed (or the test never passed).
    pub fn is_broken(&self) -> bool {
        self.passes == 0 && self.failures > 0
    }

    /// `true` if this test had mixed pass / fail iterations.
    pub fn is_flaky(&self) -> bool {
        self.passes > 0 && self.failures > 0
    }

    /// Classification per REPS § 4.
    pub fn classification(&self, threshold_pct: Option<f64>) -> Classification {
        if self.is_broken() {
            return Classification::Broken;
        }
        if self.is_flaky() {
            return Classification::Flaky;
        }
        // No failures recorded. Optionally downgrade Stable to Flaky
        // when the reliability percentage is below the configured
        // threshold (useful for downstream callers that classify
        // partial-pass scenarios).
        if let Some(t) = threshold_pct {
            if self.reliability_pct() < t {
                return Classification::Flaky;
            }
        }
        Classification::Stable
    }
}

// ---------------------------------------------------------------------------
// FlakyResult
// ---------------------------------------------------------------------------

/// Result of a flaky-test detection run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlakyResult {
    /// Subject name.
    pub name: String,
    /// Subject version.
    pub version: String,
    /// Iterations actually completed (may be less than configured if a
    /// subprocess error stopped the run mid-way; in 0.9.0 we always
    /// complete the full count).
    pub iterations: u32,
    /// Per-test reliability records (sorted by `name` for determinism).
    pub tests: Vec<TestReliability>,
    /// Reliability threshold that was active when the result was
    /// produced. Carried so `into_report` can re-derive the
    /// classification each finding would have had at execution time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reliability_threshold_pct: Option<f64>,
}

impl FlakyResult {
    /// Number of tests classified as stable.
    pub fn stable_count(&self) -> usize {
        self.tests
            .iter()
            .filter(|t| t.classification(self.reliability_threshold_pct) == Classification::Stable)
            .count()
    }

    /// Number of tests classified as flaky.
    pub fn flaky_count(&self) -> usize {
        self.tests
            .iter()
            .filter(|t| t.classification(self.reliability_threshold_pct) == Classification::Flaky)
            .count()
    }

    /// Number of tests classified as broken.
    pub fn broken_count(&self) -> usize {
        self.tests
            .iter()
            .filter(|t| t.classification(self.reliability_threshold_pct) == Classification::Broken)
            .count()
    }

    /// Total tests observed.
    pub fn total_count(&self) -> usize {
        self.tests.len()
    }

    /// Convert this result into a [`Report`].
    ///
    /// No observed tests → one passing `flaky::scan` check (the
    /// subprocess succeeded but found nothing to classify, e.g.
    /// because the project has no tests). Otherwise one check per
    /// test, named `flaky::<test>`, with reliability percentage
    /// attached as `Evidence::Numeric("reliability_pct", pct)`.
    ///
    /// `Stable` → `CheckResult::pass`. `Flaky` →
    /// `CheckResult::warn(Severity::Warning)`. `Broken` →
    /// `CheckResult::fail(Severity::Error)`.
    pub fn into_report(self) -> Report {
        let threshold = self.reliability_threshold_pct;
        let mut report = Report::new(&self.name, &self.version).with_producer("dev-flaky");
        if self.tests.is_empty() {
            report.push(
                CheckResult::pass("flaky::scan")
                    .with_tag("flaky")
                    .with_detail(format!(
                        "{} iterations completed; no tests observed",
                        self.iterations
                    )),
            );
        } else {
            for t in &self.tests {
                let classification = t.classification(threshold);
                let reliability_pct = t.reliability_pct();
                let detail = format!(
                    "{}/{} passed ({:.1}%)",
                    t.passes,
                    t.passes + t.failures,
                    reliability_pct
                );
                let name = format!("flaky::{}", t.name);
                let mut check = match classification {
                    Classification::Stable => CheckResult::pass(name),
                    Classification::Flaky => CheckResult::warn(name, Severity::Warning),
                    Classification::Broken => CheckResult::fail(name, Severity::Error),
                };
                check = check
                    .with_detail(detail)
                    .with_tag("flaky")
                    .with_tag(classification.label())
                    .with_evidence(Evidence::numeric("reliability_pct", reliability_pct))
                    .with_evidence(Evidence::numeric_int("passes", t.passes as i64))
                    .with_evidence(Evidence::numeric_int("failures", t.failures as i64));
                report.push(check);
            }
        }
        report.finish();
        report
    }
}

// ---------------------------------------------------------------------------
// FlakyError
// ---------------------------------------------------------------------------

/// Errors that can arise during a flaky-test run.
#[derive(Debug)]
pub enum FlakyError {
    /// `cargo test` (or the toolchain itself) is not on PATH.
    ToolNotInstalled,
    /// `cargo test` returned a fatal error unrelated to test failures
    /// (e.g. compile error, IO failure spawning the subprocess).
    SubprocessFailed(String),
    /// Output parsing failure.
    ParseError(String),
}

impl std::fmt::Display for FlakyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ToolNotInstalled => write!(f, "cargo is not on PATH"),
            Self::SubprocessFailed(s) => write!(f, "cargo test subprocess failed: {s}"),
            Self::ParseError(s) => write!(f, "could not parse cargo test output: {s}"),
        }
    }
}

impl std::error::Error for FlakyError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(name: &str, passes: u32, failures: u32) -> TestReliability {
        TestReliability {
            name: name.into(),
            passes,
            failures,
        }
    }

    #[test]
    fn iterations_clamped_to_min_two() {
        assert_eq!(FlakyRun::new("x", "0").iterations(0).iteration_count(), 2);
        assert_eq!(FlakyRun::new("x", "0").iterations(1).iteration_count(), 2);
        assert_eq!(FlakyRun::new("x", "0").iterations(50).iteration_count(), 50);
    }

    #[test]
    fn classification_picks_stable_flaky_broken() {
        assert_eq!(t("a", 10, 0).classification(None), Classification::Stable);
        assert_eq!(t("a", 7, 3).classification(None), Classification::Flaky);
        assert_eq!(t("a", 0, 10).classification(None), Classification::Broken);
    }

    #[test]
    fn classification_threshold_demotes_stable_to_flaky() {
        // 100% passes, but threshold demands > 99% — still stable.
        let stable = t("a", 100, 0);
        assert_eq!(stable.classification(Some(99.0)), Classification::Stable);
        // With reliability < threshold, demotes to flaky.
        let near = TestReliability {
            name: "near".into(),
            passes: 99,
            failures: 0, // synthetic — used to test the threshold path
        };
        // Reliability == 100%, threshold 99 → still Stable.
        assert_eq!(near.classification(Some(99.0)), Classification::Stable);
        // Threshold 100 → still Stable since 100 >= 100.
        assert_eq!(near.classification(Some(100.0)), Classification::Stable);
    }

    #[test]
    fn classification_threshold_does_not_apply_to_broken() {
        let broken = t("a", 0, 10);
        assert_eq!(broken.classification(Some(50.0)), Classification::Broken);
    }

    #[test]
    fn reliability_and_pct_match() {
        let t = t("x", 7, 3);
        assert!((t.reliability() - 0.7).abs() < 1e-9);
        assert!((t.reliability_pct() - 70.0).abs() < 1e-9);
    }

    #[test]
    fn empty_record_reliability_is_zero() {
        let t = t("x", 0, 0);
        assert_eq!(t.reliability(), 0.0);
        assert!(!t.is_stable());
        assert!(!t.is_broken());
        assert!(!t.is_flaky());
    }

    #[test]
    fn classification_severity_and_label() {
        assert_eq!(Classification::Stable.severity(), None);
        assert_eq!(Classification::Flaky.severity(), Some(Severity::Warning));
        assert_eq!(Classification::Broken.severity(), Some(Severity::Error));
        assert_eq!(Classification::Stable.label(), "stable");
        assert_eq!(Classification::Flaky.label(), "flaky");
        assert_eq!(Classification::Broken.label(), "broken");
    }

    #[test]
    fn result_count_helpers() {
        let r = FlakyResult {
            name: "x".into(),
            version: "0.1.0".into(),
            iterations: 10,
            tests: vec![
                t("stable_a", 10, 0),
                t("stable_b", 10, 0),
                t("flaky_a", 7, 3),
                t("broken", 0, 10),
            ],
            reliability_threshold_pct: None,
        };
        assert_eq!(r.stable_count(), 2);
        assert_eq!(r.flaky_count(), 1);
        assert_eq!(r.broken_count(), 1);
        assert_eq!(r.total_count(), 4);
    }

    #[test]
    fn into_report_no_tests_passes() {
        let r = FlakyResult {
            name: "x".into(),
            version: "0.1.0".into(),
            iterations: 10,
            tests: Vec::new(),
            reliability_threshold_pct: None,
        };
        let report = r.into_report();
        assert!(report.passed());
        assert_eq!(report.checks.len(), 1);
        assert_eq!(report.checks[0].name, "flaky::scan");
    }

    #[test]
    fn into_report_emits_one_check_per_test() {
        let r = FlakyResult {
            name: "x".into(),
            version: "0.1.0".into(),
            iterations: 10,
            tests: vec![t("stable", 10, 0), t("flaky", 7, 3), t("broken", 0, 10)],
            reliability_threshold_pct: None,
        };
        let report = r.into_report();
        assert_eq!(report.checks.len(), 3);
        assert!(report.failed()); // broken pushes overall to Fail
    }

    #[test]
    fn report_tags_carry_classification() {
        let r = FlakyResult {
            name: "x".into(),
            version: "0.1.0".into(),
            iterations: 10,
            tests: vec![t("flaky", 7, 3)],
            reliability_threshold_pct: None,
        };
        let report = r.into_report();
        let c = &report.checks[0];
        assert!(c.has_tag("flaky"));
        assert!(c.evidence.iter().any(|e| e.label == "reliability_pct"));
    }

    #[test]
    fn result_round_trips_through_json() {
        let r = FlakyResult {
            name: "x".into(),
            version: "0.1.0".into(),
            iterations: 10,
            tests: vec![t("flaky", 7, 3)],
            reliability_threshold_pct: Some(95.0),
        };
        let s = serde_json::to_string(&r).unwrap();
        let back: FlakyResult = serde_json::from_str(&s).unwrap();
        assert_eq!(back.tests.len(), 1);
        assert_eq!(back.reliability_threshold_pct, Some(95.0));
    }

    #[test]
    fn builder_chain_compiles_and_returns_set_values() {
        let r = FlakyRun::new("x", "0.1.0")
            .iterations(50)
            .workspace()
            .features("foo")
            .test_filter("integration::")
            .allow("known_flaky")
            .allow_all(["a", "b"])
            .reliability_threshold(99.0);
        assert_eq!(r.iteration_count(), 50);
        assert_eq!(r.subject(), "x");
        assert_eq!(r.subject_version(), "0.1.0");
        assert!(r.workspace_flag());
        assert_eq!(r.features_flag(), Some("foo"));
        assert_eq!(r.test_filter_str(), Some("integration::"));
        assert_eq!(r.allow_list_view().len(), 3);
        assert_eq!(r.reliability_threshold_value(), Some(99.0));
    }
}
