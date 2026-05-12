//! Run the test suite a few times and print the resulting `Report`.
//!
//! ```text
//! cargo run --example basic
//! ```
//!
//! **Important:** invoking this from inside `cargo test` (or from a
//! working directory where another `cargo` invocation already holds
//! the target-dir lock) will deadlock. Run it from a checkout of the
//! crate you actually want to scan for flakiness, e.g.:
//!
//! ```text
//! cd /path/to/your-crate
//! CARGO_TARGET_DIR=/tmp/flaky-target cargo run --manifest-path /path/to/dev-flaky/Cargo.toml --example basic
//! ```

use dev_flaky::{FlakyError, FlakyRun};

fn main() {
    let run = FlakyRun::new("example", "0.1.0").iterations(5);
    let result = match run.execute() {
        Ok(r) => r,
        Err(FlakyError::ToolNotInstalled) => {
            eprintln!("cargo is not on PATH; skipping example.");
            return;
        }
        Err(e) => {
            eprintln!("flaky run failed: {e}");
            return;
        }
    };
    println!(
        "iterations completed: {}, total tests observed: {}, flaky: {}, broken: {}",
        result.iterations,
        result.total_count(),
        result.flaky_count(),
        result.broken_count()
    );
    let report = result.into_report();
    println!("{}", report.to_json().expect("serialize report"));
}
