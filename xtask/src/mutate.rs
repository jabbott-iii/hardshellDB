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

//! The inputs of a `cargo xtask fuzz-smoke` run: the seeds unchanged, then
//! mutated seeds (`docs/04-quality.md`, "Fuzz smoke").
//!
//! Input `i` is a function of the seeds, the run's seed and `i` alone, so
//! the watchdog can rebuild the input a hung thread is stuck in, and a
//! seed replays a run.

use hardshell_core::rng::Pcg32;

/// The integers a mutation writes (`docs/04-quality.md`).
const INTERESTING: [u64; 6] = [0, 1, 255, 65_535, u32::MAX as u64, u64::MAX];

/// The widths, in bytes, of the integers a mutation reads and writes, in
/// increasing order.
const WIDTHS: [usize; 4] = [1, 2, 4, 8];

/// What one mutation does to an input.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Mutation {
    /// Flips one bit.
    BitFlip,
    /// Inserts one random byte.
    Insert,
    /// Deletes one byte.
    Delete,
    /// Keeps the input up to a point, then a seed's bytes from a point on.
    Splice,
    /// Treats 1, 2, 4 or 8 bytes as a length and nudges it.
    Length,
    /// Writes an interesting integer over the bytes at a point, or inserts
    /// it there.
    Interesting,
}

/// Every mutation, each drawn as often as the others.
const MUTATIONS: [Mutation; 6] = [
    Mutation::BitFlip,
    Mutation::Insert,
    Mutation::Delete,
    Mutation::Splice,
    Mutation::Length,
    Mutation::Interesting,
];

/// The inputs of one target's run.
pub(crate) struct Inputs<'a> {
    seeds: &'a [Vec<u8>],
    seed: u64,
    max_len: usize,
}

impl Inputs<'_> {
    /// The inputs from `seeds`, mutated with draws from `seed`, none longer
    /// than `max_len`.
    pub(crate) fn new(seeds: &[Vec<u8>], seed: u64, max_len: usize) -> Inputs<'_> {
        Inputs {
            seeds,
            seed,
            max_len,
        }
    }

    /// Writes input `index` to `out`: seed `index` unchanged while there are
    /// seeds left, then a seed drawn from `Pcg32::new(seed, index)` with 1,
    /// 2, 4 or 8 mutations from the same stream. With no seeds every input
    /// is empty.
    pub(crate) fn build(&self, index: u64, out: &mut Vec<u8>) {
        out.clear();
        if let Some(seed) = usize::try_from(index)
            .ok()
            .and_then(|index| self.seeds.get(index))
        {
            out.extend_from_slice(seed);
            return;
        }
        if self.seeds.is_empty() {
            return;
        }
        let mut rng = Pcg32::new(self.seed, index);
        out.extend_from_slice(&self.seeds[rng.range(self.seeds.len())]);
        out.truncate(self.max_len);
        for _ in 0..1 << rng.range(4) {
            let mutation = MUTATIONS[rng.range(MUTATIONS.len())];
            apply(mutation, &mut rng, out, self.seeds);
            out.truncate(self.max_len);
        }
    }
}

/// Applies `mutation` to `input` with draws from `rng`, splicing from one
/// of `seeds` (not empty), and returns what it did: on an empty input, a
/// mutation that changes a byte inserts one instead.
pub(crate) fn apply(
    mutation: Mutation,
    rng: &mut Pcg32,
    input: &mut Vec<u8>,
    seeds: &[Vec<u8>],
) -> Mutation {
    let mutation = match mutation {
        Mutation::BitFlip | Mutation::Delete | Mutation::Length if input.is_empty() => {
            Mutation::Insert
        }
        mutation => mutation,
    };
    match mutation {
        Mutation::BitFlip => {
            let bit = rng.range(input.len() * 8);
            input[bit / 8] ^= 1 << (bit % 8);
        }
        Mutation::Insert => {
            let at = rng.range(input.len() + 1);
            // The low byte of a draw.
            input.insert(at, rng.next_u32() as u8);
        }
        Mutation::Delete => {
            input.remove(rng.range(input.len()));
        }
        Mutation::Splice => {
            let other = &seeds[rng.range(seeds.len())];
            let at = rng.range(input.len() + 1);
            let from = rng.range(other.len() + 1);
            input.truncate(at);
            input.extend_from_slice(&other[from..]);
        }
        Mutation::Length => {
            length(rng, input);
        }
        Mutation::Interesting => interesting(rng, input),
    }
    mutation
}

/// The bytes a length tweak treated as a length.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "only the tests look at where a tweak went")
)]
#[derive(Debug)]
struct Field {
    at: usize,
    width: usize,
    big_endian: bool,
}

