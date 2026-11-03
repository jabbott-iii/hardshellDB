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

//! The property-test harness (`docs/04-quality.md`, "Property testing").
//!
//! [`check`] runs a property on inputs generated from seeded [`Pcg32`]s.
//! [`check_sized`] also passes the generator a size that grows over the
//! run, and shrinks a failure to the smallest size that still fails. A
//! failure panics with a report naming the case and the seed that replays
//! it.
//!
//! The seed is `HARDSHELL_SEED` (decimal, or `0x` and hex digits) when it
//! is set and not empty, else [`DEFAULT_SEED`], so a run without it, as in
//! CI, is the same every time. Case `i` of the property `name` draws from
//! `Pcg32::new(seed ^ fnv1a64(name), i)`: no case depends on another,
//! properties sharing a seed get different inputs, and [`check_sized`] can
//! rebuild one case at a smaller size.
//!
//! ```
//! use hardshell_core::testing::check;
//! use hardshell_core::varint::{decode_u64, encode_u64};
//!
//! check("varint round-trips", 1000, |rng| rng.next_u64(), |&value| {
//!     let mut bytes = Vec::new();
//!     encode_u64(value, &mut bytes);
//!     match decode_u64(&bytes) {
//!         Ok((decoded, used)) if decoded == value && used == bytes.len() => Ok(()),
//!         other => Err(format!("{bytes:02x?} decodes to {other:?}")),
//!     }
//! });
//! ```
//!
//! Generators and properties should be pure: a replay must see the same
//! inputs, and shrinking calls them again, even after one panicked.

use crate::hash::fnv1a64;
use crate::rng::Pcg32;
use std::any::Any;
use std::env;
use std::ffi::OsStr;
use std::fmt;
use std::mem;
use std::panic::{self, AssertUnwindSafe};

/// The seed when `HARDSHELL_SEED` is unset or empty.
pub const DEFAULT_SEED: u64 = 42;

/// The largest size [`check_sized`] passes to its generator.
pub const MAX_SIZE: u32 = 100;

/// The environment variable that sets the seed.
const SEED_VAR: &str = "HARDSHELL_SEED";

/// Runs `property` on `cases` inputs from `generate`, and panics at the
/// first failure: an `Err` from the property, or a panic in the generator,
/// the property, or the input's drop. The report names the property, the
/// case and the seed that replays it, then gives the input (its `Debug`
/// form, built only for a failure; a panic there is shown in its place)
/// and the error.
///
/// # Panics
///
/// When a case fails; when `cases` is 0; when `HARDSHELL_SEED` is set, not
/// empty, and not a `u64` in decimal or `0x` hex.
#[track_caller]
pub fn check<T: fmt::Debug>(
    name: &str,
    cases: u32,
    generate: impl Fn(&mut Pcg32) -> T,
    property: impl Fn(&T) -> Result<(), String>,
) {
    assert!(cases != 0, "property \"{name}\" needs at least 1 case");
    let seed = seed_from_env();
    if let Err(report) = run(seed, name, cases, false, &|rng, _| generate(rng), &property) {
        panic!("{report}");
    }
}

/// [`check`], with a size for the generator: from 0 to [`MAX_SIZE`],
/// growing evenly over the run. Case `i` of `n` has size
/// `i * MAX_SIZE / (n - 1)`, so the first case has size 0 and the last
/// [`MAX_SIZE`] (a single case has size 0). The generator decides what a
/// size means: a length, a depth, a magnitude.
///
/// A case that fails at size `s` is rebuilt from its own stream at sizes 0,
/// 1, and so on below `s`, and the report gives the smallest size that
/// fails, with that size's input and error. Failure need not grow with
/// size, so this scan of at most `s` attempts finds the smallest for that
/// case where a binary search could miss it.
///
/// # Panics
///
/// As [`check`].
#[track_caller]
pub fn check_sized<T: fmt::Debug>(
    name: &str,
    cases: u32,
    generate: impl Fn(&mut Pcg32, u32) -> T,
    property: impl Fn(&T) -> Result<(), String>,
) {
    assert!(cases != 0, "property \"{name}\" needs at least 1 case");
    let seed = seed_from_env();
    if let Err(report) = run(seed, name, cases, true, &generate, &property) {
        panic!("{report}");
    }
}

