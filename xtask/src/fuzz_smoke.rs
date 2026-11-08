// Copyright 2026 Joseph Anthony Abbott III
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! `cargo xtask fuzz-smoke [--secs N] [--target T]`: the mutation fuzzer
//! over the parsers and decoders of untrusted bytes (`docs/04-quality.md`,
//! "Fuzz smoke").
//!
//! Each target runs its seeds from `fuzz/corpus/<target>/` unchanged, then
//! mutated inputs (`mutate`) until N seconds from its start. A worker
//! thread runs each input under `catch_unwind` while this thread watches
//! it. A panic is a finding, saved to `fuzz/crashes/<target>/` once per
//! panic location; so is an input still running after 2 s, which ends the
//! target's run.

use crate::files;
use crate::fuzz_targets::{TARGETS, Target};
use crate::mutate::Inputs;
use hardshell_core::hash::fnv1a64;
use hardshell_core::testing::{SEED_VAR, seed_from};
use std::any::Any;
use std::cell::{Cell, RefCell};
use std::collections::BTreeSet;
use std::env;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::mem;
use std::panic::{self, AssertUnwindSafe, PanicHookInfo};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Once};
use std::thread;
use std::time::{Duration, Instant};

/// How long one input may run before it is a hang.
pub(crate) const HANG: Duration = Duration::from_secs(2);

/// The seconds each target runs without `--secs`.
const DEFAULT_SECS: u64 = 60;

/// The longest panic message a finding keeps, in characters.
const MAX_MESSAGE: usize = 1_000;

/// The most bytes of an input a finding shows.
const SHOWN: usize = 32;

/// Where a panic that does not say is reported to be.
const UNKNOWN_LOCATION: &str = "an unknown location";

/// What `fuzz-smoke` was asked to do.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Options {
    /// The seconds each target runs, counted from its start: its seeds
    /// always run, even past them.
    pub(crate) secs: u64,
    /// One target, by its index in `TARGETS`, or every target.
    pub(crate) target: Option<usize>,
}

/// Reads `fuzz-smoke`'s arguments: `--secs N` and `--target T`, each at
/// most once, in either order.
pub(crate) fn parse(args: &[&str]) -> Result<Options, String> {
    let (mut secs, mut target) = (None, None);
    let mut args = args.iter();
    while let Some(&flag) = args.next() {
        if flag != "--secs" && flag != "--target" {
            return Err(format!(
                "`fuzz-smoke` takes `--secs N` and `--target T`, but was given `{flag}`"
            ));
        }
        let Some(&value) = args.next() else {
            return Err(format!("`{flag}` needs a value"));
        };
        let given_twice = if flag == "--secs" {
            secs.replace(parse_secs(value)?).is_some()
        } else {
            target.replace(find_target(value)?).is_some()
        };
        if given_twice {
            return Err(format!("`{flag}` given twice"));
        }
    }
    Ok(Options {
        secs: secs.unwrap_or(DEFAULT_SECS),
        target,
    })
}

fn parse_secs(value: &str) -> Result<u64, String> {
    // `u64::from_str` also takes a leading `+`.
    match value.parse() {
        Ok(secs) if value.bytes().all(|b| b.is_ascii_digit()) => Ok(secs),
        _ => Err(format!(
            "`--secs` takes a whole number of seconds, not `{value}`"
        )),
    }
}

fn find_target(name: &str) -> Result<usize, String> {
    TARGETS
        .iter()
        .position(|target| target.name == name)
        .ok_or_else(|| {
            let names: Vec<&str> = TARGETS.iter().map(|target| target.name).collect();
            format!(
                "unknown fuzz target `{name}`; the targets are {}",
                names.join(", ")
            )
        })
}

/// When a target's run stops, once its seeds have run.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Budget {
    /// After this long; never if the clock cannot count that far.
    pub(crate) time: Duration,
    /// After this many inputs, the seeds included, if set.
    pub(crate) inputs: Option<u64>,
}

/// Runs `options`' targets in the workspace at `root`, printing as it goes,
/// and says whether every target ran and found nothing.
pub(crate) fn run(root: &Path, options: &Options) -> bool {
    let targets = match options.target {
        Some(index) => TARGETS.get(index..=index).unwrap_or_default(),
        None => TARGETS,
    };
    let (budget, hang) = limits(options);
    run_to(
        &mut io::stdout(),
        &mut io::stderr(),
        root,
        targets,
        seed_from(env::var_os(SEED_VAR).as_deref()),
        budget,
        hang,
    )
}

/// The budget and the hang timeout `options` give each target.
fn limits(options: &Options) -> (Budget, Duration) {
    let budget = Budget {
        time: Duration::from_secs(options.secs),
        inputs: None,
    };
    (budget, HANG)
}

/// `run` for `targets`, with `seed` as `HARDSHELL_SEED` gave it, to `out`
/// and `err`. Write errors are ignored, as in `say!`.
fn run_to(
    out: &mut dyn Write,
    err: &mut dyn Write,
    root: &Path,
    targets: &[Target],
    seed: Result<u64, String>,
    budget: Budget,
    hang: Duration,
) -> bool {
    let seed = match seed {
        Ok(seed) => seed,
        Err(message) => {
            let _ = writeln!(err, "fuzz-smoke: could not run: {message}");
            return false;
        }
    };
    let _ = writeln!(
        out,
        "fuzz-smoke: seed {seed}; {SEED_VAR}={seed} gives the same inputs"
    );
    let (mut findings, mut failed) = (0, 0);
    for target in targets {
        match run_target(out, root, target, seed, budget, hang) {
            Ok(found) => findings += found,
            Err(message) => {
                let _ = writeln!(err, "fuzz-smoke: {}: could not run: {message}", target.name);
                failed += 1;
            }
        }
    }
    let _ = match (findings, failed) {
        (0, 0) => writeln!(out, "fuzz-smoke: ok"),
        (n, 0) => writeln!(out, "fuzz-smoke: FAILED, {}", count(n, "finding")),
        (0, k) => writeln!(
            out,
            "fuzz-smoke: FAILED, {} could not run",
            count(k, "target")
        ),
        (n, k) => writeln!(
            out,
            "fuzz-smoke: FAILED, {}; {} could not run",
            count(n, "finding"),
            count(k, "target")
        ),
    };
    findings == 0 && failed == 0
}

