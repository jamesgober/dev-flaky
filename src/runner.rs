//! `cargo test` repeated-run driver + libtest output parser.
//!
//! libtest prints one block per test binary on stdout:
//!
//! ```text
//! running 3 tests
//! test path::to::a ... ok
//! test path::to::b ... FAILED
//! test path::to::c ... ignored
//!
//! failures:
//!
//! ---- path::to::b stdout ----
//! (captured output of the failing test)
//!
//! test result: FAILED. 1 passed; 1 failed; 1 ignored; ...
//! ```
//!
//! Outcome lines are read only from the list that follows
//! `running N tests`, up to the first blank line. Everything after that
//! (captured test output) is skipped until `test result:`, so a test that
//! prints something looking like `test x ... ok` cannot create a phantom
//! entry. A block that never reaches `test result:` belongs to a test
//! binary that crashed or was killed by the iteration timeout.
//!
//! cargo prints a matching `Running ...` / `Doc-tests ...` line on stderr
//! for each binary. Those labels are used to tell apart tests that share
//! a name across binaries (e.g. `tests::smoke` in two workspace crates).
//!
//! We parse those blocks across N iterations and accumulate pass / fail
//! counters per test. Ignored tests are skipped.

use std::collections::BTreeMap;
use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::{FlakyError, FlakyResult, FlakyRun, TestReliability};

pub(crate) fn run(cfg: &FlakyRun) -> Result<FlakyResult, FlakyError> {
    detect_cargo()?;
    prebuild(cfg)?;

    let mut acc = Accumulator::default();
    let mut iterations_completed: u32 = 0;
    let mut last_subprocess_error: Option<String> = None;

    for _ in 0..cfg.iteration_count() {
        let output = run_cargo_test(cfg)?;
        let sections = parse_libtest(&output.stdout);

        // No test blocks at all and a non-zero exit: cargo itself failed
        // (or the iteration timed out before any test binary started).
        // Record it and keep going; it only becomes the error if no
        // iteration ever produced test results.
        if sections.is_empty() && !output.success {
            last_subprocess_error = Some(output.describe_failure());
            continue;
        }

        acc.add_iteration(&sections, &parse_section_labels(&output.stderr));
        iterations_completed += 1;
    }

    if acc.is_empty() {
        if let Some(err) = last_subprocess_error {
            return Err(FlakyError::SubprocessFailed(err));
        }
    }

    let mut tests = acc.finish();
    let allow = cfg.allow_list_view();
    if !allow.is_empty() {
        tests.retain(|t| !allow.iter().any(|n| allow_matches(n, &t.name)));
    }

    Ok(FlakyResult {
        name: cfg.subject().to_string(),
        version: cfg.subject_version().to_string(),
        iterations: iterations_completed,
        tests,
        reliability_threshold_pct: cfg.reliability_threshold_value(),
    })
}

/// An allow-list entry matches the test name exactly, or the base name of
/// a disambiguated `name [binary]` entry.
fn allow_matches(allowed: &str, test: &str) -> bool {
    test == allowed
        || test
            .strip_prefix(allowed)
            .is_some_and(|rest| rest.starts_with(" ["))
}

fn detect_cargo() -> Result<(), FlakyError> {
    match Command::new("cargo").arg("--version").output() {
        Ok(o) if o.status.success() => Ok(()),
        _ => Err(FlakyError::ToolNotInstalled),
    }
}

/// `cargo test` with the run's cargo-level flags, without the libtest
/// arguments.
fn cargo_test_command(cfg: &FlakyRun) -> Command {
    let mut cmd = Command::new("cargo");
    cmd.args(["test", "--no-fail-fast"]);
    if cfg.workspace_flag() {
        cmd.arg("--workspace");
    }
    if let Some(features) = cfg.features_flag() {
        cmd.args(["--features", features]);
    }
    if let Some(dir) = cfg.workdir_path() {
        cmd.current_dir(dir);
    }
    cmd
}

/// Build the test binaries once (`cargo test --no-run`) so a compile
/// error is reported straight away instead of once per iteration, and so
/// the iteration timeout only covers running the tests.
fn prebuild(cfg: &FlakyRun) -> Result<(), FlakyError> {
    let mut cmd = cargo_test_command(cfg);
    cmd.arg("--no-run");
    let output = cmd.output().map_err(|e| {
        FlakyError::SubprocessFailed(format!("could not spawn cargo test --no-run: {e}"))
    })?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(FlakyError::SubprocessFailed(format!(
        "cargo test --no-run exited with {}: {}",
        output.status,
        stderr.trim()
    )))
}