/// Reads the bytes at a random place of `input` (not empty) as a length of
/// 1, 2, 4 or 8 bytes, in either byte order, and writes back a nearby one:
/// 1 to 16 more or less, the number of bytes after the field (what a length
/// usually counts) give or take one, or 0. Returns where it was.
fn length(rng: &mut Pcg32, input: &mut [u8]) -> Field {
    let fitting = WIDTHS.iter().filter(|&&width| width <= input.len()).count();
    let width = WIDTHS[rng.range(fitting)];
    let at = rng.range(input.len() - width + 1);
    let big_endian = rng.range(2) == 1;
    // Lossless: usize is 64 bits on every target (ADR-0001).
    let after = (input.len() - at - width) as u64;
    let field = &mut input[at..at + width];
    let value = read(field, big_endian);
    let delta = 1 + rng.range(16) as u64;
    let value = match rng.range(6) {
        0 => value.wrapping_add(delta),
        1 => value.wrapping_sub(delta),
        2 => after,
        3 => after + 1,
        4 => after.wrapping_sub(1),
        _ => 0,
    };
    write(field, big_endian, value);
    Field {
        at,
        width,
        big_endian,
    }
}

/// Writes one of `INTERESTING`, cut to 1, 2, 4 or 8 bytes in either byte
/// order, over the bytes at a random place of `input`, or inserts it there.
fn interesting(rng: &mut Pcg32, input: &mut Vec<u8>) {
    let value = INTERESTING[rng.range(INTERESTING.len())];
    let width = WIDTHS[rng.range(WIDTHS.len())];
    let mut bytes = [0; 8];
    write(&mut bytes[..width], rng.range(2) == 1, value);
    let at = rng.range(input.len() + 1);
    let overwrite = rng.range(2) == 0;
    match input.get_mut(at..at + width) {
        Some(field) if overwrite => field.copy_from_slice(&bytes[..width]),
        _ => {
            input.splice(at..at, bytes[..width].iter().copied());
        }
    }
}

/// `field` as an unsigned integer.
fn read(field: &[u8], big_endian: bool) -> u64 {
    let fold = |value: u64, &byte: &u8| (value << 8) | u64::from(byte);
    if big_endian {
        field.iter().fold(0, fold)
    } else {
        field.iter().rev().fold(0, fold)
    }
}