/// Fuzzes `target`, printing each finding as it comes, then a summary, and
/// returns the number of findings.
fn run_target(
    out: &mut dyn Write,
    root: &Path,
    target: &Target,
    seed: u64,
    budget: Budget,
    hang: Duration,
) -> Result<u64, String> {
    let seeds = load_seeds(root, target)?;
    let crashes = Path::new("fuzz/crashes").join(target.name);
    let mut findings = 0;
    // Each target draws its own inputs from the one seed.
    let seed = seed ^ fnv1a64(target.name.as_bytes());
    let stats = fuzz(target, seeds, seed, budget, hang, &mut |finding| {
        findings += 1;
        let saved = save(&root.join(&crashes), &finding);
        let saved = saved.map(|name| files::display(&crashes.join(name)));
        for line in describe(&finding, saved, hang) {
            let _ = writeln!(out, "fuzz-smoke: {}: {line}", target.name);
        }
    })?;
    let _ = writeln!(out, "fuzz-smoke: {}: {}", target.name, stats.summary());
    Ok(findings)
}

/// The seeds of `target`: the files in `fuzz/corpus/<name>/` under `root`
/// whose names do not start with `.`, in name order. Directories in it are
/// not seeds.
pub(crate) fn load_seeds(root: &Path, target: &Target) -> Result<Vec<Vec<u8>>, String> {
    let dir = Path::new("fuzz/corpus").join(target.name);
    let cannot = |path: &Path, e: io::Error| format!("cannot read {}: {e}", files::display(path));
    let mut names = Vec::new();
    for entry in fs::read_dir(root.join(&dir)).map_err(|e| cannot(&dir, e))? {
        let entry = entry.map_err(|e| cannot(&dir, e))?;
        // Not `.DS_Store` and the like; `is_file` follows a link.
        if !entry.file_name().as_encoded_bytes().starts_with(b".") && entry.path().is_file() {
            names.push(entry.file_name());
        }
    }
    if names.is_empty() {
        return Err(format!("{} has no seeds", files::display(&dir)));
    }
    names.sort();
    names
        .iter()
        .map(|name| {
            let path = dir.join(name);
            let mut seed = Vec::new();
            // One byte past the limit is enough to know a seed is too long.
            let limit = target.max_len as u64 + 1;
            File::open(root.join(&path))
                .and_then(|file| file.take(limit).read_to_end(&mut seed))
                .map_err(|e| cannot(&path, e))?;
            if seed.len() > target.max_len {
                return Err(format!(
                    "{} is longer than the target's limit of {}",
                    files::display(&path),
                    count(target.max_len as u64, "byte")
                ));
            }
            Ok(seed)
        })
        .collect()
}

/// An input that panicked, or ran past the hang timeout.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Finding {
    pub(crate) kind: Kind,
    /// The input's place in the run, the seeds first.
    pub(crate) index: u64,
    pub(crate) input: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    /// A panic at `location` (`file:line:column`).
    Panic { location: String, message: String },
    /// Still running at the hang timeout.
    Hang,
}

/// What a target's run did.
#[derive(Debug)]
pub(crate) struct Stats {
    pub(crate) seeds: u64,
    /// The inputs that ran to their end, the seeds included.
    pub(crate) inputs: u64,
    /// Those of them that panicked.
    pub(crate) panicked: u64,
    /// Whether an input hung, which ended the run.
    pub(crate) hung: bool,
    pub(crate) elapsed: Duration,
}

impl Stats {
    fn summary(&self) -> String {
        // Fewer only when a seed hung.
        let seeds = if self.inputs < self.seeds {
            format!("{} of {}", self.inputs, count(self.seeds, "seed"))
        } else {
            count(self.seeds, "seed")
        };
        let mut summary = format!(
            "{seeds}, then {} in {:.1} s",
            count(self.inputs.saturating_sub(self.seeds), "mutated input"),
            self.elapsed.as_secs_f64()
        );
        if self.panicked > 0 {
            summary += &format!("; {} panicked", count(self.panicked, "input"));
        }
        if self.hung {
            summary += "; stopped by a hang";
        }
        summary
    }
}

/// What the worker thread tells the watchdog without waiting for it.
struct Progress {
    /// The inputs that ran to their end.
    finished: AtomicU64,
    /// Those of them that panicked.
    panicked: AtomicU64,
    /// Set by the watchdog: the worker runs no more inputs.
    stop: AtomicBool,
}

/// Writes input `i` of a run to the buffer: the same bytes every time.
type Build = Arc<dyn Fn(u64, &mut Vec<u8>) + Send + Sync>;

/// Fuzzes `target` with `seeds` and the inputs `seed` draws from them,
/// until `budget` runs out once the seeds have run, or an input hangs. The
/// first input that panics at each location, and a hang, go to `found` as
/// they happen.
pub(crate) fn fuzz(
    target: &Target,
    seeds: Vec<Vec<u8>>,
    seed: u64,
    budget: Budget,
    hang: Duration,
    found: &mut dyn FnMut(Finding),
) -> Result<Stats, String> {
    let seed_count = seeds.len() as u64;
    let max_len = target.max_len;
    let build: Build =
        Arc::new(move |index, out| Inputs::new(&seeds, seed, max_len).build(index, out));
    fuzz_inputs(target, seed_count, build, budget, hang, found)
}

/// `fuzz` on the inputs `build` writes, the first `seeds` of them seeds.
fn fuzz_inputs(
    target: &Target,
    seeds: u64,
    build: Build,
    budget: Budget,
    hang: Duration,
    found: &mut dyn FnMut(Finding),
) -> Result<Stats, String> {
    install_hook();
    let progress = Arc::new(Progress {
        finished: AtomicU64::new(0),
        panicked: AtomicU64::new(0),
        stop: AtomicBool::new(false),
    });
    let start = Instant::now();
    let deadline = start.checked_add(budget.time);
    let (events, findings) = mpsc::channel();
    let worker = {
        let (build, progress) = (Arc::clone(&build), Arc::clone(&progress));
        let (name, run, limit) = (target.name, target.run, budget.inputs);
        thread::Builder::new()
            .name(format!("fuzz-smoke {name}"))
            .spawn(move || {
                TARGET.set(name);
                work(run, &*build, seeds, deadline, limit, &progress, &events);
            })
            .map_err(|e| format!("cannot start a thread: {e}"))?
    };
    // Often enough to see a hang soon after its timeout.
    let poll = (hang / 20).max(Duration::from_millis(1));
    let (mut seen, mut since) = (0, Instant::now());
    let mut hung = None;
    loop {
        match findings.recv_timeout(poll) {
            Ok(finding) => found(finding),
            Err(RecvTimeoutError::Timeout) => {}
            // The worker returned, or panicked: either way it dropped its
            // sender after sending every finding.
            Err(RecvTimeoutError::Disconnected) => break,
        }
        let finished = progress.finished.load(Ordering::Relaxed);
        if finished != seen {
            (seen, since) = (finished, Instant::now());
        } else if since.elapsed() >= hang {
            // The worker is stuck in input `finished`. It is left to
            // finish it, or to the process's end, and runs no other.
            progress.stop.store(true, Ordering::Relaxed);
            let mut input = Vec::new();
            build(finished, &mut input);
            found(Finding {
                kind: Kind::Hang,
                index: finished,
                input,
            });
            hung = Some(finished);
            break;
        }
    }
    if hung.is_none() {
        worker.join().map_err(|payload| {
            format!(
                "the fuzzing thread panicked: {}",
                payload_text(payload.as_ref())
            )
        })?;
    }
    Ok(Stats {
        seeds,
        inputs: hung.unwrap_or_else(|| progress.finished.load(Ordering::Relaxed)),
        panicked: progress.panicked.load(Ordering::Relaxed),
        hung: hung.is_some(),
        elapsed: start.elapsed(),
    })
}