/// Captured output of one `cargo test` iteration.
struct Captured {
    stdout: String,
    stderr: String,
    success: bool,
    status: String,
    timed_out: Option<Duration>,
}

impl Captured {
    fn describe_failure(&self) -> String {
        match self.timed_out {
            Some(limit) => format!(
                "cargo test timed out after {:.1}s before any test ran: {}",
                limit.as_secs_f64(),
                self.stderr.trim()
            ),
            None => format!(
                "cargo test exited with {}: {}",
                self.status,
                self.stderr.trim()
            ),
        }
    }
}

fn run_cargo_test(cfg: &FlakyRun) -> Result<Captured, FlakyError> {
    let mut cmd = cargo_test_command(cfg);
    // The double-dash separates cargo args from libtest args.
    cmd.arg("--");
    if let Some(filter) = cfg.test_filter_str() {
        cmd.arg(filter);
    }

    let Some(limit) = cfg.iteration_timeout_value() else {
        let output = cmd.output().map_err(|e| {
            FlakyError::SubprocessFailed(format!("could not spawn cargo test: {e}"))
        })?;
        return Ok(Captured {
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            success: output.status.success(),
            status: output.status.to_string(),
            timed_out: None,
        });
    };
    run_with_timeout(cmd, limit)
}

/// Spawn `cmd`, drain stdout / stderr on helper threads, and kill the
/// whole process tree (cargo plus the test binary it started) if it is
/// still running after `limit`.
fn run_with_timeout(mut cmd: Command, limit: Duration) -> Result<Captured, FlakyError> {
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // Own process group, so the test binary cargo spawns can be killed
    // together with cargo.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    let mut child = cmd
        .spawn()
        .map_err(|e| FlakyError::SubprocessFailed(format!("could not spawn cargo test: {e}")))?;

    let stdout_buf = Arc::new(Mutex::new(Vec::new()));
    let stderr_buf = Arc::new(Mutex::new(Vec::new()));
    let readers = [
        child
            .stdout
            .take()
            .map(|pipe| drain(pipe, Arc::clone(&stdout_buf))),
        child
            .stderr
            .take()
            .map(|pipe| drain(pipe, Arc::clone(&stderr_buf))),
    ];

    let start = Instant::now();
    let mut timed_out = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => {}
            Err(_) => {
                // Cannot poll the child any more; do not leave it running.
                kill_tree(&mut child);
                break child.wait().ok();
            }
        }
        let elapsed = start.elapsed();
        if elapsed >= limit {
            timed_out = true;
            kill_tree(&mut child);
            break child.wait().ok();
        }
        thread::sleep((limit - elapsed).min(Duration::from_millis(50)));
    };

    // The readers finish once every process holding the pipes has
    // exited. If a grandchild survived the kill, stop waiting after a
    // grace period and use what has been read so far.
    let grace = Instant::now() + Duration::from_secs(5);
    for handle in readers.into_iter().flatten() {
        while !handle.is_finished() && Instant::now() < grace {
            thread::sleep(Duration::from_millis(10));
        }
        if handle.is_finished() {
            let _ = handle.join();
        }
    }

    let snapshot = |buf: &Arc<Mutex<Vec<u8>>>| -> String {
        match buf.lock() {
            Ok(b) => String::from_utf8_lossy(&b).into_owned(),
            Err(poisoned) => String::from_utf8_lossy(&poisoned.into_inner()).into_owned(),
        }
    };
    Ok(Captured {
        stdout: snapshot(&stdout_buf),
        stderr: snapshot(&stderr_buf),
        success: !timed_out && status.is_some_and(|s| s.success()),
        status: status.map_or_else(|| "unknown status".to_string(), |s| s.to_string()),
        timed_out: timed_out.then_some(limit),
    })
}

fn drain<R: Read + Send + 'static>(
    mut pipe: R,
    buf: Arc<Mutex<Vec<u8>>>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut chunk = [0u8; 8192];
        loop {
            match pipe.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(n) => match buf.lock() {
                    Ok(mut b) => b.extend_from_slice(&chunk[..n]),
                    Err(_) => break,
                },
            }
        }
    })
}

