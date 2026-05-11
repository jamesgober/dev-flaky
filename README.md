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
</p>

<p align="center">
    Repeated-run reliability tracking with per-test confidence scoring.<br>
    Part of the <code>dev-*</code> verification suite.
</p>

---

## What it does

`dev-flaky` runs your test suite many times and tracks each test's
pass/fail history. Stable tests pass every time. Flaky tests fail
sometimes for no apparent reason. Broken tests fail every time.

After the run, every test gets a reliability score in `[0.0, 1.0]`
and a classification (stable / flaky / broken).

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

```rust
use dev_flaky::FlakyRun;

let run = FlakyRun::new("my-crate", "0.1.0").iterations(20);
let result = run.execute()?;

println!("Flaky tests: {}", result.flaky_count());
let report = result.into_report();
# Ok::<(), Box<dyn std::error::Error>>(())
```

## Classification

| Classification | Pass count | Fail count | Verdict      |
|----------------|------------|------------|--------------|
| **Stable**     | > 0        | 0          | `Pass`       |
| **Flaky**      | > 0        | > 0        | `Warn`       |
| **Broken**     | 0          | > 0        | `Fail`       |

The per-test reliability percentage is attached as
`Evidence::Numeric` so you can sort tests by flakiness in downstream
reports.

## The `dev-*` suite

See [`dev-tools`](https://github.com/jamesgober/dev-tools) for the
full suite.

## Status

`v0.9.0` is the foundation release: API shape defined, subprocess
integration lands in `0.9.1`. Production use is discouraged until
`1.0`.

## Minimum supported Rust version

`1.85` — pinned in `Cargo.toml` and verified by CI.

## License

Apache-2.0. See [LICENSE](LICENSE).