/// One failed case: what the report shows as its input (its `Debug` form,
/// or why there is none) and the error.
struct Failure {
    input: String,
    error: String,
}

/// Runs the cases; on a failure, shrinks it if `sized` and returns the
/// report.
fn run<T: fmt::Debug>(
    seed: u64,
    name: &str,
    cases: u32,
    sized: bool,
    generate: &dyn Fn(&mut Pcg32, u32) -> T,
    property: &dyn Fn(&T) -> Result<(), String>,
) -> Result<(), String> {
    let base = seed ^ fnv1a64(name.as_bytes());
    for case in 0..cases {
        let first = if sized { size_of_case(case, cases) } else { 0 };
        let Err(failure) = attempt(base, case, first, generate, property) else {
            continue;
        };
        let mut header = format!("property \"{name}\" failed at case {case} of {cases}");
        let mut failure = failure;
        if sized {
            let smaller = (0..first).find_map(|size| {
                attempt(base, case, size, generate, property)
                    .err()
                    .map(|failure| (size, failure))
            });
            match smaller {
                Some((size, smaller)) => {
                    failure = smaller;
                    header += &format!(", size {size} (shrunk from size {first})");
                }
                None => header += &format!(", size {first}"),
            }
        }
        return Err(format!(
            "{header}; HARDSHELL_SEED={seed} replays it\ninput: {}\nerror: {}",
            failure.input, failure.error
        ));
    }
    Ok(())
}

/// Builds case `case` at `size` from its own stream and checks it. A panic
/// anywhere in the case becomes its failure, so the report still names the
/// case and the seed.
fn attempt<T: fmt::Debug>(
    base: u64,
    case: u32,
    size: u32,
    generate: &dyn Fn(&mut Pcg32, u32) -> T,
    property: &dyn Fn(&T) -> Result<(), String>,
) -> Result<(), Failure> {
    let mut rng = Pcg32::new(base, u64::from(case));
    let input = match catch(|| generate(&mut rng, size)) {
        Ok(input) => input,
        Err(message) => {
            return Err(Failure {
                input: "none, the generator panicked".to_owned(),
                error: format!("the generator panicked: {message}"),
            });
        }
    };
    let error = match catch(|| property(&input)) {
        Ok(Ok(())) => None,
        Ok(Err(error)) => Some(error),
        Err(message) => Some(format!("the property panicked: {message}")),
    };
    // Shown only for a failure, so a passing case never formats.
    let shown = error.as_ref().map(|_| {
        catch(|| format!("{input:?}"))
            .unwrap_or_else(|message| format!("none, its Debug form panicked: {message}"))
    });
    let dropped = catch(move || drop(input));
    match (shown.zip(error), dropped) {
        // A failed case keeps its own error; a panicking drop is the
        // lesser news.
        (Some((input, error)), _) => Err(Failure { input, error }),
        (None, Err(message)) => Err(Failure {
            input: "none, dropping it panicked".to_owned(),
            error: format!("dropping the input panicked: {message}"),
        }),
        (None, Ok(())) => Ok(()),
    }
}

/// Runs `body`, turning a panic into its message.
fn catch<R>(body: impl FnOnce() -> R) -> Result<R, String> {
    panic::catch_unwind(AssertUnwindSafe(body)).map_err(|payload| {
        let message = panic_message(&*payload).to_owned();
        // A payload whose own drop panics must not escape either: that
        // second payload is leaked, not dropped.
        if let Err(inner) = panic::catch_unwind(AssertUnwindSafe(move || drop(payload))) {
            mem::forget(inner);
        }
        message
    })
}

/// The size of case `case` of `cases`: see [`check_sized`].
fn size_of_case(case: u32, cases: u32) -> u32 {
    if cases < 2 {
        return 0;
    }
    let size = u64::from(case) * u64::from(MAX_SIZE) / u64::from(cases - 1);
    // Lossless: `case < cases`, so `size <= MAX_SIZE`.
    size as u32
}

