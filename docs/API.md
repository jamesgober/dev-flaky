# dev-flaky — API Reference

> Hand-written reference. Mirrors `cargo doc --open` output with
> curated examples and structure.

## Table of contents

- [`FlakyRun`](#flakyrun)
  - [`FlakyRun::new`](#flakyrunnew)
  - [`FlakyRun::iterations`](#flakyruniterations)
  - [`FlakyRun::iteration_count`](#flakyruniteration_count)
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

### `FlakyRun::execute`

```rust
pub fn execute(&self) -> Result<FlakyResult, FlakyError>
```

Run the test suite N times and aggregate per-test pass/fail counts.

---

## `TestReliability`

```rust
pub struct TestReliability {
    pub name: String,
    pub passes: u32,
    pub failures: u32,
}
```

Per-test reliability record. The `name` is the full test path
(e.g. `crate::module::test_name`).

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

`true` when `failures == 0`.

### `TestReliability::is_flaky`

```rust
pub fn is_flaky(&self) -> bool
```

`true` when both `passes > 0` AND `failures > 0`.

### `TestReliability::is_broken`

```rust
pub fn is_broken(&self) -> bool
```

`true` when `passes == 0`.

---

## `FlakyResult`

```rust
pub struct FlakyResult {
    pub name: String,
    pub version: String,
    pub iterations: u32,
    pub tests: Vec<TestReliability>,
}
```

### FlakyResult fields

| Field        | Type                    | Description                              |
|--------------|-------------------------|------------------------------------------|
| `name`       | `String`                | Crate name.                              |
| `version`    | `String`                | Crate version.                           |
| `iterations` | `u32`                   | Iterations completed.                    |
| `tests`      | `Vec<TestReliability>`  | Per-test records.                        |

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
    SubprocessFailed(String),
    ParseError(String),
}
```

Typical remediation: ensure `cargo test --no-fail-fast` runs cleanly
in your project before running flaky-test detection.
