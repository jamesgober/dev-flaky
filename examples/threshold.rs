//! Demonstrate `reliability_threshold` and the `allow` list.
//!
//! ```text
//! CARGO_TARGET_DIR=/tmp/flaky-target cargo run --example threshold
//! ```

use dev_flaky::{FlakyError, FlakyRun};

fn main() {
    let run = FlakyRun::new("example", "0.1.0")
        .iterations(20)
        .reliability_threshold(99.0)
        .allow("known_flaky::flaky_under_load")
        .allow_all(["other_known_flaky"]);

    match run.execute() {
        Ok(result) => {
            println!("Tests below threshold ({} flaky):", result.flaky_count());
            for t in &result.tests {
                if t.is_flaky() {
                    println!("  - {} ({:.1}%)", t.name, t.reliability_pct());
                }
            }
        }
        Err(FlakyError::ToolNotInstalled) => {
            eprintln!("cargo is not on PATH; skipping example.");
        }
        Err(e) => eprintln!("flaky run failed: {e}"),
    }
}
