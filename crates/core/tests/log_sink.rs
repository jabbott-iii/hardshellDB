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

//! The log macros with a sink installed. The sink is process-wide and set
//! once, so every test here shares one, installed by `capture`. A sink
//! runs on the thread that logs, so it keeps each thread's records apart.

use hardshell_core::log::{self, Level, LogRecord, debug, error, info, trace, warn};
use std::cell::{Cell, RefCell};
use std::sync::Once;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

type Captured = (Level, &'static str, String);

thread_local! {
    static RECORDS: RefCell<Vec<Captured>> = const { RefCell::new(Vec::new()) };
}

/// Installs the capturing sink if no test has yet, runs `body`, and
/// returns what `body` logged on this thread.
fn capture(body: impl FnOnce()) -> Vec<Captured> {
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| {
        let installed = log::set_sink(Box::new(|record: &LogRecord<'_>| {
            let captured = (
                record.level(),
                record.target(),
                record.message().to_string(),
            );
            RECORDS.with_borrow_mut(|records| records.push(captured));
        }));
        assert!(
            installed.is_ok(),
            "no other code in this binary installs a sink"
        );
    });
    RECORDS.with_borrow_mut(Vec::clear);
    body();
    RECORDS.with_borrow_mut(std::mem::take)
}

mod wal {
    pub(crate) fn replayed(frames: u32) {
        hardshell_core::log::info!("replayed {frames} frames");
    }
}

#[test]
fn each_macro_logs_its_level_target_and_message() {
    let records = capture(|| {
        let page = 7;
        error!("page {page} is damaged");
        warn!("the WAL holds {} frames", 2_000);
        info!("checkpoint done");
        debug!("{:>4}|{:<3}|", 5, "ab",);
        trace!("{0}-{0}-{last}", "x", last = 'y');
    });
    let target = "log_sink";
    assert_eq!(
        records,
        [
            (Level::Error, target, "page 7 is damaged".to_owned()),
            (Level::Warn, target, "the WAL holds 2000 frames".to_owned()),
            (Level::Info, target, "checkpoint done".to_owned()),
            (Level::Debug, target, "   5|ab |".to_owned()),
            (Level::Trace, target, "x-x-y".to_owned()),
        ]
    );
}

#[test]
fn the_target_is_the_calling_module() {
    assert_eq!(
        capture(|| wal::replayed(3)),
        [(Level::Info, "log_sink::wal", "replayed 3 frames".to_owned())]
    );
}

#[test]
fn arguments_are_evaluated_once() {
    let evaluated = Cell::new(0);
    let records = capture(|| {
        warn!("{}", {
            evaluated.set(evaluated.get() + 1);
            "costly"
        });
    });
    assert_eq!(evaluated.get(), 1);
    assert_eq!(records, [(Level::Warn, "log_sink", "costly".to_owned())]);
}

#[test]
fn the_sink_runs_on_the_logging_thread() {
    let mine = capture(|| {
        let theirs = thread::spawn(|| capture(|| error!("from another thread")));
        let theirs = theirs.join().expect("the logging thread does not panic");
        assert_eq!(
            theirs,
            [(Level::Error, "log_sink", "from another thread".to_owned())]
        );
    });
    assert_eq!(mine, []);
}

#[test]
fn a_second_sink_is_refused_and_the_first_stays() {
    static SECOND_CALLED: AtomicBool = AtomicBool::new(false);
    capture(|| {});
    let refused = log::set_sink(Box::new(|_| SECOND_CALLED.store(true, Ordering::Relaxed)));
    assert!(refused.is_err());
    assert_eq!(
        capture(|| warn!("still the first")),
        [(Level::Warn, "log_sink", "still the first".to_owned())]
    );
    assert!(!SECOND_CALLED.load(Ordering::Relaxed));
}