/// The text of a panic: `panic!` gives a `&str` or a `String`.
fn panic_message(payload: &(dyn Any + Send)) -> &str {
    if let Some(text) = payload.downcast_ref::<&str>() {
        text
    } else if let Some(text) = payload.downcast_ref::<String>() {
        text
    } else {
        "a payload that is not a string"
    }
}

/// The seed from `HARDSHELL_SEED`, or [`DEFAULT_SEED`].
#[track_caller]
fn seed_from_env() -> u64 {
    match seed_from(env::var_os(SEED_VAR).as_deref()) {
        Ok(seed) => seed,
        Err(message) => panic!("{message}"),
    }
}

/// The seed for a `HARDSHELL_SEED` of `value`, or the message for a
/// malformed one.
fn seed_from(value: Option<&OsStr>) -> Result<u64, String> {
    let Some(value) = value.filter(|value| !value.is_empty()) else {
        return Ok(DEFAULT_SEED);
    };
    value.to_str().and_then(parse_seed).ok_or_else(|| {
        format!("{SEED_VAR} is {value:?}: expected a u64 in decimal, or 0x and hex digits")
    })
}

/// A `u64` in decimal, or `0x` and hex digits; nothing else.
fn parse_seed(text: &str) -> Option<u64> {
    let (digits, radix) = match text.strip_prefix("0x") {
        Some(hex) => (hex, 16),
        None => (text, 10),
    };
    // `from_str_radix` also takes a leading `+`.
    if digits.is_empty() || !digits.chars().all(|c| c.is_digit(radix)) {
        return None;
    }
    u64::from_str_radix(digits, radix).ok()
}

#[cfg(test)]
mod tests {
    use super::{
        DEFAULT_SEED, MAX_SIZE, check, check_sized, parse_seed, run, seed_from, size_of_case,
    };
    use crate::hash::fnv1a64;
    use crate::rng::Pcg32;
    use std::cell::{Cell, RefCell};
    use std::ffi::OsStr;

    /// The first draw of each case's stream for the property `name`.
    fn first_draws(seed: u64, name: &str, cases: u32) -> Vec<u64> {
        (0..cases)
            .map(|case| Pcg32::new(seed ^ fnv1a64(name.as_bytes()), u64::from(case)).next_u64())
            .collect()
    }

    /// Runs a property that passes, and returns each call's size and first
    /// draw.
    fn calls(seed: u64, name: &str, cases: u32, sized: bool) -> Vec<(u32, u64)> {
        let seen = RefCell::new(Vec::new());
        let generate = |rng: &mut Pcg32, size| seen.borrow_mut().push((size, rng.next_u64()));
        assert_eq!(
            run(seed, name, cases, sized, &generate, &|_| Ok(())),
            Ok(())
        );
        seen.into_inner()
    }

    #[test]
    fn each_case_draws_once_from_its_own_stream() {
        let seen = calls(7, "p", 5, false);
        let sizes: Vec<u32> = seen.iter().map(|&(size, _)| size).collect();
        let draws: Vec<u64> = seen.iter().map(|&(_, draw)| draw).collect();
        assert_eq!(sizes, [0; 5]);
        assert_eq!(draws, first_draws(7, "p", 5));
        // The name and the seed both pick the streams.
        assert_eq!(calls(7, "q", 5, false), calls(7, "q", 5, false));
        assert_ne!(calls(7, "q", 5, false), seen);
        assert_ne!(calls(8, "p", 5, false), seen);
    }

    #[test]
    fn sizes_grow_evenly_from_zero_to_max_size() {
        let sizes = |cases| -> Vec<u32> {
            calls(1, "s", cases, true)
                .into_iter()
                .map(|(size, _)| size)
                .collect()
        };
        assert_eq!(sizes(1), [0]);
        assert_eq!(sizes(2), [0, MAX_SIZE]);
        assert_eq!(sizes(3), [0, 50, 100]);
        assert_eq!(sizes(101), (0..=100).collect::<Vec<u32>>());
        let many = sizes(1000);
        assert!(many.windows(2).all(|pair| pair[0] <= pair[1]));
        assert_eq!((many[0], many[999]), (0, MAX_SIZE));
        assert!((0..=MAX_SIZE).all(|size| many.contains(&size)));
        // The arithmetic holds at the largest run.
        assert_eq!(size_of_case(0, u32::MAX), 0);
        assert_eq!(size_of_case(u32::MAX - 1, u32::MAX), MAX_SIZE);
        assert_eq!(size_of_case(u32::MAX / 2, u32::MAX), MAX_SIZE / 2);
        assert_eq!(size_of_case(u32::MAX / 2 - 1, u32::MAX), MAX_SIZE / 2 - 1);
    }