/// The worker thread: runs the inputs `build` writes, in order, until the
/// budget runs out once the `seeds` first inputs have run, or the watchdog
/// says stop. Sends the first input that panics at each location.
fn work(
    run: fn(&[u8]),
    build: &dyn Fn(u64, &mut Vec<u8>),
    seeds: u64,
    deadline: Option<Instant>,
    limit: Option<u64>,
    progress: &Progress,
    events: &Sender<Finding>,
) {
    let mut locations = BTreeSet::new();
    let mut index = 0;
    while !progress.stop.load(Ordering::Relaxed) {
        let spent = limit.is_some_and(|limit| index >= limit)
            || deadline.is_some_and(|deadline| Instant::now() >= deadline);
        if index >= seeds && spent {
            return;
        }
        // In `INPUT`, where the panic hook can report it.
        INPUT.with_borrow_mut(|input| build(index, input));
        INDEX.set(index);
        if let Err(Caught { location, message }) = INPUT.with_borrow(|input| attempt(run, input)) {
            progress.panicked.fetch_add(1, Ordering::Relaxed);
            if locations.insert(location.clone()) {
                // Only a watchdog that gave up on this run stops listening.
                let _ = events.send(Finding {
                    kind: Kind::Panic { location, message },
                    index,
                    input: INPUT.with_borrow(Vec::clone),
                });
            }
        }
        index += 1;
        progress.finished.store(index, Ordering::Relaxed);
    }
}

/// A caught panic: where it happened, and what it said.
struct Caught {
    location: String,
    message: String,
}

impl Caught {
    fn of(info: &PanicHookInfo<'_>) -> Caught {
        let location = info.location().map_or_else(
            || UNKNOWN_LOCATION.to_owned(),
            |at| format!("{}:{}:{}", at.file(), at.line(), at.column()),
        );
        Caught {
            location,
            message: cut(info.payload_as_str().unwrap_or(NOT_A_STRING)),
        }
    }
}

/// What a panic whose payload is not a string said.
const NOT_A_STRING: &str = "a payload that is not a string";

thread_local! {
    /// Set while this thread runs a fuzz input: the panic hook then keeps
    /// its panics in `CAUGHT` instead of printing them.
    static CATCHING: Cell<bool> = const { Cell::new(false) };
    /// The panics the hook kept while this thread ran its input.
    static CAUGHT: RefCell<Vec<Caught>> = const { RefCell::new(Vec::new()) };
    /// On a worker thread: its target, and the input it runs and that
    /// input's index, for the panic hook to report.
    static TARGET: Cell<&'static str> = const { Cell::new("") };
    static INPUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    static INDEX: Cell<u64> = const { Cell::new(0) };
}

/// Installs, once, a panic hook that keeps the panics of a thread running
/// a fuzz input in `CAUGHT`, and leaves every other panic to the hook
/// before it.
fn install_hook() {
    static INSTALLED: Once = Once::new();
    INSTALLED.call_once(|| {
        let previous = panic::take_hook();
        panic::set_hook(Box::new(move |info| {
            if !CATCHING.try_with(Cell::get).unwrap_or(false) {
                return previous(info);
            }
            let earlier = CAUGHT.try_with(|caught| {
                caught.try_borrow_mut().map(|mut caught| {
                    caught.push(Caught::of(info));
                    caught.len() - 1
                })
            });
            match earlier {
                // The input's first panic: `attempt` catches it.
                Ok(Ok(0)) => {}
                // A panic while the first unwinds, in a destructor: the
                // process aborts, and nothing catches either (the engine
                // never catches its own panics). Say what ran while there
                // is a process to say it, then print as usual.
                Ok(Ok(1)) => {
                    report_abort();
                    previous(info);
                }
                _ => previous(info),
            }
        }));
    });
}

/// Reports on standard error the input this worker thread is running and
/// its first panic, before the abort a second panic brings.
fn report_abort() {
    let target = TARGET.try_with(Cell::get).unwrap_or_default();
    let index = INDEX.try_with(Cell::get).unwrap_or_default();
    let input = INPUT
        .try_with(|input| input.try_borrow().map(|input| shown(&input)).ok())
        .ok()
        .flatten()
        .unwrap_or_else(|| "bytes unknown".to_owned());
    let first = CAUGHT
        .try_with(|caught| {
            let caught = caught.try_borrow().ok()?;
            caught
                .first()
                .map(|first| (first.location.clone(), first.message.clone()))
        })
        .ok()
        .flatten();
    let (location, message) = first.unwrap_or_default();
    let mut err = io::stderr();
    let _ = writeln!(
        err,
        "fuzz-smoke: {target}: panic at {location}: input {index} ({input}), not saved: \
         it panicked again before that panic was caught, which aborts the run"
    );
    for line in message.lines() {
        let _ = writeln!(err, "fuzz-smoke: {target}:   {line}");
    }
}

