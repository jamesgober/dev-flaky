# dev-flaky — API Reference

> Hand-written reference. Mirrors `cargo doc --open` output with
> curated examples and structure.

## Table of contents

- [`FlakyRun`](#flakyrun)
  - [`FlakyRun::new`](#flakyrunnew)
  - [`FlakyRun::iterations`](#flakyruniterations)
  - [`FlakyRun::iteration_count`](#flakyruniteration_count)
  - [Other builder methods](#other-builder-methods)
  - [`FlakyRun::execute`](#flakyrunexecute)
- [`TestReliability`](#testreliability)
  - [Fields](#testreliability-fields)
  - [`TestReliability::reliability`](#testreliabilityreliability)
  - [`TestReliability::is_stable`](#testreliabilityis_stable)
  - [`TestReliability::is_flaky`](#testreliabilityis_flaky)
  - [`TestReliability::is_broken`](#testreliabilityis_broken)
- [`FlakyResult`](#flakyresult)
  - [Fields](#flakyresult-fields)
  - [`FlakyResult::flaky_count`](#flakyresultflaky_count)
  - [`FlakyResult::into_report`](#flakyresultinto_report)
- [`FlakyError`](#flakyerror)

---

## `FlakyRun`

```rust
pub struct FlakyRun { /* private */ }
```

### `FlakyRun::new`

```rust
pub fn new(name: impl Into<String>, version: impl Into<String>) -> Self
```

| Parameter | Type                | Description    |
|-----------|---------------------|----------------|
| `name`    | `impl Into<String>` | Crate name.    |
| `version` | `impl Into<String>` | Crate version. |

Defaults to 10 iterations.

```rust
use dev_flaky::FlakyRun;

let run = FlakyRun::new("my-crate", "0.1.0");
```

### `FlakyRun::iterations`

```rust
pub fn iterations(self, n: u32) -> Self
```

Set how many iterations to run. Clamped to a minimum of `2`; below
2, the distinction between stable and flaky is meaningless.

```rust
use dev_flaky::FlakyRun;

let r = FlakyRun::new("c", "0.1.0").iterations(20);
assert_eq!(r.iteration_count(), 20);

// Clamped: 1 becomes 2.
let r2 = FlakyRun::new("c", "0.1.0").iterations(1);
assert_eq!(r2.iteration_count(), 2);
```

### `FlakyRun::iteration_count`

```rust
pub fn iteration_count(&self) -> u32
```

Return the configured iteration count.

### Other builder methods

| Method                              | Effect                                                         |
|-------------------------------------|----------------------------------------------------------------|
| `in_dir(dir)`                       | Run `cargo test` from `dir`.                                   |
| `workspace()`                       | Pass `--workspace`.                                            |
| `features(list)`                    | Pass `--features <list>`.                                      |
| `test_filter(substring)`            | Pass the libtest name filter.                                  |
| `allow(name)` / `allow_all(names)`  | Drop records for these test paths.                             |
| `reliability_threshold(pct)`        | Classify records below `pct` with no failures as flaky.       |
| `iteration_timeout(limit)`          | Kill an iteration's `cargo test` process tree after `limit`.   |

### `FlakyRun::execute`

```rust
pub fn execute(&self) -> Result<FlakyResult, FlakyError>
```

Build the test binaries once (`cargo test --no-run`), then run
`cargo test --no-fail-fast` N times and aggregate per-test pass/fail
counts. Ignored tests are not recorded. A compile error is returned
once as `FlakyError::SubprocessFailed` from the build step.

Only the list of outcome lines after each `running N tests` header is
parsed; captured test output in the failure details is skipped. A test
binary that crashes or is killed by `iteration_timeout` counts as a
failure of the test libtest showed as running, or of a
`<binary>: test binary did not finish` record when no test was named.
Tests with the same path in different binaries are kept apart as
`name [binary]`.

---

## `TestReliability`

```rust
pub struct TestReliability {
    pub name: String,
    pub passes: u32,
    pub failures: u32,
}
```

Per-test reliability record. The `name` is the full test path as
libtest prints it (e.g. `module::test_name`), without the
` - should panic` suffix. When the same path exists in several test
binaries, the binary is appended: `tests::smoke [app: unittests src/lib.rs]`.

### TestReliability fields

| Field      | Type     | Description                                  |
|------------|----------|----------------------------------------------|
| `name`     | `String` | Full test path.                              |
| `passes`   | `u32`    | Number of runs where this test passed.       |
| `failures` | `u32`    | Number of runs where this test failed.       |

### `TestReliability::reliability`

```rust
pub fn reliability(&self) -> f64
```

Fraction of runs that passed, in `[0.0, 1.0]`. Returns `0.0` when
there were no runs (defensive default).

```rust
use dev_flaky::TestReliability;

let t = TestReliability { name: "a".into(), passes: 7, failures: 3 };
assert!((t.reliability() - 0.7).abs() < 0.0001);
```

### `TestReliability::is_stable`

```rust
pub fn is_stable(&self) -> bool
```

`true` when `failures == 0` and `passes > 0`.

### `TestReliability::is_flaky`

```rust
pub fn is_flaky(&self) -> bool
```

`true` when both `passes > 0` AND `failures > 0`.

### `TestReliability::is_broken`

```rust
pub fn is_broken(&self) -> bool
```

`true` when `passes == 0` and `failures > 0`.

---

## `FlakyResult`

```rust
pub struct FlakyResult {
    pub name: String,
    pub version: String,
    pub iterations: u32,
    pub tests: Vec<TestReliability>,
    pub reliability_threshold_pct: Option<f64>,
}
```

### FlakyResult fields

| Field        | Type                    | Description                              |
|--------------|-------------------------|------------------------------------------|
| `name`       | `String`                | Crate name.                              |
| `version`    | `String`                | Crate version.                           |
| `iterations` | `u32`                   | Iterations that produced test results.   |
| `tests`      | `Vec<TestReliability>`  | Per-test records, sorted by name.        |
| `reliability_threshold_pct` | `Option<f64>` | Threshold used for classification. |

### `FlakyResult::flaky_count`

```rust
pub fn flaky_count(&self) -> usize
```

Count of tests classified as flaky.

### `FlakyResult::into_report`

```rust
pub fn into_report(self) -> Report
```

Convert this result into a `dev-report::Report`. Stable tests pass.
Flaky tests warn (with reliability % attached as `Evidence::Numeric`).
Broken tests fail.

---

## `FlakyError`

```rust
pub enum FlakyError {
    ToolNotInstalled,
    SubprocessFailed(String),
    ParseError(String),
}
```

Typical remediation: ensure `cargo test --no-fail-fast` runs cleanly
in your project before running flaky-test detection.