/// Kill `child` and every process it started.
fn kill_tree(child: &mut std::process::Child) {
    let pid = child.id().to_string();
    #[cfg(windows)]
    {
        let _ = Command::new("taskkill")
            .args(["/T", "/F", "/PID", &pid])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    #[cfg(unix)]
    {
        // The child leads its own process group (see `process_group(0)`).
        let _ = Command::new("kill")
            .args(["-KILL", "--", &format!("-{pid}")])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    #[cfg(not(any(windows, unix)))]
    let _ = pid;
    let _ = child.kill();
}

// ---------------------------------------------------------------------------
// Libtest output parser
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Outcome {
    Pass,
    Fail,
    Ignored,
}

/// One test binary's block of libtest output.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct Section {
    /// `(test name, outcome)` in output order.
    pub(crate) outcomes: Vec<(String, Outcome)>,
    /// Tests seen running without an outcome yet: a `test x ... ` line
    /// with nothing after it (single-threaded runs), or libtest's
    /// `test x has been running for over 60 seconds` notice.
    pub(crate) running: Vec<String>,
    /// The block reached `test result:`. `false` means the binary
    /// crashed or was killed.
    pub(crate) finished: bool,
}

#[derive(PartialEq, Eq)]
enum State {
    /// Between blocks.
    Outside,
    /// Reading `test x ... outcome` lines.
    List,
    /// Blank line after the list; the next line tells whether failure
    /// details follow, the summary follows, or (when the binary died)
    /// the next binary's header follows.
    AfterList,
    /// Failure details / captured output, until `test result:`.
    Details,
}

pub(crate) fn parse_libtest(stdout: &str) -> Vec<Section> {
    let mut sections: Vec<Section> = Vec::new();
    let mut state = State::Outside;
    for raw in stdout.lines() {
        let line = raw.trim_end();
        if state != State::Details && is_running_header(line) {
            // A new header before the previous block's summary means the
            // previous binary died.
            sections.push(Section::default());
            state = State::List;
            continue;
        }
        let Some(current) = sections.last_mut() else {
            continue;
        };
        if line.starts_with("test result: ") {
            if state != State::Outside {
                current.finished = true;
            }
            state = State::Outside;
            continue;
        }
        match state {
            State::List => {}
            State::AfterList => {
                if !line.is_empty() {
                    state = State::Details;
                }
                continue;
            }
            State::Outside | State::Details => continue,
        }
        if line.is_empty() {
            state = State::AfterList;
            continue;
        }
        let Some(rest) = line.strip_prefix("test ") else {
            continue;
        };
        if let Some(name) = rest.strip_suffix(" has been running for over 60 seconds") {
            current.running.push(normalize_name(name));
            continue;
        }
        // `trim_end` above turns a pending `test x ... ` into `test x ...`.
        if let Some(name) = rest.strip_suffix(" ...") {
            current.running.push(normalize_name(name));
            continue;
        }
        let Some((name, outcome)) = rest.rsplit_once(" ... ") else {
            continue;
        };
        let word = outcome.split_whitespace().next().unwrap_or("");
        let kind = match word.trim_end_matches(',') {
            "ok" => Outcome::Pass,
            "FAILED" => Outcome::Fail,
            "ignored" => Outcome::Ignored,
            _ => continue,
        };
        current.outcomes.push((normalize_name(name), kind));
    }
    sections
}

/// `running 3 tests` / `running 1 test`.
fn is_running_header(line: &str) -> bool {
    let Some(rest) = line.strip_prefix("running ") else {
        return false;
    };
    let Some((count, word)) = rest.split_once(' ') else {
        return false;
    };
    !count.is_empty()
        && count.bytes().all(|b| b.is_ascii_digit())
        && matches!(word, "test" | "tests")
}

/// libtest appends ` - should panic` to `#[should_panic]` tests; drop it
/// so the name is the test path the allow-list expects.
fn normalize_name(name: &str) -> String {
    name.strip_suffix(" - should panic")
        .unwrap_or(name)
        .to_string()
}

/// One label per test binary, in run order, from cargo's stderr:
/// `Running unittests src/lib.rs (target/debug/deps/app-0123456789abcdef)`
/// becomes `app: unittests src/lib.rs`, `Doc-tests app` becomes
/// `doc-tests app`.
pub(crate) fn parse_section_labels(stderr: &str) -> Vec<String> {
    let mut labels = Vec::new();
    for raw in stderr.lines() {
        let line = raw.trim();
        if let Some(krate) = line.strip_prefix("Doc-tests ") {
            labels.push(format!("doc-tests {}", krate.trim()));
            continue;
        }
        let Some(rest) = line.strip_prefix("Running ") else {
            continue;
        };
        let rest = rest.trim();
        let (desc, path) = match rest.rsplit_once(" (") {
            Some((desc, path)) if path.ends_with(')') => (Some(desc), path.trim_end_matches(')')),
            _ => (None, rest),
        };
        let stem = binary_stem(path);
        let label = match desc {
            Some(d) => format!("{stem}: {}", d.replace('\\', "/")),
            None => stem,
        };
        labels.push(label);
    }
    labels
}

/// `target\debug\deps\app-0123456789abcdef.exe` -> `app`.
fn binary_stem(path: &str) -> String {
    let file = path.rsplit(['/', '\\']).next().unwrap_or(path);
    let file = file.strip_suffix(".exe").unwrap_or(file);
    match file.rsplit_once('-') {
        Some((stem, hash)) if !hash.is_empty() && hash.bytes().all(|b| b.is_ascii_hexdigit()) => {
            stem.to_string()
        }
        _ => file.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Accumulation across iterations
// ---------------------------------------------------------------------------

/// Counters keyed by `(binary ordinal, test name)`. The order in which
/// cargo runs test binaries is the same every iteration, so the ordinal
/// identifies the binary.
#[derive(Default)]
pub(crate) struct Accumulator {
    counts: BTreeMap<(usize, String), (u32, u32)>,
    labels: BTreeMap<usize, String>,
    /// Per binary: (iterations that finished or were attributed to a
    /// named test, iterations that ended early with no test to blame).
    unfinished: BTreeMap<usize, (u32, u32)>,
}

impl Accumulator {
    pub(crate) fn is_empty(&self) -> bool {
        self.counts.is_empty() && self.unfinished.values().all(|(_, bad)| *bad == 0)
    }

    pub(crate) fn add_iteration(&mut self, sections: &[Section], labels: &[String]) {
        // Labels line up with blocks only when cargo announced exactly
        // one binary per block (a `harness = false` target prints no
        // libtest block).
        let use_labels = labels.len() == sections.len();
        for (i, s) in sections.iter().enumerate() {
            if use_labels {
                self.labels.entry(i).or_insert_with(|| labels[i].clone());
            }
            for (name, outcome) in &s.outcomes {
                let delta = match outcome {
                    Outcome::Pass => (1, 0),
                    Outcome::Fail => (0, 1),
                    Outcome::Ignored => continue,
                };
                let e = self.counts.entry((i, name.clone())).or_insert((0, 0));
                e.0 = e.0.saturating_add(delta.0);
                e.1 = e.1.saturating_add(delta.1);
            }
            let fin = self.unfinished.entry(i).or_insert((0, 0));
            if s.finished {
                fin.0 = fin.0.saturating_add(1);
                continue;
            }
            // The binary died. Blame the tests that were seen running
            // without an outcome; if none, record it against the binary.
            let mut blamed = false;
            for name in &s.running {
                if s.outcomes.iter().any(|(n, _)| n == name) {
                    continue;
                }
                let e = self.counts.entry((i, name.clone())).or_insert((0, 0));
                e.1 = e.1.saturating_add(1);
                blamed = true;
            }
            if blamed {
                fin.0 = fin.0.saturating_add(1);
            } else {
                fin.1 = fin.1.saturating_add(1);
            }
        }
    }

    pub(crate) fn finish(self) -> Vec<TestReliability> {
        let label_of = |i: usize| -> String {
            self.labels
                .get(&i)
                .cloned()
                .unwrap_or_else(|| format!("#{}", i + 1))
        };

        // Which binaries each name appears in.
        let mut ordinals_by_name: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
        for (i, name) in self.counts.keys() {
            ordinals_by_name.entry(name.as_str()).or_default().push(*i);
        }

        let mut tests: Vec<TestReliability> = Vec::new();
        for ((i, name), (passes, failures)) in &self.counts {
            let display = match ordinals_by_name.get(name.as_str()) {
                Some(ords) if ords.len() > 1 => {
                    let label = label_of(*i);
                    let label_unique = ords.iter().filter(|o| label_of(**o) == label).count() == 1;
                    if label_unique {
                        format!("{name} [{label}]")
                    } else {
                        format!("{name} [#{}]", i + 1)
                    }
                }
                _ => name.clone(),
            };
            tests.push(TestReliability {
                name: display,
                passes: *passes,
                failures: *failures,
            });
        }
        for (i, (ok, bad)) in &self.unfinished {
            if *bad == 0 {
                continue;
            }
            tests.push(TestReliability {
                name: format!("{}: test binary did not finish", label_of(*i)),
                passes: *ok,
                failures: *bad,
            });
        }
        tests.sort_by(|a, b| a.name.cmp(&b.name));
        tests
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn outcomes(stdout: &str) -> Vec<(String, Outcome)> {
        parse_libtest(stdout)
            .into_iter()
            .flat_map(|s| s.outcomes)
            .collect()
    }

    /// Real `cargo test --no-fail-fast` stdout (Rust 1.9x, Windows) for a
    /// crate with unit tests, two integration test files that share a
    /// test name, and a doc-test.
    const REAL_STDOUT: &str = "
running 7 tests
test tests::ignored_plain ... ignored
test tests::ignored_with_reason ... ignored, slow
test tests::flaky ... ok
test tests::fails ... FAILED
test tests::prints_fake_lines ... FAILED
test tests::passes ... ok
test tests::should_panic_ok - should panic ... ok

failures:

---- tests::fails stdout ----

thread 'tests::fails' (51496) panicked at src\\lib.rs:13:18:
boom
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

---- tests::prints_fake_lines stdout ----
test fake::phantom ... ok
running 9 tests
test fake::second_phantom ... FAILED

thread 'tests::prints_fake_lines' (92104) panicked at src\\lib.rs:24:98:
x


failures:
    tests::fails
    tests::prints_fake_lines

test result: FAILED. 3 passed; 2 failed; 2 ignored; 0 measured; 0 filtered out; finished in 0.00s


running 1 test
test shared_name ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s


running 1 test
test shared_name ... FAILED

failures:

---- shared_name stdout ----

thread 'shared_name' (113212) panicked at tests\\b.rs:2:20:
b fails
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace


failures:
    shared_name

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s


running 1 test
test src\\lib.rs - one (line 3) ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.22s

";

    const REAL_STDERR: &str =
        "    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.67s
     Running unittests src\\lib.rs (target\\debug\\deps\\proj-32657e2f297e82de.exe)
error: test failed, to rerun pass `--lib`
     Running tests\\a.rs (target\\debug\\deps\\a-15cbccc83786b531.exe)
     Running tests\\b.rs (target\\debug\\deps\\b-4ef922a6ae124fc1.exe)
error: test failed, to rerun pass `--test b`
   Doc-tests proj
error: 2 targets failed:
    `--lib`
    `--test b`
";

    #[test]
    fn parses_real_output_blocks() {
        let sections = parse_libtest(REAL_STDOUT);
        assert_eq!(sections.len(), 4);
        assert!(sections.iter().all(|s| s.finished));
        let unit = &sections[0].outcomes;
        assert_eq!(unit.len(), 7);
        assert!(unit.contains(&("tests::ignored_with_reason".into(), Outcome::Ignored)));
        assert!(unit.contains(&("tests::fails".into(), Outcome::Fail)));
        // ` - should panic` is stripped.
        assert!(unit.contains(&("tests::should_panic_ok".into(), Outcome::Pass)));
        // Captured output that looks like libtest lines is not parsed.
        assert!(!unit.iter().any(|(n, _)| n.starts_with("fake::")));
        // Doc-test names keep their spaces.
        assert_eq!(
            sections[3].outcomes,
            vec![("src\\lib.rs - one (line 3)".into(), Outcome::Pass)]
        );
    }

    #[test]
    fn labels_come_from_cargo_stderr() {
        assert_eq!(
            parse_section_labels(REAL_STDERR),
            vec![
                "proj: unittests src/lib.rs",
                "a: tests/a.rs",
                "b: tests/b.rs",
                "doc-tests proj",
            ]
        );
        // Unix paths, no extension, and the old format without a
        // description.
        assert_eq!(
            parse_section_labels(
                "     Running unittests src/main.rs (target/debug/deps/app-95f19e21e5fbdd16)\n     Running target/debug/deps/legacy-0123abcd\n"
            ),
            vec!["app: unittests src/main.rs", "legacy"]
        );
    }

    #[test]
    fn same_name_in_two_binaries_is_not_merged() {
        let mut acc = Accumulator::default();
        let sections = parse_libtest(REAL_STDOUT);
        let labels = parse_section_labels(REAL_STDERR);
        for _ in 0..3 {
            acc.add_iteration(&sections, &labels);
        }
        let tests = acc.finish();
        let get = |n: &str| tests.iter().find(|t| t.name == n).unwrap();
        // tests/a.rs always passes, tests/b.rs always fails: one stable,
        // one broken, not a single "flaky" record.
        let a = get("shared_name [a: tests/a.rs]");
        assert_eq!((a.passes, a.failures), (3, 0));
        let b = get("shared_name [b: tests/b.rs]");
        assert_eq!((b.passes, b.failures), (0, 3));
        // Unique names stay plain.
        assert_eq!(get("tests::passes").passes, 3);
        // Ignored tests produce no record at all.
        assert!(!tests.iter().any(|t| t.name.contains("ignored")));
    }

    #[test]
    fn duplicate_names_without_labels_use_ordinals() {
        let mut acc = Accumulator::default();
        let sections = parse_libtest(REAL_STDOUT);
        acc.add_iteration(&sections, &[]);
        let tests = acc.finish();
        assert!(tests.iter().any(|t| t.name == "shared_name [#2]"));
        assert!(tests.iter().any(|t| t.name == "shared_name [#3]"));
    }

    #[test]
    fn crashed_binary_is_recorded() {
        // Real output of a test binary killed by `std::process::abort()`
        // (multi-threaded: no test can be blamed), followed by the next
        // binary.
        let crashed = "\nrunning 3 tests\n\nrunning 1 test\ntest other ... ok\n\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n\n";
        let clean = "\nrunning 3 tests\ntest aborts ... ok\ntest before ... ok\ntest zz_after ... ok\n\ntest result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.30s\n\n\nrunning 1 test\ntest other ... ok\n\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n\n";
        let sections = parse_libtest(crashed);
        assert_eq!(sections.len(), 2);
        assert!(!sections[0].finished);
        assert!(sections[1].finished);

        let labels = vec![
            "crash: tests/crash.rs".to_string(),
            "other: tests/other.rs".to_string(),
        ];
        let mut acc = Accumulator::default();
        acc.add_iteration(&sections, &labels);
        for _ in 0..3 {
            acc.add_iteration(&parse_libtest(clean), &labels);
        }
        let tests = acc.finish();
        let crash = tests
            .iter()
            .find(|t| t.name == "crash: tests/crash.rs: test binary did not finish")
            .unwrap();
        assert_eq!((crash.passes, crash.failures), (3, 1));
        assert!(crash.is_flaky());
        // Binaries that never crashed get no synthetic record.
        assert!(!tests.iter().any(|t| t.name.starts_with("other:")));
    }

    #[test]
    fn crash_with_pending_test_blames_that_test() {
        // Single-threaded run: libtest prints the name before running it.
        let stdout = "\nrunning 3 tests\ntest aborts ... ";
        let sections = parse_libtest(stdout);
        assert_eq!(sections[0].running, vec!["aborts".to_string()]);
        let mut acc = Accumulator::default();
        acc.add_iteration(&sections, &[]);
        let tests = acc.finish();
        assert_eq!(tests.len(), 1);
        assert_eq!(tests[0].name, "aborts");
        assert_eq!((tests[0].passes, tests[0].failures), (0, 1));
    }

    #[test]
    fn hung_test_reported_by_libtest_is_blamed() {
        // Killed by the iteration timeout after libtest's 60 s notice.
        let stdout = "\nrunning 2 tests\ntest quick ... ok\ntest slow::deadlock has been running for over 60 seconds\n";
        let mut acc = Accumulator::default();
        acc.add_iteration(&parse_libtest(stdout), &[]);
        let tests = acc.finish();
        let slow = tests.iter().find(|t| t.name == "slow::deadlock").unwrap();
        assert_eq!(slow.failures, 1);
        assert_eq!(tests.iter().find(|t| t.name == "quick").unwrap().passes, 1);
    }

    #[test]
    fn slow_test_that_finishes_is_not_blamed() {
        let stdout = "\nrunning 1 test\ntest slow has been running for over 60 seconds\ntest slow ... ok\n\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 61.00s\n";
        let mut acc = Accumulator::default();
        acc.add_iteration(&parse_libtest(stdout), &[]);
        let tests = acc.finish();
        assert_eq!(tests.len(), 1);
        assert_eq!((tests[0].passes, tests[0].failures), (1, 0));
    }

    #[test]
    fn crlf_output_parses_the_same() {
        let crlf = REAL_STDOUT.replace('\n', "\r\n");
        assert_eq!(parse_libtest(&crlf), parse_libtest(REAL_STDOUT));
        let labels = parse_section_labels(&REAL_STDERR.replace('\n', "\r\n"));
        assert_eq!(labels.len(), 4);
        assert_eq!(labels[1], "a: tests/a.rs");
    }

    #[test]
    fn non_utf8_output_is_decoded_lossily() {
        let mut bytes =
            b"\nrunning 1 test\ntest bad_\xff_name ... ok\n\ntest result: ok. 1 passed".to_vec();
        bytes.extend_from_slice(b"\n");
        let text = String::from_utf8_lossy(&bytes);
        let o = outcomes(&text);
        assert_eq!(o.len(), 1);
        assert_eq!(o[0].0, "bad_\u{fffd}_name");
    }

    #[test]
    fn parses_ok_failed_ignored() {
        let stdout = "\
running 4 tests
test foo::bar ... ok
test foo::baz ... FAILED
test foo::qux ... ignored
test foo::quux ... ok (0.01s)

failures:
foo::baz
";
        let outcomes = outcomes(stdout);
        assert_eq!(outcomes.len(), 4);
        assert_eq!(outcomes[0], ("foo::bar".into(), Outcome::Pass));
        assert_eq!(outcomes[1], ("foo::baz".into(), Outcome::Fail));
        assert_eq!(outcomes[2], ("foo::qux".into(), Outcome::Ignored));
        assert_eq!(outcomes[3], ("foo::quux".into(), Outcome::Pass));
    }

    #[test]
    fn skips_summary_lines() {
        let stdout = "\
running 1 test
test foo::a ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
";
        let outcomes = outcomes(stdout);
        assert_eq!(outcomes.len(), 1);
        assert_eq!(outcomes[0].0, "foo::a");
    }

    #[test]
    fn ignores_unrelated_lines() {
        let stdout = "\
   Compiling foo v0.1.0
test outside_any_block ... ok
running 1 test
test test_a ... ok
hello world
";
        let outcomes = outcomes(stdout);
        assert_eq!(outcomes.len(), 1);
        assert_eq!(outcomes[0].0, "test_a");
    }

    #[test]
    fn ignores_unknown_outcomes() {
        let stdout = "running 3 tests\ntest foo ... maybe\ntest bar ... ok\ntest b ... bench:   1,234 ns/iter (+/- 5)\n";
        let outcomes = outcomes(stdout);
        assert_eq!(outcomes.len(), 1);
        assert_eq!(outcomes[0].0, "bar");
    }

    #[test]
    fn running_header_requires_a_count() {
        assert!(is_running_header("running 0 tests"));
        assert!(is_running_header("running 1 test"));
        assert!(is_running_header("running 12 tests"));
        assert!(!is_running_header("running tests"));
        assert!(!is_running_header("running 1 benchmark"));
        assert!(!is_running_header("running x tests"));
    }

    #[test]
    fn binary_stem_strips_hash_and_extension() {
        assert_eq!(
            binary_stem("target\\debug\\deps\\app-95f19e21e5fbdd16.exe"),
            "app"
        );
        assert_eq!(
            binary_stem("target/debug/deps/my-crate-0123abcd"),
            "my-crate"
        );
        assert_eq!(binary_stem("target/debug/deps/my-crate"), "my-crate");
    }

    #[test]
    fn allow_list_matches_plain_and_disambiguated_names() {
        assert!(allow_matches("shared_name", "shared_name"));
        assert!(allow_matches("shared_name", "shared_name [b: tests/b.rs]"));
        assert!(!allow_matches("shared", "shared_name"));
        assert!(!allow_matches("shared", "shared_name [b: tests/b.rs]"));
    }

    #[test]
    fn empty_input_yields_empty_output() {
        assert!(parse_libtest("").is_empty());
        let acc = Accumulator::default();
        assert!(acc.is_empty());
        assert!(acc.finish().is_empty());
    }
}