/// Runs `run` on `input`, turning a panic into where it happened and what
/// it said.
fn attempt(run: fn(&[u8]), input: &[u8]) -> Result<(), Caught> {
    CATCHING.set(true);
    let caught = match panic::catch_unwind(AssertUnwindSafe(|| run(input))) {
        Ok(()) => None,
        Err(payload) => {
            // Taken before the payload's drop can add one. The hook does
            // not run for `resume_unwind`.
            let caught = CAUGHT.with_borrow_mut(Vec::pop).unwrap_or_else(|| Caught {
                location: UNKNOWN_LOCATION.to_owned(),
                message: cut(payload_text(payload.as_ref())),
            });
            // A payload whose own drop panics must not escape either: that
            // second payload is leaked, not dropped.
            if let Err(inner) = panic::catch_unwind(AssertUnwindSafe(move || drop(payload))) {
                mem::forget(inner);
            }
            Some(caught)
        }
    };
    CAUGHT.with_borrow_mut(Vec::clear);
    CATCHING.set(false);
    caught.map_or(Ok(()), Err)
}

/// The text of a panic: `panic!` gives a `&str` or a `String`.
fn payload_text(payload: &(dyn Any + Send)) -> &str {
    if let Some(text) = payload.downcast_ref::<&str>() {
        text
    } else if let Some(text) = payload.downcast_ref::<String>() {
        text
    } else {
        NOT_A_STRING
    }
}

/// `text`, cut to `MAX_MESSAGE` characters.
fn cut(text: &str) -> String {
    match text.char_indices().nth(MAX_MESSAGE) {
        Some((end, _)) => format!("{} [cut]", &text[..end]),
        None => text.to_owned(),
    }
}

/// Saves `finding`'s input in `dir` as `panic-` and the 16 hex digits of
/// the FNV-1a hash of its location, or `hang-` and those of the input, so
/// a panic location keeps one file from run to run; returns the file's
/// name.
fn save(dir: &Path, finding: &Finding) -> io::Result<String> {
    let name = match &finding.kind {
        Kind::Panic { location, .. } => format!("panic-{:016x}", fnv1a64(location.as_bytes())),
        Kind::Hang => format!("hang-{:016x}", fnv1a64(&finding.input)),
    };
    fs::create_dir_all(dir)?;
    fs::write(dir.join(&name), &finding.input)?;
    Ok(name)
}

/// The lines that report `finding`, with where it was saved, or why it
/// was not.
fn describe(finding: &Finding, saved: io::Result<String>, hang: Duration) -> Vec<String> {
    let what = match &finding.kind {
        Kind::Panic { location, .. } => format!("panic at {location}"),
        Kind::Hang => format!("hang, still running after {hang:?}"),
    };
    let saved = match saved {
        Ok(path) => format!("saved as {path}"),
        Err(e) => format!("not saved: {e}"),
    };
    let mut lines = vec![format!(
        "{what}: input {} ({}), {saved}",
        finding.index,
        shown(&finding.input)
    )];
    if let Kind::Panic { message, .. } = &finding.kind {
        lines.extend(message.lines().map(|line| format!("  {line}")));
    }
    lines
}

/// `input`'s length, and its first `SHOWN` bytes in hex.
fn shown(input: &[u8]) -> String {
    let hex: Vec<String> = input
        .iter()
        .take(SHOWN)
        .map(|b| format!("{b:02x}"))
        .collect();
    let more = if input.len() > SHOWN { " ..." } else { "" };
    match input.len() {
        0 => "0 bytes".to_owned(),
        len => format!("{}: {}{more}", count(len as u64, "byte"), hex.join(" ")),
    }
}

