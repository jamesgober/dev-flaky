# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.9.0] - 2026-05-11

### Added

- Initial crate skeleton.
- `FlakyRun` builder with `new`, `iterations`, `iteration_count`,
  `execute`. Minimum 2 iterations enforced.
- `TestReliability` struct with `reliability`, `is_stable`, `is_flaky`,
  `is_broken` classifiers.
- `FlakyResult` with `flaky_count`, `into_report` producing per-test
  CheckResults.
- Reliability percentage attached as `Evidence::Numeric`.
- `FlakyError` for subprocess / parse failures.
- Smoke tests covering classification, empty results, broken-test
  reporting, flaky count.

### Note

This is the name-claim release. The actual `cargo test` repeated-run
orchestration and output parsing lands in `0.9.1`.

[Unreleased]: https://github.com/jamesgober/dev-flaky/compare/v0.9.0...HEAD
[0.9.0]: https://github.com/jamesgober/dev-flaky/releases/tag/v0.9.0