/// Writes the low bytes of `value` that fit in `field`.
fn write(field: &mut [u8], big_endian: bool, value: u64) {
    for (byte, low) in field.iter_mut().zip(value.to_le_bytes()) {
        *byte = low;
    }
    if big_endian {
        field.reverse();
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Field, INTERESTING, Inputs, MUTATIONS, Mutation, WIDTHS, apply, length, read, write,
    };
    use hardshell_core::rng::Pcg32;
    use hardshell_core::testing::check;
    use std::collections::BTreeSet;

    fn seeds() -> Vec<Vec<u8>> {
        vec![vec![], vec![0xAC, 0x02], b"a longer seed".to_vec()]
    }

    fn built(inputs: &Inputs<'_>, index: u64) -> Vec<u8> {
        let mut out = vec![0xEE; 3];
        inputs.build(index, &mut out);
        out
    }

    #[test]
    fn the_seeds_come_first_unchanged() {
        let seeds = seeds();
        let inputs = Inputs::new(&seeds, 7, 64);
        for (index, seed) in (0..).zip(&seeds) {
            assert_eq!(&built(&inputs, index), seed);
        }
    }

    #[test]
    fn an_input_is_a_function_of_the_seed_and_its_index() {
        let seeds = seeds();
        let (one, again) = (Inputs::new(&seeds, 7, 64), Inputs::new(&seeds, 7, 64));
        let other = Inputs::new(&seeds, 8, 64);
        let mut differs = 0;
        // Out of order, so nothing carries from one input to the next.
        for index in (3..200).rev() {
            assert_eq!(built(&one, index), built(&again, index), "{index}");
            assert_eq!(built(&one, index), built(&one, index), "{index}");
            differs += usize::from(built(&one, index) != built(&other, index));
        }
        assert!(differs > 150, "{differs} of 197 differ with another seed");
        let distinct: BTreeSet<Vec<u8>> = (3..200).map(|index| built(&one, index)).collect();
        assert!(distinct.len() > 150, "{} distinct", distinct.len());
    }

    #[test]
    fn inputs_stay_within_the_limit() {
        check(
            "mutated inputs stay within max_len",
            200,
            |rng| {
                let seeds: Vec<Vec<u8>> = (0..1 + rng.range(4))
                    .map(|_| {
                        let mut seed = vec![0; rng.range(12)];
                        rng.fill_bytes(&mut seed);
                        seed
                    })
                    .collect();
                (seeds, rng.next_u64(), rng.range(12))
            },
            |(seeds, seed, max_len)| {
                let inputs = Inputs::new(seeds, *seed, *max_len);
                let mut out = Vec::new();
                for index in seeds.len() as u64..seeds.len() as u64 + 200 {
                    inputs.build(index, &mut out);
                    if out.len() > *max_len {
                        return Err(format!("input {index} is {} bytes", out.len()));
                    }
                }
                Ok(())
            },
        );
    }

    #[test]
    fn no_seeds_make_empty_inputs() {
        let inputs = Inputs::new(&[], 7, 64);
        for index in 0..100 {
            assert_eq!(built(&inputs, index), Vec::<u8>::new());
        }
    }

    /// What `build` makes of input `index`, made again as documented: a
    /// seed drawn from `Pcg32::new(seed, index)`, then 1, 2, 4 or 8
    /// mutations, each drawn evenly, from the same stream. The input, the
    /// seed's index and the mutations.
    fn replayed(
        seeds: &[Vec<u8>],
        seed: u64,
        max_len: usize,
        index: u64,
    ) -> (Vec<u8>, usize, Vec<Mutation>) {
        let mut rng = Pcg32::new(seed, index);
        let pick = rng.range(seeds.len());
        let mut out = seeds[pick].clone();
        out.truncate(max_len);
        let count = 1 << rng.range(4);
        let drawn = (0..count)
            .map(|_| {
                let mutation = MUTATIONS[rng.range(MUTATIONS.len())];
                apply(mutation, &mut rng, &mut out, seeds);
                out.truncate(max_len);
                mutation
            })
            .collect();
        (out, pick, drawn)
    }

    #[test]
    fn a_mutated_input_is_a_drawn_seed_with_1_2_4_or_8_drawn_mutations() {
        let seeds = seeds();
        let inputs = Inputs::new(&seeds, 7, 16);
        let (mut picks, mut counts, mut drawn) =
            (BTreeSet::new(), BTreeSet::new(), BTreeSet::new());
        for index in 3..3_000 {
            let (out, pick, mutations) = replayed(&seeds, 7, 16, index);
            assert_eq!(built(&inputs, index), out, "{index}");
            picks.insert(pick);
            counts.insert(mutations.len());
            drawn.extend(mutations);
        }
        assert_eq!(picks, BTreeSet::from([0, 1, 2]));
        assert_eq!(counts, BTreeSet::from([1, 2, 4, 8]));
        let every = [
            Mutation::BitFlip,
            Mutation::Insert,
            Mutation::Delete,
            Mutation::Splice,
            Mutation::Length,
            Mutation::Interesting,
        ];
        // A new mutation breaks this match until it joins `every` too.
        for mutation in every {
            match mutation {
                Mutation::BitFlip
                | Mutation::Insert
                | Mutation::Delete
                | Mutation::Splice
                | Mutation::Length
                | Mutation::Interesting => {}
            }
        }
        assert_eq!(drawn, BTreeSet::from(every));
    }

    /// `apply(mutation)` on `input`, 2,000 times from different streams:
    /// what each did, and the result.
    fn applied(mutation: Mutation, input: &[u8]) -> Vec<(Mutation, Vec<u8>)> {
        let seeds = seeds();
        (0..2_000)
            .map(|stream| {
                let mut rng = Pcg32::new(1, stream);
                let mut out = input.to_vec();
                (apply(mutation, &mut rng, &mut out, &seeds), out)
            })
            .collect()
    }

    /// Ten distinct bytes, none of them 00, 01 or FF.
    const INPUT: &[u8] = &[0x10, 0x20, 0x30, 0x40, 0x50, 0x60, 0x70, 0x80, 0x90, 0xA0];

    /// The positions where `a` and `b`, of one length, differ.
    fn differing(a: &[u8], b: &[u8]) -> Vec<usize> {
        assert_eq!(a.len(), b.len());
        (0..a.len()).filter(|&i| a[i] != b[i]).collect()
    }

    /// Where `long` is `short` with `count` bytes inserted, if it is.
    fn inserted(short: &[u8], long: &[u8], count: usize) -> Option<usize> {
        if long.len() != short.len() + count {
            return None;
        }
        (0..=short.len()).find(|&at| long[..at] == short[..at] && long[at + count..] == short[at..])
    }

    #[test]
    fn a_bit_flip_flips_any_one_bit() {
        let mut flips = BTreeSet::new();
        for (did, out) in applied(Mutation::BitFlip, INPUT) {
            assert_eq!(did, Mutation::BitFlip);
            let flipped: u32 = INPUT
                .iter()
                .zip(&out)
                .map(|(a, b)| (a ^ b).count_ones())
                .sum();
            assert_eq!(flipped, 1, "{out:02x?}");
            let at = differing(INPUT, &out)[0];
            flips.insert((at, (INPUT[at] ^ out[at]).trailing_zeros()));
        }
        assert_eq!(flips.len(), INPUT.len() * 8, "every bit of every byte");
    }

    #[test]
    fn an_insert_adds_one_byte_and_a_delete_takes_one_anywhere() {
        let mut places = BTreeSet::new();
        for (did, out) in applied(Mutation::Insert, INPUT) {
            assert_eq!(did, Mutation::Insert);
            places.insert(inserted(INPUT, &out, 1).expect("one byte inserted"));
        }
        assert_eq!(
            places.len(),
            INPUT.len() + 1,
            "at every place, the end included"
        );
        let mut deleted = BTreeSet::new();
        for (did, out) in applied(Mutation::Delete, INPUT) {
            assert_eq!(did, Mutation::Delete);
            deleted.insert(inserted(&out, INPUT, 1).expect("one byte deleted"));
        }
        assert_eq!(deleted.len(), INPUT.len(), "every byte, the last included");
    }

    #[test]
    fn a_splice_joins_the_input_s_head_to_a_seed_s_tail() {
        // Of bytes `INPUT` lacks, so the joint shows.
        let seeds = [b"XYZW".to_vec(), b"jk".to_vec()];
        let (mut heads, mut tails) = (BTreeSet::new(), BTreeSet::new());
        for stream in 0..2_000 {
            let mut rng = Pcg32::new(1, stream);
            let mut out = INPUT.to_vec();
            let did = apply(Mutation::Splice, &mut rng, &mut out, &seeds);
            assert_eq!(did, Mutation::Splice);
            let head = out.iter().zip(INPUT).take_while(|(a, b)| a == b).count();
            let tail = &out[head..];
            let seed = seeds.iter().position(|seed| seed.ends_with(tail));
            let seed = seed.unwrap_or_else(|| panic!("{out:02x?}"));
            heads.insert(head);
            if !tail.is_empty() {
                tails.insert((seed, seeds[seed].len() - tail.len()));
            }
        }
        assert_eq!(
            heads,
            (0..=INPUT.len()).collect(),
            "every head, all of it included"
        );
        let every_tail = [(0, 0), (0, 1), (0, 2), (0, 3), (1, 0), (1, 1)];
        assert_eq!(
            tails,
            BTreeSet::from(every_tail),
            "every tail of every seed"
        );
    }

    /// What a length tweak at `field` of `input` could have written to
    /// make `out`: 0, the bytes after the field give or take one, or 1 to
    /// 16 more or less, each cut to the field's width, and read in the
    /// field's byte order.
    fn tweaks(input: &[u8], out: &[u8], field: &Field) -> Vec<(&'static str, i64)> {
        let (at, end) = (field.at, field.at + field.width);
        assert!(
            WIDTHS.contains(&field.width) && end <= input.len(),
            "{field:?}"
        );
        assert_eq!((&out[..at], &out[end..]), (&input[..at], &input[end..]));
        let old = read(&input[at..end], field.big_endian);
        let new = read(&out[at..end], field.big_endian);
        let after = (input.len() - end) as u64;
        let cut = |value: u64| match field.width {
            8 => value,
            width => value & ((1 << (8 * width)) - 1),
        };
        let mut tweaks = Vec::new();
        if new == 0 {
            tweaks.push(("0", 0));
        }
        for (offset, value) in [(-1, after.wrapping_sub(1)), (0, after), (1, after + 1)] {
            if new == cut(value) {
                tweaks.push(("after", offset));
            }
        }
        for delta in 1..=16 {
            if new == cut(old.wrapping_add(delta)) {
                tweaks.push(("more", delta as i64));
            }
            if new == cut(old.wrapping_sub(delta)) {
                tweaks.push(("less", delta as i64));
            }
        }
        tweaks
    }

    #[test]
    fn a_length_tweak_nudges_one_field() {
        let (mut starts, mut ends) = (BTreeSet::new(), BTreeSet::new());
        for stream in 0..2_000 {
            let mut rng = Pcg32::new(1, stream);
            let mut out = INPUT.to_vec();
            let field = length(&mut rng, &mut out);
            // Read in the byte order it claims.
            assert!(
                !tweaks(INPUT, &out, &field).is_empty(),
                "{out:02x?} {field:?}"
            );
            if field.at == 0 {
                starts.insert((field.width, field.big_endian));
            }
            if field.at + field.width == INPUT.len() {
                ends.insert((field.width, field.big_endian));
            }
        }
        let every_field: BTreeSet<(usize, bool)> = WIDTHS
            .into_iter()
            .flat_map(|width| [(width, false), (width, true)])
            .collect();
        assert_eq!(starts, every_field, "every field at the start");
        assert_eq!(ends, every_field, "every field at the end");

        // Bytes far from 0, from 16 and from its length, so that most
        // tweaks have one explanation alone.
        let uniform = [0x80; 10];
        let mut seen = BTreeSet::new();
        for stream in 0..4_000 {
            let mut rng = Pcg32::new(1, stream);
            let mut out = uniform.to_vec();
            let field = length(&mut rng, &mut out);
            if let [tweak] = tweaks(&uniform, &out, &field)[..] {
                seen.insert(tweak);
            }
        }
        let mut every_tweak = BTreeSet::from([("0", 0), ("after", -1), ("after", 0), ("after", 1)]);
        every_tweak.extend((1..=16).flat_map(|delta| [("more", delta), ("less", delta)]));
        assert_eq!(seen, every_tweak);

        // A one-byte input has room for a one-byte field alone.
        for (_, out) in applied(Mutation::Length, &[0x05]) {
            assert_eq!(out.len(), 1);
        }
    }

    #[test]
    fn an_interesting_integer_is_written_or_inserted() {
        assert_eq!(
            INTERESTING,
            [0, 1, 255, 65_535, u64::from(u32::MAX), u64::MAX]
        );
        // Each, cut to each width, in either byte order.
        let encodings: BTreeSet<Vec<u8>> = INTERESTING
            .iter()
            .flat_map(|&value| {
                WIDTHS.iter().flat_map(move |&width| {
                    [false, true].map(|big_endian| {
                        let mut field = vec![0; width];
                        write(&mut field, big_endian, value);
                        field
                    })
                })
            })
            .collect();
        // `INPUT` has none of the bytes an encoding has, so an overwrite
        // changes exactly the encoding's bytes.
        let (mut written, mut starts) = (BTreeSet::new(), BTreeSet::new());
        let (mut insertions, mut places) = (BTreeSet::new(), BTreeSet::new());
        for (did, out) in applied(Mutation::Interesting, INPUT) {
            assert_eq!(did, Mutation::Interesting);
            if out.len() == INPUT.len() {
                let positions = differing(INPUT, &out);
                let (first, last) = (positions[0], positions[positions.len() - 1]);
                let encoding = out[first..=last].to_vec();
                assert!(encodings.contains(&encoding), "{out:02x?}");
                written.insert(encoding);
                starts.insert(first);
            } else {
                let width = out.len() - INPUT.len();
                let at = inserted(INPUT, &out, width).expect("inserted at one place");
                insertions.insert(out[at..at + width].to_vec());
                places.insert(at);
            }
        }
        assert_eq!(written, encodings, "every encoding written over the input");
        assert_eq!(insertions, encodings, "every encoding inserted");
        assert_eq!(
            starts,
            (0..INPUT.len()).collect(),
            "written from every byte"
        );
        assert_eq!(
            places,
            (0..=INPUT.len()).collect(),
            "inserted at every place"
        );
    }

    #[test]
    fn a_mutation_that_changes_a_byte_inserts_one_into_an_empty_input() {
        for mutation in [Mutation::BitFlip, Mutation::Delete, Mutation::Length] {
            for (did, out) in applied(mutation, &[]) {
                assert_eq!(did, Mutation::Insert);
                assert_eq!(out.len(), 1);
            }
        }
        for (did, out) in applied(Mutation::Splice, &[]) {
            assert_eq!(did, Mutation::Splice);
            assert!(seeds().iter().any(|seed| seed.ends_with(&out)));
        }
        for (_, out) in applied(Mutation::Interesting, &[]) {
            assert!(WIDTHS.contains(&out.len()));
        }
    }

    #[test]
    fn integers_are_read_and_written_in_either_byte_order() {
        let mut field = [0; 4];
        write(&mut field, false, 0x1122_3344_5566);
        assert_eq!(field, [0x66, 0x55, 0x44, 0x33]);
        assert_eq!(read(&field, false), 0x3344_5566);
        write(&mut field, true, 0x1122_3344_5566);
        assert_eq!(field, [0x33, 0x44, 0x55, 0x66]);
        assert_eq!(read(&field, true), 0x3344_5566);
        assert_eq!(read(&[0xFF; 8], true), u64::MAX);
    }
}