/// `n` and `noun`, plural unless `n` is 1.
fn count(n: u64, noun: &str) -> String {
    if n == 1 {
        format!("1 {noun}")
    } else {
        format!("{n} {noun}s")
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Budget, Build, DEFAULT_SECS, Finding, HANG, Kind, MAX_MESSAGE, Options, Stats, cut,
        describe, fuzz, fuzz_inputs, limits, load_seeds, parse, run_to, shown,
    };
    use crate::fixture::Fixture;
    use crate::fuzz_targets::Target;
    use crate::mutate::Inputs;
    use hardshell_core::hash::fnv1a64;
    use std::collections::BTreeSet;
    use std::env;
    use std::fs;
    use std::io;
    use std::panic;
    use std::process::Command;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::thread;
    use std::time::Duration;

    fn clean(_: &[u8]) {}

    fn magic(input: &[u8]) {
        if input.contains(&0xA5) {
            panic!("the magic byte");
        }
    }

    fn two_magics(input: &[u8]) {
        if input.contains(&0xA5) {
            panic!("A5");
        }
        if input.contains(&0x5A) {
            panic!("5A");
        }
    }

    /// Two panics on one line, told apart by their columns.
    #[rustfmt::skip]
    fn one_line(input: &[u8]) { if input.contains(&0xA5) { panic!("A5") } else if input.contains(&0x5A) { panic!("5A") } }

    /// A hang the test watchdog sees long before it ends.
    fn slow(input: &[u8]) {
        if input.contains(&0xA5) {
            thread::sleep(Duration::from_secs(3));
        }
    }

    /// The calls of `finite_hang`, which one test alone runs.
    static FINITE_CALLS: AtomicU64 = AtomicU64::new(0);

    /// A hang that ends soon after the test watchdog sees it.
    fn finite_hang(input: &[u8]) {
        FINITE_CALLS.fetch_add(1, Ordering::SeqCst);
        if input == [0xA5] {
            thread::sleep(SHORT_HANG * 2);
        }
    }

    /// Slower than the test watchdog's polls, well within its timeout.
    fn sleepy(input: &[u8]) {
        if input.contains(&0xA5) {
            thread::sleep(SHORT_HANG / 5);
        }
    }

    /// A panic payload whose drop panics.
    struct Bomb;

    impl Drop for Bomb {
        fn drop(&mut self) {
            panic!("the payload's drop");
        }
    }

    fn bomb(input: &[u8]) {
        if input.contains(&0xA5) {
            panic::panic_any(Bomb);
        }
    }

    /// What the child run fuzzes: panics caught as usual on 5A, and on B0
    /// with a payload whose drop panics; on A5, a second panic as it
    /// unwinds from the first, which aborts the process.
    fn child_target(input: &[u8]) {
        struct Guard;
        impl Drop for Guard {
            fn drop(&mut self) {
                panic!("the guard's drop");
            }
        }
        match input.first() {
            Some(0x5A) => panic!("a caught panic"),
            Some(0xB0) => panic::panic_any(Bomb),
            Some(0xA5) => {
                let _guard = Guard;
                panic!("the first panic");
            }
            _ => {}
        }
    }

    /// Passes inputs of up to 4 bytes alone.
    fn four_at_most(input: &[u8]) {
        assert!(input.len() <= 4, "{} bytes", input.len());
    }

    fn boom(input: &[u8]) {
        assert_ne!(input, b"boom", "a seed panics");
    }

    fn resumed(input: &[u8]) {
        if input.contains(&0xA5) {
            panic::resume_unwind(Box::new(5));
        }
    }

    fn target(name: &'static str, run: fn(&[u8])) -> Target {
        Target {
            name,
            max_len: 64,
            run,
        }
    }

    /// The test watchdog's timeout: short, but far past a busy machine's
    /// pauses.
    const SHORT_HANG: Duration = Duration::from_millis(300);

    /// At most `inputs` inputs, however long they take.
    fn inputs(inputs: u64) -> Budget {
        Budget {
            time: Duration::MAX,
            inputs: Some(inputs),
        }
    }

    /// What fuzzing `run` from `seeds` found, and its stats.
    fn fuzzed(
        run: fn(&[u8]),
        seeds: &[&[u8]],
        seed: u64,
        budget: Budget,
        hang: Duration,
    ) -> (Vec<Finding>, Stats) {
        let mut found = Vec::new();
        let seeds = seeds.iter().map(|seed| seed.to_vec()).collect();
        let stats = fuzz(
            &target("t", run),
            seeds,
            seed,
            budget,
            hang,
            &mut |finding| {
                found.push(finding);
            },
        )
        .unwrap();
        (found, stats)
    }

    fn location(finding: &Finding) -> &str {
        match &finding.kind {
            Kind::Panic { location, .. } => location,
            Kind::Hang => panic!("a hang: {finding:?}"),
        }
    }

    fn message(finding: &Finding) -> &str {
        match &finding.kind {
            Kind::Panic { message, .. } => message,
            Kind::Hang => panic!("a hang: {finding:?}"),
        }
    }

    #[test]
    fn a_target_that_panics_on_a_magic_byte_is_found() {
        let (found, stats) = fuzzed(magic, &[&[0; 8]], 1, inputs(100_000), HANG);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(
            location(&found[0]).starts_with(&format!("{}:", file!())),
            "{found:?}"
        );
        assert_eq!(message(&found[0]), "the magic byte");
        assert!(found[0].input.contains(&0xA5), "{found:?}");
        assert!(found[0].index > 0, "the seed itself passes");
        // Many inputs panicked there: one finding.
        assert!(stats.panicked > 10, "{stats:?}");
        assert_eq!((stats.seeds, stats.inputs, stats.hung), (1, 100_000, false));
    }

    #[test]
    fn each_panic_location_is_one_finding() {
        let (found, stats) = fuzzed(two_magics, &[&[0; 8]], 1, inputs(100_000), HANG);
        let messages: BTreeSet<&str> = found.iter().map(message).collect();
        assert_eq!(messages, BTreeSet::from(["5A", "A5"]), "{found:?}");
        assert_ne!(location(&found[0]), location(&found[1]));
        assert!(stats.panicked > 20, "{stats:?}");

        let (found, _) = fuzzed(one_line, &[&[0; 8]], 1, inputs(100_000), HANG);
        let messages: BTreeSet<&str> = found.iter().map(message).collect();
        assert_eq!(messages, BTreeSet::from(["5A", "A5"]), "{found:?}");
        let line = |finding| location(finding).rsplit_once(':').map(|(line, _)| line);
        assert_eq!(line(&found[0]), line(&found[1]), "{found:?}");
    }

    #[test]
    fn the_seed_picks_the_inputs() {
        let first = |seed| {
            let (found, _) = fuzzed(magic, &[&[0; 8]], seed, inputs(100_000), HANG);
            (found[0].index, found[0].input.clone())
        };
        assert_eq!(first(1), first(1));
        assert_ne!(first(1), first(2));
    }

    #[test]
    fn a_clean_target_finds_nothing() {
        let (found, stats) = fuzzed(clean, &[b"", b"abc"], 1, inputs(10_000), HANG);
        assert!(found.is_empty(), "{found:?}");
        assert_eq!(
            (stats.seeds, stats.inputs, stats.panicked, stats.hung),
            (2, 10_000, 0, false)
        );
    }

    #[test]
    fn the_seeds_run_even_when_the_time_is_up() {
        let budget = Budget {
            time: Duration::ZERO,
            inputs: None,
        };
        let (found, stats) = fuzzed(boom, &[b"one", b"boom", b"two"], 1, budget, HANG);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(
            (found[0].index, found[0].input.as_slice()),
            (1, &b"boom"[..])
        );
        assert_eq!(stats.inputs, 3);
    }

    #[test]
    fn an_input_that_runs_too_long_is_a_hang() {
        // A seed: the first input hangs.
        let (found, stats) = fuzzed(slow, &[&[0xA5]], 1, inputs(1_000), SHORT_HANG);
        let hang = Finding {
            kind: Kind::Hang,
            index: 0,
            input: vec![0xA5],
        };
        assert_eq!(found, [hang]);
        assert!(stats.hung && stats.inputs == 0, "{stats:?}");
        // Without waiting for the stuck input, which sleeps 3 s.
        assert!(stats.elapsed < Duration::from_secs(2), "{stats:?}");
        // A mutated input: the watchdog rebuilds the one the worker is in.
        let (found, stats) = fuzzed(slow, &[&[0; 8]], 1, inputs(1_000_000), SHORT_HANG);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].kind, Kind::Hang);
        assert!(found[0].input.contains(&0xA5), "{found:?}");
        assert!(
            found[0].index > 0 && found[0].index == stats.inputs,
            "{stats:?}"
        );
        assert!(stats.hung);
        assert!(
            stats.summary().ends_with("; stopped by a hang"),
            "{stats:?}"
        );
    }

    #[test]
    fn a_hang_in_the_seeds_says_how_many_ran() {
        let (found, stats) = fuzzed(slow, &[b"a", &[0xA5], b"c"], 1, inputs(10), SHORT_HANG);
        assert_eq!((found.len(), found[0].index), (1, 1), "{found:?}");
        let summary = stats.summary();
        assert!(
            summary.starts_with("1 of 3 seeds, then 0 mutated inputs in "),
            "{summary}"
        );
        // Seen soon after the timeout: the watchdog looks often.
        assert!(stats.elapsed < SHORT_HANG * 7 / 4, "{stats:?}");
    }

    #[test]
    fn a_hang_ends_the_target_s_run() {
        let budget = Budget {
            time: Duration::MAX,
            inputs: None,
        };
        let (found, stats) = fuzzed(finite_hang, &[&[0xA5], &[0]], 1, budget, SHORT_HANG);
        assert_eq!((found.len(), found[0].index), (1, 0), "{found:?}");
        assert!(stats.hung);
        // Once the stuck input ends, its thread runs no other.
        thread::sleep(SHORT_HANG * 4);
        assert_eq!(FINITE_CALLS.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn an_input_slower_than_a_poll_is_no_hang() {
        // Longer than the timeout, so a watchdog that timed the run, not
        // the input, would see one.
        let budget = Budget {
            time: SHORT_HANG * 5,
            inputs: None,
        };
        let (found, stats) = fuzzed(sleepy, &[&[0xA5]], 1, budget, SHORT_HANG);
        assert!(found.is_empty() && !stats.hung, "{found:?} {stats:?}");
        assert!(stats.inputs > 5, "{stats:?}");
    }

    #[test]
    fn no_input_is_longer_than_the_target_s_limit() {
        let target = Target {
            name: "four",
            max_len: 4,
            run: four_at_most,
        };
        let mut found = Vec::new();
        let seeds = vec![vec![1, 2, 3], vec![4, 5, 6, 7]];
        let stats = fuzz(&target, seeds, 1, inputs(5_000), HANG, &mut |finding| {
            found.push(finding);
        })
        .unwrap();
        assert!(found.is_empty(), "{found:?}");
        assert_eq!(stats.inputs, 5_000);
    }

    #[test]
    fn a_payload_whose_drop_panics_is_still_a_finding() {
        let (found, stats) = fuzzed(bomb, &[b"x", &[0xA5], &[0xA5, 0]], 1, inputs(10), HANG);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(message(&found[0]), "a payload that is not a string");
        assert!(
            location(&found[0]).starts_with(&format!("{}:", file!())),
            "{found:?}"
        );
        assert_eq!(stats.inputs, 10);
    }

    #[test]
    fn a_panic_in_the_harness_is_an_error() {
        let run = |build: Build| {
            fuzz_inputs(
                &target("t", clean),
                1,
                build,
                inputs(100),
                HANG,
                &mut |_| {},
            )
            .err()
        };
        let formatted: Build = Arc::new(|index, out| {
            assert!(index < 3, "a harness bug at input {index}");
            out.clear();
        });
        assert_eq!(
            run(formatted).as_deref(),
            Some("the fuzzing thread panicked: a harness bug at input 3")
        );
        let literal: Build = Arc::new(|_, _| panic!("a literal harness bug"));
        assert_eq!(
            run(literal).as_deref(),
            Some("the fuzzing thread panicked: a literal harness bug")
        );
    }

    /// Set only in the child runs of `child_runs`, to the run's mode.
    const CHILD: &str = "HARDSHELL_XTASK_FUZZ_CHILD";

    /// In a child run, fuzzes as its mode says; outside one it passes at
    /// once. `abort`: caught panics, then a panic while unwinding, which
    /// aborts. `harness`: panics outside a fuzz input once the hook is in.
    #[test]
    fn child_runs() {
        match env::var(CHILD).as_deref() {
            Ok("abort") => {
                let seeds: [&[u8]; 5] = [b"x", &[0x5A], &[0xB0], &[0x5A, 1], &[0xA5]];
                fuzzed(child_target, &seeds, 1, inputs(10), HANG);
            }
            Ok("harness") => {
                fuzzed(magic, &[&[0xA5]], 1, inputs(1), HANG);
                let _ = thread::spawn(|| panic!("a panic outside a fuzz input")).join();
                let build: Build = Arc::new(|index, out| {
                    assert!(index < 2, "a harness bug after inputs ran");
                    out.clear();
                });
                let found = &mut |_| {};
                let _ = fuzz_inputs(&target("t", magic), 1, build, inputs(10), HANG, found);
            }
            _ => {}
        }
    }

    /// `child_runs` in a child process in `mode`: its standard error, and
    /// whether it succeeded.
    fn child(mode: &str) -> (String, bool) {
        let output = Command::new(env::current_exe().unwrap())
            .args(["fuzz_smoke::tests::child_runs", "--exact", "--nocapture"])
            .env(CHILD, mode)
            .env_remove("RUST_BACKTRACE")
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        (stderr, output.status.success())
    }

    #[test]
    fn a_panic_while_unwinding_is_reported_once_before_the_abort() {
        let (stderr, succeeded) = child("abort");
        assert!(!succeeded, "{stderr}");
        let reports: Vec<&str> = stderr
            .lines()
            .filter(|line| line.contains("which aborts the run"))
            .collect();
        let [report] = reports[..] else {
            panic!("{stderr}");
        };
        assert!(
            report.starts_with(&format!("fuzz-smoke: t: panic at {}:", file!())),
            "{stderr}"
        );
        assert!(
            report.ends_with(
                ": input 4 (1 byte: a5), not saved: it panicked again before that panic was \
                 caught, which aborts the run"
            ),
            "{stderr}"
        );
        assert_eq!(stderr.matches("the first panic").count(), 1, "{stderr}");
        let first = stderr.find("\nfuzz-smoke: t:   the first panic\n");
        let guard = stderr.find("the guard's drop");
        assert!(
            first.zip(guard).is_some_and(|(first, guard)| first < guard),
            "{stderr}"
        );
        // The hook before it prints the second panic and the abort's; the
        // panics caught before print nothing.
        assert!(
            stderr.contains("panic in a destructor during cleanup"),
            "{stderr}"
        );
        assert_eq!(stderr.matches("panicked at").count(), 2, "{stderr}");
        assert!(!stderr.contains("a caught panic"), "{stderr}");
        assert!(!stderr.contains("the payload's drop"), "{stderr}");
    }

    #[test]
    fn panics_outside_a_fuzz_input_print_as_usual() {
        let (stderr, succeeded) = child("harness");
        assert!(succeeded, "{stderr}");
        for message in [
            "a panic outside a fuzz input",
            "a harness bug after inputs ran",
        ] {
            assert_eq!(stderr.matches(message).count(), 1, "{stderr}");
        }
        assert_eq!(stderr.matches("panicked at").count(), 2, "{stderr}");
        assert!(!stderr.contains("the magic byte"), "{stderr}");
    }

    #[test]
    fn a_resumed_unwind_has_no_location() {
        let (found, _) = fuzzed(resumed, &[&[0xA5]], 1, inputs(1), HANG);
        assert_eq!(location(&found[0]), "an unknown location");
        assert_eq!(message(&found[0]), "a payload that is not a string");
    }

    /// `run_to` on `targets` in `fixture`: what it printed to standard
    /// output and standard error, and whether the run passed.
    fn ran(
        fixture: &Fixture,
        targets: &[Target],
        seed: Result<u64, String>,
        hang: Duration,
    ) -> (String, String, bool) {
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let passed = run_to(
            &mut out,
            &mut err,
            fixture.root(),
            targets,
            seed,
            inputs(20_000),
            hang,
        );
        let text = |bytes| String::from_utf8(bytes).unwrap();
        (text(out), text(err), passed)
    }

    #[test]
    fn a_run_passes_clean_and_fails_on_a_finding() {
        let fixture = Fixture::new("fuzz-smoke-run");
        for name in ["clean", "magic", "slow"] {
            fixture.write_bytes(&format!("fuzz/corpus/{name}/zeros"), &[0; 8]);
        }
        let (out, err, passed) = ran(&fixture, &[target("clean", clean)], Ok(7), HANG);
        assert!(passed, "{out}{err}");
        assert_eq!(err, "");
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 3, "{out}");
        assert_eq!(
            lines[0],
            "fuzz-smoke: seed 7; HARDSHELL_SEED=7 gives the same inputs"
        );
        let summary = "fuzz-smoke: clean: 1 seed, then 19999 mutated inputs in ";
        assert!(lines[1].starts_with(summary), "{out}");
        assert_eq!(lines[2], "fuzz-smoke: ok");
        assert!(!fixture.root().join("fuzz/crashes").exists());

        let targets = [target("clean", clean), target("magic", magic)];
        let (out, err, passed) = ran(&fixture, &targets, Ok(7), HANG);
        assert!(!passed);
        assert_eq!(err, "");
        let saved = saved_panics(&fixture, &out, "magic");
        let [(saved, input)] = &saved[..] else {
            panic!("{out}");
        };
        assert!(saved.starts_with("fuzz/crashes/magic/panic-"), "{saved}");
        assert!(input.contains(&0xA5), "{input:02x?}");
        assert!(
            out.contains("\nfuzz-smoke: magic:   the magic byte\n"),
            "{out}"
        );
        assert!(
            out.contains("\nfuzz-smoke: magic: 1 seed, then 19999 mutated inputs in "),
            "{out}"
        );
        assert!(out.ends_with("\nfuzz-smoke: FAILED, 1 finding\n"), "{out}");

        // Another seed's run keeps one file for the one location.
        let (_, _, passed) = ran(&fixture, &[target("magic", magic)], Ok(8), HANG);
        assert!(!passed);
        assert_eq!(files_in(&fixture, "fuzz/crashes/magic").len(), 1);

        // Two locations, two files.
        fixture.write_bytes("fuzz/corpus/two/zeros", &[0; 8]);
        let (out, _, _) = ran(&fixture, &[target("two", two_magics)], Ok(7), HANG);
        let mut saved = saved_panics(&fixture, &out, "two");
        saved.sort_by_key(|(_, input)| input.contains(&0xA5));
        let [(_, five_a), (_, a_five)] = &saved[..] else {
            panic!("{out}");
        };
        assert!(
            five_a.contains(&0x5A) && !five_a.contains(&0xA5),
            "{five_a:02x?}"
        );
        assert!(a_five.contains(&0xA5), "{a_five:02x?}");
        assert_eq!(files_in(&fixture, "fuzz/crashes/two").len(), 2);

        let (out, _, passed) = ran(&fixture, &[target("slow", slow)], Ok(7), SHORT_HANG);
        assert!(!passed);
        let hang = out
            .lines()
            .find(|line| line.starts_with("fuzz-smoke: slow: hang, still running after 300ms: "))
            .unwrap_or_else(|| panic!("{out}"));
        let (_, saved) = hang.rsplit_once(", saved as ").unwrap();
        let input = fs::read(fixture.root().join(saved)).unwrap();
        assert!(input.contains(&0xA5), "{input:02x?}");
        let name = format!("fuzz/crashes/slow/hang-{:016x}", fnv1a64(&input));
        assert_eq!(saved, name);
        assert_eq!(files_in(&fixture, "fuzz/crashes/slow").len(), 1);
        assert!(
            out.ends_with("; stopped by a hang\nfuzz-smoke: FAILED, 1 finding\n"),
            "{out}"
        );
    }

    /// The panic findings of `target` that `out` reports: where each was
    /// saved, named for its location, and the input saved there.
    fn saved_panics(fixture: &Fixture, out: &str, target: &str) -> Vec<(String, Vec<u8>)> {
        let start = format!("fuzz-smoke: {target}: panic at ");
        out.lines()
            .filter_map(|line| line.strip_prefix(&start))
            .map(|line| {
                let (location, _) = line.split_once(": input ").unwrap();
                let (_, saved) = line.rsplit_once(", saved as ").unwrap();
                let name = format!(
                    "fuzz/crashes/{target}/panic-{:016x}",
                    fnv1a64(location.as_bytes())
                );
                assert_eq!(saved, name, "{line}");
                (name, fs::read(fixture.root().join(saved)).unwrap())
            })
            .collect()
    }

    /// The names in `dir` of `fixture`.
    fn files_in(fixture: &Fixture, dir: &str) -> Vec<String> {
        fs::read_dir(fixture.root().join(dir))
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect()
    }

    #[test]
    fn the_seed_and_the_target_pick_the_inputs() {
        let fixture = Fixture::new("fuzz-smoke-streams");
        for name in ["magic", "other"] {
            fixture.write_bytes(&format!("fuzz/corpus/{name}/zeros"), &[0; 8]);
        }
        // The index and the saved input of a run's first finding.
        let first = |name: &'static str, seed| {
            let (out, _, _) = ran(&fixture, &[target(name, magic)], Ok(seed), HANG);
            let line = out
                .lines()
                .find(|line| line.contains(": panic at "))
                .unwrap_or_else(|| panic!("{out}"));
            let index = line
                .split(": input ")
                .nth(1)
                .and_then(|rest| rest.split(' ').next());
            let index: u64 = index.unwrap().parse().unwrap();
            let (_, saved) = line.rsplit_once(", saved as ").unwrap();
            (index, fs::read(fixture.root().join(saved)).unwrap())
        };
        let (index, input) = first("magic", 7);
        // Input `i` of target `t` draws from `seed ^ fnv1a64(t)`.
        let mut expected = Vec::new();
        Inputs::new(&[vec![0; 8]], 7 ^ fnv1a64(b"magic"), 64).build(index, &mut expected);
        assert_eq!(input, expected);
        assert_eq!(first("magic", 7), (index, input.clone()));
        assert_ne!(first("magic", 8), (index, input.clone()));
        assert_ne!(first("other", 7), (index, input));
    }

    #[test]
    fn a_run_that_cannot_start_fails() {
        let fixture = Fixture::new("fuzz-smoke-cannot");
        let bad_seed = Err("HARDSHELL_SEED is \"x\": expected a u64".to_owned());
        let (out, err, passed) = ran(&fixture, &[target("clean", clean)], bad_seed, HANG);
        assert!(!passed);
        assert_eq!(out, "");
        assert_eq!(
            err,
            "fuzz-smoke: could not run: HARDSHELL_SEED is \"x\": expected a u64\n"
        );

        fixture.write("fuzz/corpus/clean/a", "");
        let targets = [target("absent", clean), target("clean", clean)];
        let (out, err, passed) = ran(&fixture, &targets, Ok(7), HANG);
        assert!(!passed);
        assert!(
            err.starts_with("fuzz-smoke: absent: could not run: cannot read fuzz/corpus/absent: "),
            "{err}"
        );
        assert!(out.contains("\nfuzz-smoke: clean: 1 seed, then "), "{out}");
        assert!(
            out.ends_with("\nfuzz-smoke: FAILED, 1 target could not run\n"),
            "{out}"
        );
    }

    #[test]
    fn seeds_are_the_files_in_the_corpus_in_name_order() {
        let fixture = Fixture::new("fuzz-smoke-seeds");
        let load = |name| load_seeds(fixture.root(), &target(name, clean));
        fixture.write("fuzz/corpus/t/b", "B");
        fixture.write("fuzz/corpus/t/a", "A");
        fixture.write("fuzz/corpus/t/c", "");
        fixture.write("fuzz/corpus/t/sub/d", "D");
        fixture.write("fuzz/corpus/t/.DS_Store", "not a seed");
        assert_eq!(load("t"), Ok(vec![b"A".to_vec(), b"B".to_vec(), vec![]]));
        fixture.write("fuzz/corpus/hidden/.keep", "");
        assert_eq!(
            load("hidden"),
            Err("fuzz/corpus/hidden has no seeds".to_owned())
        );

        fs::create_dir_all(fixture.root().join("fuzz/corpus/empty/sub")).unwrap();
        assert_eq!(
            load("empty"),
            Err("fuzz/corpus/empty has no seeds".to_owned())
        );
        assert!(
            load("absent")
                .unwrap_err()
                .starts_with("cannot read fuzz/corpus/absent: ")
        );

        fixture.write_bytes("fuzz/corpus/long/at-limit", &[7; 64]);
        assert_eq!(load("long"), Ok(vec![vec![7; 64]]));
        fixture.write_bytes("fuzz/corpus/long/over", &[7; 65]);
        assert_eq!(
            load("long"),
            Err("fuzz/corpus/long/over is longer than the target's limit of 64 bytes".to_owned())
        );
    }

    #[test]
    fn options_parse() {
        let options = |secs, target| Ok(Options { secs, target });
        assert_eq!(parse(&[]), options(60, None));
        assert_eq!(parse(&["--secs", "5"]), options(5, None));
        assert_eq!(parse(&["--target", "varint"]), options(60, Some(0)));
        assert_eq!(
            parse(&["--target", "varint", "--secs", "0"]),
            options(0, Some(0))
        );
        assert_eq!(
            parse(&["--secs", "18446744073709551615"]),
            options(u64::MAX, None)
        );
        for bad in [
            &["--secs"][..],
            &["--secs", "x"],
            &["--secs", "+5"],
            &["--secs", "-1"],
            &["--secs", ""],
            &["--secs", "1.5"],
            &["--secs", "18446744073709551616"],
            &["--secs", "1", "--secs", "2"],
            &["--target"],
            &["--target", "nope"],
            &["--target", "Varint"],
            &["--target", "varint", "--target", "varint"],
            &["--verbose"],
            &["5"],
            &["--secs=5"],
            &["--secs", "5", "extra"],
        ] {
            assert!(parse(bad).is_err(), "{bad:?}");
        }
        assert_eq!(
            parse(&["--target", "nope"]),
            Err("unknown fuzz target `nope`; the targets are varint".to_owned())
        );
    }

    #[test]
    fn the_command_line_sets_the_time_and_the_hang_timeout() {
        let (budget, hang) = limits(&Options {
            secs: 5,
            target: None,
        });
        assert_eq!(
            (budget.time, budget.inputs, hang),
            (Duration::from_secs(5), None, HANG)
        );
    }

    #[test]
    fn the_documented_numbers() {
        assert_eq!(HANG, Duration::from_secs(2));
        assert_eq!(DEFAULT_SECS, 60);
    }

    #[test]
    fn a_finding_reports_its_input_and_message() {
        let finding = Finding {
            kind: Kind::Panic {
                location: "src/a.rs:1:2".to_owned(),
                message: "first\nsecond".to_owned(),
            },
            index: 9,
            input: vec![0xAB; 40],
        };
        let lines = describe(&finding, Ok("fuzz/crashes/t/panic-x".to_owned()), HANG);
        let bytes = format!("{} ...", ["ab"; 32].join(" "));
        assert_eq!(
            lines,
            [
                format!(
                    "panic at src/a.rs:1:2: input 9 (40 bytes: {bytes}), saved as fuzz/crashes/t/panic-x"
                ),
                "  first".to_owned(),
                "  second".to_owned(),
            ]
        );
        let hang = Finding {
            kind: Kind::Hang,
            index: 0,
            input: vec![],
        };
        let not_saved = Err(io::Error::other("disk full"));
        assert_eq!(
            describe(&hang, not_saved, HANG),
            ["hang, still running after 2s: input 0 (0 bytes), not saved: disk full"]
        );
        assert_eq!(shown(&[1]), "1 byte: 01");
    }

    #[test]
    fn a_long_message_is_cut() {
        let long = "é".repeat(MAX_MESSAGE + 1);
        assert_eq!(cut(&long), format!("{} [cut]", "é".repeat(MAX_MESSAGE)));
        assert_eq!(cut(&long[2..]), long[2..]);
    }
}
