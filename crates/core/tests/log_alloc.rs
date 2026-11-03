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

//! Logging with no sink allocates nothing (roadmap M0.4), counted by this
//! binary's global allocator: the test-only `unsafe` that ADR-0017 allows.
//!
//! The binary holds one test, because it installs a sink part-way and a
//! sink, once installed, stays.

use hardshell_core::log::{self, debug, error, info, trace, warn};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};

thread_local! {
    /// Allocations made by this thread. Per thread, because the test
    /// harness's other threads allocate when they like.
    static ALLOCATIONS: Cell<u64> = const { Cell::new(0) };
}

/// Counts one allocation on this thread. Never allocates or panics: the
/// thread-local is a `const`-initialized `Cell` with no destructor, which
/// every target (ADR-0001) keeps in native thread-local storage.
fn count() {
    let _ = ALLOCATIONS.try_with(|n| n.set(n.get().wrapping_add(1)));
}

/// The allocations this thread has made so far.
fn allocations() -> u64 {
    ALLOCATIONS.with(Cell::get)
}

/// `System`, counting every allocation and reallocation.
struct Counting;

#[global_allocator]
static COUNTING: Counting = Counting;

#[allow(unsafe_code)] // ADR-0017: a test binary's counting allocator.
// SAFETY: each method passes its arguments unchanged to the same method of
// `System` and returns its result, so it keeps `GlobalAlloc`'s contract
// exactly as `System` does. `count` neither allocates nor unwinds.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count();
        // SAFETY: the caller upholds `alloc`'s contract, which is `System`'s.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        count();
        // SAFETY: the caller upholds `alloc_zeroed`'s contract, which is
        // `System`'s.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        count();
        // SAFETY: `ptr` came from this allocator, so from `System`, with
        // `layout`; the caller upholds the rest of `realloc`'s contract.
        unsafe { System.realloc(ptr, layout, new_size) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: `ptr` came from this allocator, so from `System`, with
        // `layout`.
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[test]
fn logging_allocates_nothing_without_a_sink_or_with_one_that_formats_nothing() {
    // The counter sees this thread's allocations.
    let before = allocations();
    drop(black_box(vec![0u8; 16]));
    let mut grown = black_box(Vec::<u8>::with_capacity(1));
    grown.extend_from_slice(&[1, 2, 3]);
    drop(black_box(grown));
    assert_eq!(allocations() - before, 3, "a Vec, another, and its growth");

    // No sink: no argument is evaluated, though each would allocate.
    let evaluated = Cell::new(0u32);
    let costly = || {
        evaluated.set(evaluated.get() + 1);
        String::from("costly")
    };
    let before = allocations();
    for page in 0..1_000u32 {
        error!("page {page}: {}", costly());
        warn!("{}", costly());
        info!("{:?}", vec![costly()]);
        debug!("{}", format!("{page}"));
        trace!("no arguments");
    }
    let without_sink = allocations() - before;
    assert_eq!(without_sink, 0, "allocations with no sink");
    assert_eq!(evaluated.get(), 0, "arguments evaluated with no sink");

    // A sink that formats nothing: building and passing the record
    // allocates nothing either.
    static RECORDS: AtomicU64 = AtomicU64::new(0);
    let installed = log::set_sink(Box::new(|_| {
        RECORDS.fetch_add(1, Ordering::Relaxed);
    }));
    assert!(installed.is_ok());
    let before = allocations();
    for page in 0..1_000u32 {
        error!("page {page} is damaged");
        warn!("the WAL holds {} frames", page * 2);
        info!("{page:?}");
        debug!("{:>8}", page);
        trace!("no arguments");
    }
    let with_sink = allocations() - before;
    assert_eq!(with_sink, 0, "allocations with a sink that formats nothing");
    assert_eq!(RECORDS.load(Ordering::Relaxed), 5_000);
}
