# dev-flaky — Project Specification (REPS)

> Rust Engineering Project Specification.
> Normative language follows RFC 2119.

## 1. Purpose

`dev-flaky` MUST run a project's test suite repeatedly and emit
per-test reliability as `dev-report::Report`. Output MUST be
machine-readable so AI agents and CI gates can quarantine flaky tests
automatically.

## 2. Scope

This crate MUST provide:

- A `FlakyRun` builder with `iterations`, `in_dir`, `workspace`,
  `features`, `test_filter`, `allow`, `allow_all`,
  `reliability_threshold`, plus `subject` / `subject_version`
  accessors and `execute`.
- A `TestReliability` struct with `reliability()`,
  `reliability_pct()`, `is_stable()`, `is_flaky()`, `is_broken()`,
  and `classification(threshold)`.
- A `Classification` enum (`Stable`, `Flaky`, `Broken`) with
  `severity()` and `label()` methods.
- A `FlakyResult` with `stable_count`, `flaky_count`,
  `broken_count`, `total_count`, and `into_report` integration.
- A `FlakyProducer` adapter implementing `dev_report::Producer`.
- `cargo test` repeated-run orchestration with libtest output
  parsing.

This crate MAY provide later:

- Test isolation (one test per invocation) for stricter classification
  in the presence of test-to-test contamination.
- Historical trend analysis (compare reliability across runs).
- Per-test minimum iteration counts for confidence intervals.

This crate MUST NOT:

- Modify test code.
- Quarantine tests automatically. We report; the user decides.
- Run tests through a custom harness. We use `cargo test`.

## 3. Iteration count

Minimum iterations is `2`. Below `2`, the distinction between stable
and flaky is meaningless. The `iterations(n)` setter MUST clamp to a
minimum of `2`.

## 4. Classification policy

| Pass count | Fail count | Classification |
|------------|------------|----------------|
| `> 0`      | `0`        | Stable         |
| `> 0`      | `> 0`      | Flaky          |
| `0`        | `> 0`      | Broken         |
| `0`        | `0`        | (not possible) |

Verdict mapping in the emitted `CheckResult`:

| Classification | Verdict | Severity              |
|----------------|---------|------------------------|
| Stable         | Pass    | (none)                 |
| Flaky          | Warn    | Warning                |
| Broken         | Fail    | Error                  |

`FlakyRun::reliability_threshold(pct)` MAY demote a `Stable`
classification to `Flaky` when the reliability percentage drops below
the threshold. It MUST NOT promote `Broken` to anything else.

## 5. Determinism

Flakiness is inherently non-deterministic. We do not claim two runs
of `dev-flaky` produce the same `FlakyResult`.

However:

- Given a fixed `FlakyResult`, `into_report()` MUST produce a
  deterministic `Report` (modulo the auto-stamped timestamps).
- Per-test classification rules MUST be a pure function of `passes`,
  `failures`, and the optional `threshold_pct`.
- The `tests` vector inside a `FlakyResult` MUST be sorted by
  `name` for diff-friendly output.

## 6. Tool dependency

`cargo` MUST be on PATH. The crate detects its absence and emits
`FlakyError::ToolNotInstalled`. Subprocess failures with no
recoverable output across all iterations MUST surface as
`FlakyError::SubprocessFailed(stderr)`. Per-iteration subprocess
failures (e.g. a transient compile error in iteration 3 out of 20)
MUST NOT abort the run — they MUST be recorded as iteration noise
and the run MUST continue.

`cargo test` returns non-zero when any test fails. The crate MUST
parse stdout regardless of exit code: a test failure IS the success
path for `dev-flaky`.

## 7. Producer contract

`FlakyProducer::produce()` MUST always return a `Report`. It MUST
NOT panic on subprocess failure; instead, it MUST emit a single
`CheckResult::fail("flaky::scan", Severity::Critical)` carrying the
error message in `detail` and the tags `flaky` + `subprocess`.

## 8. Target-dir-lock note

Running `FlakyRun::execute()` from inside another `cargo` invocation
that already holds the workspace target-dir lock will deadlock. The
crate's `FlakyProducer` test that exercises the full pipeline is
`#[ignore]`d for this reason. Documentation MUST surface the
`CARGO_TARGET_DIR=/tmp/...` workaround for contributors who want to
exercise the real subprocess path.

## 9. Stability

Through `0.9.x` the public API MAY shift. The `1.0` release pins the
API and the classification policy.
