# dev-flaky — Project Specification (REPS)

> Rust Engineering Project Specification.
> Normative language follows RFC 2119.

## 1. Purpose

`dev-flaky` MUST run a project's test suite repeatedly and emit per-
test reliability as `dev-report::Report`. Output MUST be machine-
readable so AI agents and CI gates can quarantine flaky tests
automatically.

## 2. Scope

This crate MUST provide:

- A `FlakyRun` builder with iteration configuration.
- A `TestReliability` struct with `reliability()`, `is_stable()`,
  `is_flaky()`, `is_broken()` classifiers.
- A `FlakyResult` with `into_report` integration.

This crate SHOULD provide (later versions):

- `cargo test` orchestration with output parsing (`0.9.1`).
- Test isolation (one test per invocation) for stricter isolation.
- Reliability threshold configuration (e.g. flag tests below 95%).
- Historical trend analysis (compare reliability across runs).

This crate MUST NOT:

- Modify test code.
- Quarantine tests automatically. We report; the user decides.
- Run tests through a custom harness. We use `cargo test`.

## 3. Iteration count

Minimum iterations is `2`. Below 2, the distinction between stable
and flaky is meaningless. The `iterations(n)` setter MUST clamp to a
minimum of 2.

## 4. Classification policy

| Pass count | Fail count | Classification |
|------------|------------|----------------|
| > 0        | 0          | Stable         |
| > 0        | > 0        | Flaky          |
| 0          | > 0        | Broken         |
| 0          | 0          | (not possible) |

Verdict mapping in the emitted `CheckResult`:

| Classification | Verdict | Severity (when applicable) |
|----------------|---------|-----------------------------|
| Stable         | Pass    | (none)                      |
| Flaky          | Warn    | Warning                     |
| Broken         | Fail    | Error                       |

## 5. Determinism

Flakiness is inherently non-deterministic. We do not claim that two
runs of `dev-flaky` produce the same `FlakyResult`. However:

- Given a fixed `FlakyResult`, `into_report()` MUST produce a
  deterministic `Report`.
- Per-test classification rules MUST be stable.

## 6. Stability

Through `0.9.x` the public API MAY shift. The `1.0` release pins the
API and the classification policy.
