//! Minimal example: detect flaky tests by repeated runs.
//!
//! Run with: `cargo run --example basic`

use dev_flaky::FlakyRun;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let run = FlakyRun::new("example", "0.1.0").iterations(20);
    let result = run.execute()?;
    println!("Flaky tests: {}", result.flaky_count());
    let report = result.into_report();
    println!("{}", report.to_json()?);
    Ok(())
}
