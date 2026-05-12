//! Demonstrate `FlakyProducer` — the `dev_report::Producer` adapter.
//!
//! Construction is always safe and cheap. The actual subprocess
//! invocation is gated behind the `DEV_FLAKY_EXAMPLE_RUN` env var so
//! CI doesn't pay for `iterations × cargo test` on every example build.
//!
//! ```text
//! cargo run --example producer
//! DEV_FLAKY_EXAMPLE_RUN=1 cargo run --example producer
//! ```

use dev_flaky::{FlakyProducer, FlakyRun};
use dev_report::Producer;

fn main() {
    let producer = FlakyProducer::new(FlakyRun::new("my-crate", "0.1.0").iterations(5));
    println!("Constructed FlakyProducer for 'my-crate' v0.1.0.");

    if std::env::var("DEV_FLAKY_EXAMPLE_RUN").is_ok() {
        let report = producer.produce();
        println!("{}", report.to_json().expect("serialize report"));
    } else {
        println!("Set DEV_FLAKY_EXAMPLE_RUN=1 to run `cargo test` 5 times and");
        println!("print the resulting JSON report.");
    }
}
