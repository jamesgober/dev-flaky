//! Run the test suite a *lot* of times — useful for catching low-rate
//! flakiness (e.g. 1-in-100 timing races) that shorter runs miss.
//!
//! ```text
//! CARGO_TARGET_DIR=/tmp/flaky-target cargo run --example iterations_high
//! ```

use dev_flaky::{FlakyError, FlakyRun};

fn main() {
    let run = FlakyRun::new("example", "0.1.0")
        .iterations(50)
        .test_filter("integration::");

    match run.execute() {
        Ok(result) => {
            println!(
                "iterations: {}, tests: {}, stable: {}, flaky: {}, broken: {}",
                result.iterations,
                result.total_count(),
                result.stable_count(),
                result.flaky_count(),
                result.broken_count()
            );
            for t in &result.tests {
                println!(
                    "  {} ({}/{} ok, {:.1}%)",
                    t.name,
                    t.passes,
                    t.passes + t.failures,
                    t.reliability_pct()
                );
            }
        }
        Err(FlakyError::ToolNotInstalled) => {
            eprintln!("cargo is not on PATH; skipping example.");
        }
        Err(e) => eprintln!("flaky run failed: {e}"),
    }
}
