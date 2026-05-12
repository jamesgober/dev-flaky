<h1 align="center">
    <strong>dev-flaky</strong>
    <br>
    <sup><sub>FLAKY-TEST DETECTION FOR RUST</sub></sup>
</h1>

<p align="center">
    <a href="https://crates.io/crates/dev-flaky"><img alt="crates.io" src="https://img.shields.io/crates/v/dev-flaky.svg"></a>
    <a href="https://crates.io/crates/dev-flaky"><img alt="downloads" src="https://img.shields.io/crates/d/dev-flaky.svg"></a>
    <a href="https://docs.rs/dev-flaky"><img alt="docs.rs" src="https://docs.rs/dev-flaky/badge.svg"></a>
    <a href="https://github.com/jamesgober/dev-flaky/actions/workflows/ci.yml"><img alt="CI" src="https://github.com/jamesgober/dev-flaky/actions/workflows/ci.yml/badge.svg"></a>
    <img alt="MSRV" src="https://img.shields.io/badge/msrv-1.85%2B-blue.svg?style=flat-square" title="Rust Version">
</p>

<p align="center">
    Repeated-run reliability tracking with per-test confidence scoring.<br>
    Part of the <code>dev-*</code> verification suite.
</p>

---

## What it does

`dev-flaky` runs your test suite many times and tracks each test's
pass / fail history. Stable tests pass every iteration. Flaky tests
fail sometimes for no apparent reason. Broken tests fail every time.

After the run, every test gets a reliability score in `[0.0, 1.0]`
and a classification (`Stable` / `Flaky` / `Broken`), emitted as a
[`dev-report::Report`](https://docs.rs/dev-report).

## Why flaky tests matter

Flaky tests are corrosive. After enough false alarms, developers
start ignoring CI failures, and real failures get missed. Detecting
flakiness *automatically* lets you quarantine the worst offenders
before they erode trust in the suite.

## Quick start

```toml
[dependencies]
dev-flaky = "0.9"
```

```rust,no_run
use dev_flaky::FlakyRun;

let run = FlakyRun::new("my-crate", "0.1.0").iterations(20);
let result = run.execute()?;
println!("flaky: {}, broken: {}", result.flaky_count(), result.broken_count());
let report = result.into_report();
println!("{}", report.to_json()?);
# Ok::<(), Box<dyn std::error::Error>>(())
```

## Builder surface

| Method                          | What it does                                                |
|---------------------------------|-------------------------------------------------------------|
| `iterations(n)`                 | How many times to run `cargo test`. Clamped to ≥ 2.         |
| `in_dir(path)`                  | Working dir for `cargo test`. Default: CWD.                |
| `workspace()`                   | Pass `--workspace`.                                         |
| `features(list)`                | Pass `--features <list>`.                                   |
| `test_filter(substring)`        | Pass the libtest positional filter (`cargo test <filter>`). |
| `allow(name)` / `allow_all(iter)` | Suppress known-flaky tests by full test path.             |
| `reliability_threshold(pct)`    | Demote `Stable` to `Flaky` below this reliability percentage. |

## Classification

| Pass count | Fail count | Classification | Verdict | Severity |
|------------|------------|----------------|---------|----------|
| `> 0`      | `0`        | Stable         | Pass    | (none)   |
| `> 0`      | `> 0`      | Flaky          | Warn    | Warning  |
| `0`        | `> 0`      | Broken         | Fail    | Error    |

Each finding emits a `CheckResult` named `flaky::<test>` tagged
`flaky` + the classification label (`stable` / `flaky` / `broken`),
with numeric evidence for `reliability_pct`, `passes`, and `failures`.

## Allow-list

```rust
use dev_flaky::FlakyRun;

let run = FlakyRun::new("my-crate", "0.1.0")
    .iterations(20)
    .allow("known_flaky::flaky_under_load")
    .allow_all(["integration::slow_test", "net::flaky_endpoint"]);
```

Matches the full test path emitted by libtest.

## Reliability threshold

```rust
use dev_flaky::FlakyRun;

let run = FlakyRun::new("my-crate", "0.1.0")
    .iterations(100)
    .reliability_threshold(99.0);
```

With threshold 99.0, a test that passes 99/100 iterations is still
flagged as `Flaky` even though it had zero failures within that run's
classification rule. Useful when you want to surface tests that are
*nearly always* stable but occasionally produce intermittent partial
failures (when extended with sub-iteration counters in a future
release).

## `Producer` integration

`FlakyProducer` plugs the run into a multi-producer pipeline driven
by [`dev-tools`](https://github.com/jamesgober/dev-tools):

```rust,no_run
use dev_flaky::{FlakyProducer, FlakyRun};
use dev_report::Producer;

let producer = FlakyProducer::new(FlakyRun::new("my-crate", "0.1.0").iterations(20));
let report = producer.produce();
println!("{}", report.to_json().unwrap());
```

Subprocess failures map to a single failing `CheckResult` named
`flaky::scan` with `Severity::Critical`.

## Target-dir-lock note

Running `FlakyRun::execute()` from inside another `cargo` invocation
that already holds the workspace target-dir lock will deadlock. This
is a property of cargo, not of `dev-flaky`. When running examples or
the `#[ignore]`d integration test that exercises the real subprocess
pipeline:

```bash
CARGO_TARGET_DIR=/tmp/flaky-target cargo run --example basic
CARGO_TARGET_DIR=/tmp/flaky-target cargo test -- --ignored
```

## Wire format

`FlakyResult`, `TestReliability`, `Classification` are all
`serde`-derived. JSON uses `snake_case` field names and `lowercase`
enum variants:

```json
{
  "name": "my-crate",
  "version": "0.1.0",
  "iterations": 20,
  "tests": [
    { "name": "integration::flaky_one", "passes": 17, "failures": 3 }
  ]
}
```

## Examples

| File                              | What it shows                                                |
|-----------------------------------|---------------------------------------------------------------|
| `examples/basic.rs`               | Default 5-iteration run; graceful tool-missing handling.     |
| `examples/iterations_high.rs`     | 50 iterations + test filter for low-rate flakiness.          |
| `examples/threshold.rs`           | `reliability_threshold` + allow-list.                        |
| `examples/producer.rs`            | `FlakyProducer` (gated by `DEV_FLAKY_EXAMPLE_RUN`).          |

## The `dev-*` suite

See [`dev-tools`](https://github.com/jamesgober/dev-tools) for the
umbrella crate covering the full suite.

## Status

`v0.9.x` is the pre-1.0 stabilization line. Feature-complete for
repeated-run flakiness detection, classification, threshold, allow-
list, and producer integration. `1.0` will pin the public API and
the classification policy.

## Minimum supported Rust version

`1.85` — pinned in `Cargo.toml` via `rust-version` and verified by
the MSRV job in CI.

## License

Apache-2.0. See [LICENSE](LICENSE).
