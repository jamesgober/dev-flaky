# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.9.2] - 2026-10-09

libtest parsing and crash/hang handling fixes from a review pass, plus
the MSRV rollback to Rust 1.75. The rollback follows `dev-fixtures`
0.9.5 swapping `tempfile` for `mod-tempdir` 1.0, which removed the
`getrandom 0.4.2 -> edition2024` chain that held the dev-* collection
at 1.85.

### Added

- `FlakyRun::iteration_timeout(Duration)`, off by default. A hung test
  used to block the run forever. On expiry the iteration's `cargo test`
  and the test binary it started are killed (`taskkill /T` on Windows,
  the process group on Unix), results so far are kept, and the run
  moves on. A test named by libtest's "running for over 60 seconds"
  notice is charged with the hang.
- `VERSION` constant with the crate version as compiled, so tools that
  bundle this crate can report what is actually linked.

### Fixed

- Output printed by a failing test that looked like `test x ... ok` was
  counted as a real result. Only the outcome lines after each
  `running N tests` header are read now.
- Ignored tests became 0/0 records that showed as passing checks and
  turned `Flaky` under any threshold. They are no longer recorded.
- A test with the same path in two test binaries (two workspace crates,
  two integration-test files) shared one counter, so a broken test plus
  a stable one came out as flaky. They are kept apart as
  `name [binary]`; the allow-list still matches the plain name.
- A test binary that aborted, overflowed its stack or segfaulted was
  ignored, so a test that sometimes crashed looked stable. The failure
  is now charged to the test libtest reported as running, or to a
  `<binary>: test binary did not finish` record.
- A compile error was retried on every iteration. Test binaries are now
  built once with `cargo test --no-run`, and a build error is returned
  straight away as `SubprocessFailed`. Build time also stays out of the
  iteration timeout.
- `#[should_panic]` test names kept libtest's ` - should panic` suffix,
  so allow-list entries did not match.
- `FlakyResult::iterations` counted iterations that produced no
  results; it now counts the ones that did, as documented.
- `reliability()` and the report detail could overflow `u32`; they use
  `u64` now.

### Changed

- Error messages name the tool that failed and its exit status.
- The unused `tempfile` dev-dependency is gone; it kept
  `cargo +1.75 test` from resolving.
- `rust-version` lowered from `1.85` to `1.75`. CI's MSRV job now
  builds on 1.75 against an MSRV-compatible lockfile; it was still
  pinned to 1.85.

### Documentation

- `reliability_threshold` is documented as it actually behaves: in
  records from `execute`, any test below 100% already has a failure and
  is `Flaky` or `Broken`, so the threshold only changes the result for
  records built or edited by the caller.
- README: new "How runs are read" and "Hanging tests" sections, the
  builder table lists `iteration_timeout`, MSRV section says 1.75.
- `docs/API.md`: corrected `is_stable` / `is_broken` rules, added
  `ToolNotInstalled`, `reliability_threshold_pct` and the builder
  methods.

[0.9.2]: https://github.com/jamesgober/dev-flaky/releases/tag/v0.9.2

## [0.9.1] - 2026-05-12

Documentation and SEO pass. No code changes.

### Changed

- README header standardized: Rust logo image, MSRV badge between CI and docs.rs (was at the end, lowercase label), copyright block at bottom.
- Subtitle now reads `FLAKY TEST DETECTION FOR RUST` (was `FLAKY-TEST DETECTION FOR RUST`). Two-word form is more search-aligned.
- Tagline rewritten to lead with the developer-facing flow (run N times, classify as stable / flaky / broken).
- `## The dev-* suite` retitled to `The dev-* collection` and expanded with the full 14-crate map.
- `Cargo.toml` description rewritten: explicit about the N-iteration run, the three-class classifier, and the reliability scale.
- `Cargo.toml` keywords retuned: dropped `verification` and `ai-tools`, added `retry` and `ci` for crates.io search.

### Added

- "Part of the `dev-*` verification collection" block on the README, under the intro, linking the umbrella `dev-tools` crate.

[0.9.1]: https://github.com/jamesgober/dev-flaky/releases/tag/v0.9.1

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
