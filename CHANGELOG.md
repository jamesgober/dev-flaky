# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.9.0] - 2026-05-12

Foundation release. Replaces the `0.1.0` name-claim with full
`cargo test` repeated-run orchestration and per-test reliability
scoring.

### Added

- Real `cargo test --no-fail-fast` repeated-run integration in
  `FlakyRun::execute`. Spawns the subprocess `N` times, parses
  libtest's `test <name> ... ok|FAILED|ignored` output across every
  iteration, and accumulates per-test pass / fail counters.
- libtest output parser in `src/runner.rs` recognizes `ok`,
  `FAILED`, and `ignored` outcomes. Skips libtest's `test result: ok.
  ...` summary lines. Tolerates per-iteration subprocess failures —
  one transient compile failure does not abort the whole run.
- `FlakyRun` builder gains the full surface: `iterations`,
  `in_dir(path)`, `workspace`, `features(list)`, `test_filter(name)`,
  `allow(name)`, `allow_all(iter)`, `reliability_threshold(pct)`,
  plus `subject` / `subject_version` accessors.
- New `Classification` enum (`Stable`, `Flaky`, `Broken`) with
  `severity()` and `label()` methods. The `into_report` flow uses
  the enum rather than open-coding the policy.
- `TestReliability::classification(threshold)` runs the REPS § 4
  policy: any failures → `Flaky` / `Broken`; otherwise `Stable`
  unless the configured threshold demotes it.
- `TestReliability::reliability_pct()` returns the same number as
  `reliability() * 100.0` for convenience.
- `FlakyResult` methods: `stable_count`, `flaky_count`, `broken_count`,
  `total_count`. The `reliability_threshold_pct` field is carried on
  the result so `into_report` can re-derive classifications at
  serialization time.
- `FlakyResult::into_report` now emits one `CheckResult` per test
  named `flaky::<test>`, tagged `flaky` plus the classification label
  (`stable` / `flaky` / `broken`). Each carries numeric evidence for
  `reliability_pct`, `passes`, and `failures`. `Stable` →
  `CheckResult::pass`. `Flaky` → `CheckResult::warn(Severity::Warning)`.
  `Broken` → `CheckResult::fail(Severity::Error)`.
- New `producer` module exposing `FlakyProducer`: a
  `dev_report::Producer` adapter. Subprocess failures map to a
  single `CheckResult::fail("flaky::scan", Severity::Critical)`
  tagged `flaky` + `subprocess`.
- New `FlakyError::ToolNotInstalled` variant (in addition to the
  existing `SubprocessFailed` and `ParseError`).
- 18 unit tests across `lib.rs`, `runner.rs`, `producer.rs`.
  Coverage includes: iteration clamping, classification (Stable /
  Flaky / Broken), threshold-driven Stable→Flaky demotion (and the
  fact that threshold does *not* apply to Broken), reliability
  percentage math, count helpers, `into_report` shape for each
  classification, JSON round-trip on `FlakyResult`, the builder
  chain, libtest output parsing (ok / FAILED / ignored / summary
  line skipping / unknown outcomes / empty input).
- 9 integration tests in `tests/smoke.rs`. One `#[ignore]`d
  real-subprocess test documents the `CARGO_TARGET_DIR` workaround
  needed when running from inside another `cargo test` invocation.
- Examples: `basic.rs` (graceful tool-missing handling),
  `iterations_high.rs` (50 iterations + filter), `threshold.rs`
  (`reliability_threshold` + allow-list), `producer.rs` (gated by
  `DEV_FLAKY_EXAMPLE_RUN`).

### Changed

- README rewritten: removes the "subprocess integration lands in
  0.9.1" placeholder, documents the builder surface, the
  `Classification` enum, the threshold workflow, the producer
  integration, and the cargo target-dir deadlock workaround. MSRV
  pinned at 1.85.
- REPS.md tightened: the "SHOULD provide" items (cargo test
  orchestration, reliability threshold, allow-list) become MUST-have
  for 0.9.x.
- CI workflow: clones `../dev-report` in every job that needs the
  path dep. `actions/checkout@v5` everywhere.

### Dependencies

- Added: `serde` 1.0 (derive feature), `serde_json` 1.0. Required
  for serializing `FlakyResult` / `TestReliability` / `Classification`.
- Added: `tempfile` 3 as a `dev-dependency`.

### Note

`0.1.0` was a name-claim publish with a stub `execute()` returning
an empty result. The public API additions are additive: existing
methods (`new`, `iterations`, `execute`, `into_report`,
`TestReliability` accessors) keep their signatures.

The `FlakyResult` struct gained a new public field
`reliability_threshold_pct: Option<f64>`. Callers that constructed
`FlakyResult` literals in 0.1.0 must add the field (or use
`..Default::default()` once we add `Default`).

The producer's recursion guard is the cargo target-dir lock: running
`FlakyRun::execute()` from inside `cargo test` deadlocks unless
`CARGO_TARGET_DIR` points outside the workspace. The producer test
that triggers this is `#[ignore]`d; users who want to verify
end-to-end can run `CARGO_TARGET_DIR=/tmp/x cargo test -- --ignored`.

[Unreleased]: https://github.com/jamesgober/dev-flaky/compare/v0.9.0...HEAD
[0.9.0]: https://github.com/jamesgober/dev-flaky/releases/tag/v0.9.0
[0.1.0]: https://github.com/jamesgober/dev-flaky/releases/tag/v0.1.0