    #[test]
    fn a_failure_reports_the_first_failing_case_and_stops() {
        let draws = first_draws(7, "fourth", 100);
        let case = draws.iter().position(|draw| draw % 4 == 0);
        let case = case.expect("some draw in 100 is a multiple of 4");
        let checked = RefCell::new(0);
        let report = run(
            7,
            "fourth",
            100,
            false,
            &|rng, _| rng.next_u64(),
            &|&draw| {
                *checked.borrow_mut() += 1;
                if draw % 4 == 0 {
                    Err(format!("{draw} is a multiple of 4"))
                } else {
                    Ok(())
                }
            },
        );
        assert_eq!(
            report,
            Err(format!(
                "property \"fourth\" failed at case {case} of 100; HARDSHELL_SEED=7 replays it\n\
                 input: {0}\nerror: {0} is a multiple of 4",
                draws[case]
            ))
        );
        assert_eq!(checked.into_inner(), case + 1);
    }

    #[test]
    fn the_same_seed_replays_the_same_report() {
        let report = |seed| {
            run(
                seed,
                "half",
                50,
                true,
                &|rng, size| (size, rng.next_u32()),
                &|&(_, draw)| {
                    if draw % 2 == 0 {
                        Err("even".to_owned())
                    } else {
                        Ok(())
                    }
                },
            )
        };
        assert!(report(3).is_err());
        assert_eq!(report(3), report(3));
        assert_ne!(report(3), report(4));
    }

    #[test]
    fn a_panicking_property_is_reported_with_its_message() {
        let report = |property: &dyn Fn(&u32) -> Result<(), String>| {
            run(1, "panics", 10, false, &|_, _| 5u32, property)
        };
        let header = "property \"panics\" failed at case 0 of 10; HARDSHELL_SEED=1 replays it";
        assert_eq!(
            report(&|&n| panic!("boom at {n}")),
            Err(format!(
                "{header}\ninput: 5\nerror: the property panicked: boom at 5"
            ))
        );
        assert_eq!(
            report(&|_| panic!("plain boom")),
            Err(format!(
                "{header}\ninput: 5\nerror: the property panicked: plain boom"
            ))
        );
        assert_eq!(
            report(&|_| std::panic::panic_any(5u8)),
            Err(format!(
                "{header}\ninput: 5\nerror: the property panicked: a payload that is not a string"
            ))
        );
    }

    #[test]
    fn a_panicking_generator_is_reported_without_an_input() {
        let report = run(
            1,
            "bad generator",
            10,
            false,
            // A generator bug: an empty range when the length drawn is 0.
            &|rng, _| {
                let len = rng.range(3);
                rng.range(len)
            },
            &|_| Ok(()),
        );
        let report = report.expect_err("some case of 10 draws a length of 0");
        assert!(report.starts_with("property \"bad generator\" failed at case "));
        assert!(report.ends_with(
            "; HARDSHELL_SEED=1 replays it\ninput: none, the generator panicked\n\
             error: the generator panicked: Pcg32::range needs a bound of at least 1"
        ));
    }

    #[test]
    fn check_sized_reports_the_smallest_failing_size() {
        // Sizes 0, 50 and 100: case 1 fails at 50 and shrinks to 7.
        let report = run(9, "big", 3, true, &|_, size| size, &|&size| {
            if size >= 7 {
                Err(format!("{size} is too big"))
            } else {
                Ok(())
            }
        });
        assert_eq!(
            report,
            Err(
                "property \"big\" failed at case 1 of 3, size 7 (shrunk from size 50); \
                 HARDSHELL_SEED=9 replays it\ninput: 7\nerror: 7 is too big"
                    .to_owned()
            )
        );
    }

