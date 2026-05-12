//! [`Producer`] integration: wrap a [`FlakyRun`] and emit a [`Report`]
//! every time the producer runs.
//!
//! [`Producer`]: dev_report::Producer
//! [`Report`]: dev_report::Report

use dev_report::{CheckResult, Producer, Report, Severity};

use crate::FlakyRun;

/// `Producer` adapter that drives a [`FlakyRun`] and converts the
/// result into a `Report`.
///
/// Subprocess failures (missing `cargo`, compile error, IO error
/// spawning the subprocess) map to a single failing `CheckResult`
/// named `flaky::scan` with `Severity::Critical`. No panics.
///
/// # Example
///
/// ```no_run
/// use dev_flaky::{FlakyProducer, FlakyRun};
/// use dev_report::Producer;
///
/// let producer = FlakyProducer::new(
///     FlakyRun::new("my-crate", "0.1.0").iterations(20),
/// );
/// let report = producer.produce();
/// println!("{}", report.to_json().unwrap());
/// ```
pub struct FlakyProducer {
    run: FlakyRun,
}

impl FlakyProducer {
    /// Build a producer that drives the given [`FlakyRun`].
    pub fn new(run: FlakyRun) -> Self {
        Self { run }
    }
}

impl Producer for FlakyProducer {
    fn produce(&self) -> Report {
        match self.run.execute() {
            Ok(result) => result.into_report(),
            Err(e) => {
                let mut report = Report::new(self.run.subject(), self.run.subject_version())
                    .with_producer("dev-flaky");
                let check = CheckResult::fail("flaky::scan", Severity::Critical)
                    .with_detail(e.to_string())
                    .with_tag("flaky")
                    .with_tag("subprocess");
                report.push(check);
                report.finish();
                report
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "spawns inner `cargo test` which deadlocks on the workspace target-dir lock when run from `cargo test`; run with CARGO_TARGET_DIR outside the workspace via `cargo test -- --ignored`"]
    fn produce_returns_report_when_subprocess_fails() {
        // The producer runs `cargo test` from CWD. The contract is
        // "return a Report; never panic." Verified manually via the
        // ignored execute_against_real_cargo smoke test.
        let producer = FlakyProducer::new(FlakyRun::new("self", "0.0.0").iterations(2));
        let report = producer.produce();
        assert_eq!(report.subject, "self");
        assert!(!report.checks.is_empty());
    }
}
