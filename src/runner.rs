//! `cargo test` repeated-run driver + libtest output parser.
//!
//! libtest emits one line per test in the form:
//!
//! ```text
//! test path::to::test ... ok
//! test path::to::test ... FAILED
//! test path::to::test ... ignored
//! ```
//!
//! We parse those lines across N iterations and accumulate pass / fail
//! counters per test name. Ignored tests are skipped.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

use crate::{FlakyError, FlakyResult, FlakyRun, TestReliability};

pub(crate) fn run(cfg: &FlakyRun) -> Result<FlakyResult, FlakyError> {
    detect_cargo()?;

    let mut counts: BTreeMap<String, (u32, u32)> = BTreeMap::new();
    let mut iterations_completed: u32 = 0;
    let mut last_subprocess_error: Option<String> = None;

    for _ in 0..cfg.iteration_count() {
        let output = run_cargo_test(cfg)?;
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        let observed = parse_test_outcomes(&stdout);

        // If the run produced no test lines AND exited non-zero with
        // stderr noise, that's a compile / spawn failure rather than a
        // test result. Record it so the surface error stays informative,
        // but keep iterating — a transient compile failure can be flaky
        // on its own.
        if observed.is_empty() && !output.status.success() {
            last_subprocess_error = Some(stderr);
            iterations_completed += 1;
            continue;
        }

        for (name, outcome) in observed {
            let entry = counts.entry(name).or_insert((0, 0));
            match outcome {
                Outcome::Pass => entry.0 += 1,
                Outcome::Fail => entry.1 += 1,
                Outcome::Ignored => {}
            }
        }
        iterations_completed += 1;
    }

    // If we never observed any test lines AND every iteration also
    // produced a subprocess error, surface the error.
    if counts.is_empty() {
        if let Some(err) = last_subprocess_error {
            return Err(FlakyError::SubprocessFailed(err));
        }
    }

    let mut tests: Vec<TestReliability> = counts
        .into_iter()
        .map(|(name, (passes, failures))| TestReliability {
            name,
            passes,
            failures,
        })
        .collect();

    let allow = cfg.allow_list_view();
    if !allow.is_empty() {
        tests.retain(|t| !allow.iter().any(|n| n == &t.name));
    }

    Ok(FlakyResult {
        name: cfg.subject().to_string(),
        version: cfg.subject_version().to_string(),
        iterations: iterations_completed,
        tests,
        reliability_threshold_pct: cfg.reliability_threshold_value(),
    })
}

fn detect_cargo() -> Result<(), FlakyError> {
    match Command::new("cargo").arg("--version").output() {
        Ok(o) if o.status.success() => Ok(()),
        _ => Err(FlakyError::ToolNotInstalled),
    }
}

fn run_cargo_test(cfg: &FlakyRun) -> Result<std::process::Output, FlakyError> {
    let mut cmd = Command::new("cargo");
    cmd.args(["test", "--no-fail-fast"]);
    if cfg.workspace_flag() {
        cmd.arg("--workspace");
    }
    if let Some(features) = cfg.features_flag() {
        cmd.args(["--features", features]);
    }
    // The double-dash separates cargo args from libtest args.
    cmd.arg("--");
    if let Some(filter) = cfg.test_filter_str() {
        cmd.arg(filter);
    }
    if let Some(dir) = cfg.workdir_path() {
        cmd.current_dir(dir as &Path);
    }
    cmd.output()
        .map_err(|e| FlakyError::SubprocessFailed(e.to_string()))
}

// ---------------------------------------------------------------------------
// Libtest output parser
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Outcome {
    Pass,
    Fail,
    Ignored,
}

pub(crate) fn parse_test_outcomes(stdout: &str) -> Vec<(String, Outcome)> {
    let mut out = Vec::new();
    for line in stdout.lines() {
        let rest = match line.strip_prefix("test ") {
            Some(r) => r,
            None => continue,
        };
        // Skip "test result: ok. ..." summary lines.
        if rest.starts_with("result: ") {
            continue;
        }
        let (name, outcome) = match rest.rsplit_once(" ... ") {
            Some(pair) => pair,
            None => continue,
        };
        let trimmed = outcome.split_whitespace().next().unwrap_or("");
        let kind = match trimmed {
            "ok" => Outcome::Pass,
            "FAILED" => Outcome::Fail,
            "ignored" => Outcome::Ignored,
            _ => continue,
        };
        out.push((name.to_string(), kind));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ok_failed_ignored() {
        let stdout = "\
running 4 tests
test foo::bar ... ok
test foo::baz ... FAILED
test foo::qux ... ignored
test foo::quux ... ok (0.01s)

failures:
foo::baz
";
        let outcomes = parse_test_outcomes(stdout);
        assert_eq!(outcomes.len(), 4);
        assert_eq!(outcomes[0], ("foo::bar".into(), Outcome::Pass));
        assert_eq!(outcomes[1], ("foo::baz".into(), Outcome::Fail));
        assert_eq!(outcomes[2], ("foo::qux".into(), Outcome::Ignored));
        assert_eq!(outcomes[3], ("foo::quux".into(), Outcome::Pass));
    }

    #[test]
    fn skips_summary_lines() {
        let stdout = "\
test foo::a ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
";
        let outcomes = parse_test_outcomes(stdout);
        assert_eq!(outcomes.len(), 1);
        assert_eq!(outcomes[0].0, "foo::a");
    }

    #[test]
    fn ignores_unrelated_lines() {
        let stdout = "\
   Compiling foo v0.1.0
running 1 test
test test_a ... ok
hello world
";
        let outcomes = parse_test_outcomes(stdout);
        assert_eq!(outcomes.len(), 1);
        assert_eq!(outcomes[0].0, "test_a");
    }

    #[test]
    fn ignores_unknown_outcomes() {
        let stdout = "test foo ... maybe\ntest bar ... ok\n";
        let outcomes = parse_test_outcomes(stdout);
        assert_eq!(outcomes.len(), 1);
        assert_eq!(outcomes[0].0, "bar");
    }

    #[test]
    fn empty_input_yields_empty_output() {
        assert!(parse_test_outcomes("").is_empty());
    }
}