    #[test]
    fn shrinking_scans_every_smaller_size_of_the_same_case() {
        // Failure that does not grow with size: only 13 and 50 fail.
        let report = run(9, "odd sizes", 3, true, &|_, size| size, &|&size| {
            if size == 13 || size == 50 {
                Err("no".to_owned())
            } else {
                Ok(())
            }
        });
        assert!(report.is_err_and(|r| r.contains("case 1 of 3, size 13 (shrunk from size 50);")));
        // Nothing smaller fails: the report keeps the first size.
        let report = run(9, "one size", 3, true, &|_, size| size, &|&size| {
            if size == 50 {
                Err("no".to_owned())
            } else {
                Ok(())
            }
        });
        assert!(report.is_err_and(|r| r.contains("case 1 of 3, size 50;")));
        // The smaller input comes from the failing case's stream.
        let base = 9 ^ fnv1a64(b"stream");
        let report = run(
            9,
            "stream",
            3,
            true,
            &|rng, size| (size, rng.next_u64()),
            &|&(size, _)| {
                if size >= 7 {
                    Err("no".to_owned())
                } else {
                    Ok(())
                }
            },
        );
        let draw = Pcg32::new(base, 1).next_u64();
        assert!(report.is_err_and(|r| r.contains(&format!("\ninput: (7, {draw})\n"))));
        // Both ends of the scan: size 0 (of case 1's stream, so case 0,
        // also size 0, passes) and size s - 1.
        let first_draw =
            |name: &str, case| Pcg32::new(9 ^ fnv1a64(name.as_bytes()), case).next_u64();
        assert_ne!(first_draw("ends", 0), first_draw("ends", 1));
        let case_1 = first_draw("ends", 1);
        let report = run(
            9,
            "ends",
            3,
            true,
            &|rng, size| (size, rng.next_u64()),
            &|&(size, draw)| {
                if size == 50 || (size == 0 && draw == case_1) {
                    Err("no".to_owned())
                } else {
                    Ok(())
                }
            },
        );
        assert!(report.is_err_and(|r| r.contains("case 1 of 3, size 0 (shrunk from size 50);")));
        let report = run(9, "top", 3, true, &|_, size| size, &|&size| {
            if size == 49 || size == 50 {
                Err("no".to_owned())
            } else {
                Ok(())
            }
        });
        assert!(report.is_err_and(|r| r.contains("case 1 of 3, size 49 (shrunk from size 50);")));
    }

    /// An input whose `Debug` form or drop can panic.
    struct Fragile {
        debug_panics: bool,
        drop_panics: bool,
    }

    impl std::fmt::Debug for Fragile {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            assert!(!self.debug_panics, "bad Debug");
            f.write_str("Fragile")
        }
    }

    impl Drop for Fragile {
        fn drop(&mut self) {
            assert!(!self.drop_panics, "bad drop");
        }
    }

    #[test]
    fn a_panicking_debug_form_or_drop_is_still_reported() {
        let report = |debug_panics, drop_panics, fails: bool| {
            run(
                1,
                "fragile",
                10,
                false,
                &|_, _| Fragile {
                    debug_panics,
                    drop_panics,
                },
                &|_| if fails { Err("no".to_owned()) } else { Ok(()) },
            )
        };
        let header = "property \"fragile\" failed at case 0 of 10; HARDSHELL_SEED=1 replays it";
        assert_eq!(
            report(true, false, true),
            Err(format!(
                "{header}\ninput: none, its Debug form panicked: bad Debug\nerror: no"
            ))
        );
        assert_eq!(
            report(false, true, false),
            Err(format!(
                "{header}\ninput: none, dropping it panicked\n\
                 error: dropping the input panicked: bad drop"
            ))
        );
        // A failed case keeps its own error.
        assert_eq!(
            report(false, true, true),
            Err(format!("{header}\ninput: Fragile\nerror: no"))
        );
    }

    thread_local! {
        static FORMATTED: Cell<u32> = const { Cell::new(0) };
    }

    /// An input that counts how often it is formatted.
    struct Counted;

    impl std::fmt::Debug for Counted {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            FORMATTED.with(|n| n.set(n.get() + 1));
            f.write_str("Counted")
        }
    }

    #[test]
    fn only_a_failing_case_is_formatted() {
        let formatted = |fails: bool| {
            FORMATTED.with(|n| n.set(0));
            let result = run(1, "counted", 10, false, &|_, _| Counted, &|_| {
                if fails { Err("no".to_owned()) } else { Ok(()) }
            });
            assert_eq!(result.is_err(), fails);
            FORMATTED.with(Cell::get)
        };
        assert_eq!(formatted(false), 0);
        assert_eq!(formatted(true), 1);
    }

    /// A panic payload whose own drop panics.
    struct Payload;

    impl Drop for Payload {
        fn drop(&mut self) {
            panic!("the payload's drop panicked");
        }
    }

    #[test]
    fn a_payload_whose_drop_panics_is_still_reported() {
        let report = run(1, "payload", 10, false, &|_, _| 5u32, &|_| {
            std::panic::panic_any(Payload)
        });
        assert_eq!(
            report,
            Err(
                "property \"payload\" failed at case 0 of 10; HARDSHELL_SEED=1 replays it\n\
                 input: 5\nerror: the property panicked: a payload that is not a string"
                    .to_owned()
            )
        );
    }

    #[test]
    fn check_and_check_sized_pass_quietly() {
        check("passes", 20, |rng| rng.next_u32(), |_| Ok(()));
        check_sized(
            "passes sized",
            20,
            |_, size| size,
            |&size| {
                if size <= MAX_SIZE {
                    Ok(())
                } else {
                    Err("too big".to_owned())
                }
            },
        );
    }

    #[test]
    #[should_panic(expected = "property \"never\" failed at case 0 of 10; HARDSHELL_SEED=")]
    fn check_panics_with_the_report() {
        check("never", 10, |rng| rng.next_u32(), |_| Err("no".to_owned()));
    }

    #[test]
    #[should_panic(expected = "property \"never sized\" failed at case 0 of 10, size 0;")]
    fn check_sized_panics_with_the_report() {
        check_sized("never sized", 10, |_, size| size, |_| Err("no".to_owned()));
    }

    #[test]
    #[should_panic(expected = "property \"empty\" needs at least 1 case")]
    fn check_needs_a_case() {
        check("empty", 0, |rng| rng.next_u32(), |_| Ok(()));
    }

    #[test]
    #[should_panic(expected = "property \"empty sized\" needs at least 1 case")]
    fn check_sized_needs_a_case() {
        check_sized("empty sized", 0, |_, size| size, |_| Ok(()));
    }

    #[test]
    fn seeds_are_decimal_or_hex_digits_only() {
        for (text, seed) in [
            ("0", Some(0)),
            ("42", Some(42)),
            ("007", Some(7)),
            ("18446744073709551615", Some(u64::MAX)),
            ("0x0", Some(0)),
            ("0x2a", Some(42)),
            ("0x2A", Some(42)),
            ("0X2A", None),
            ("0xffffffffffffffff", Some(u64::MAX)),
            ("18446744073709551616", None),
            ("0x10000000000000000", None),
            ("", None),
            ("0x", None),
            ("+5", None),
            ("-1", None),
            ("0x+1", None),
            (" 5", None),
            ("5 ", None),
            ("5_000", None),
            ("1e3", None),
            ("0b101", None),
            ("x2a", None),
            ("٣", None),
        ] {
            assert_eq!(parse_seed(text), seed, "{text:?}");
        }
    }

    #[test]
    fn an_unset_or_empty_seed_is_the_default_and_a_bad_one_an_error() {
        assert_eq!(seed_from(None), Ok(DEFAULT_SEED));
        assert_eq!(seed_from(Some(OsStr::new(""))), Ok(DEFAULT_SEED));
        assert_eq!(seed_from(Some(OsStr::new("0x10"))), Ok(16));
        assert_eq!(
            seed_from(Some(OsStr::new("12a"))),
            Err(
                "HARDSHELL_SEED is \"12a\": expected a u64 in decimal, or 0x and hex digits"
                    .to_owned()
            )
        );
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            let invalid = seed_from(Some(OsStr::from_bytes(b"4\xff2")));
            assert!(invalid.is_err_and(|e| e.starts_with("HARDSHELL_SEED is \"4\\xFF2\":")));
        }
    }
}
