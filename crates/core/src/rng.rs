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

//! [`Pcg32`], the seeded generator behind property tests, fuzz mutations,
//! fault injection, crash campaigns and the WAL's `salt2`. Not for secrets.

/// The multiplier of PCG's 64-bit LCG.
const MULTIPLIER: u64 = 6_364_136_223_846_793_005;

/// O'Neill's PCG32 (XSH-RR output, 64-bit state, 32-bit output): for the
/// same seed and stream, the same outputs as the reference C library
/// (pcg-c-basic) on every target, including [`Pcg32::range`] and
/// [`Pcg32::shuffle`].
///
/// Not for secrets: a few outputs reveal the state. Values that must be
/// unguessable (cancel keys, `gen_random_uuid()`) need OS randomness
/// (`docs/subsystems/os-and-vfs.md`, "Open questions").
///
/// ```
/// use hardshell_core::rng::Pcg32;
///
/// let mut rng = Pcg32::new(42, 54);
/// assert_eq!(rng.next_u32(), 0xA15C_02B7);
/// let die = rng.range(6) + 1;
/// assert!((1..=6).contains(&die));
/// ```
// Not Copy: a silently copied generator repeats its sequence. Clone is for
// a deliberate fork.
#[derive(Clone, Debug)]
pub struct Pcg32 {
    state: u64,
    /// The stream selector, shifted left once with the low bit set: odd.
    increment: u64,
}

impl Pcg32 {
    /// Seeds a generator as the reference's `pcg32_srandom_r(seed, stream)`
    /// does: `seed` is the initial state, `stream` picks one of 2^63
    /// sequences (its top bit is ignored).
    ///
    /// Different streams give unrelated sequences for the same seed, so
    /// case `i` of a run can be `Pcg32::new(seed, i)` and replay alone.
    pub fn new(seed: u64, stream: u64) -> Pcg32 {
        let mut rng = Pcg32 {
            state: 0,
            increment: (stream << 1) | 1,
        };
        rng.step();
        rng.state = rng.state.wrapping_add(seed);
        rng.step();
        rng
    }

    #[inline]
    fn step(&mut self) {
        self.state = self
            .state
            .wrapping_mul(MULTIPLIER)
            .wrapping_add(self.increment);
    }

    /// The next 32 random bits (`pcg32_random_r`).
    #[inline]
    pub fn next_u32(&mut self) -> u32 {
        let old = self.state;
        self.step();
        // Truncation intended: XSH keeps bits 27..59, RR rotates by the top 5.
        let xorshifted = (((old >> 18) ^ old) >> 27) as u32;
        xorshifted.rotate_right((old >> 59) as u32)
    }

    /// The next 64 random bits: two [`Pcg32::next_u32`] outputs, the first
    /// as the low half, so `next_u64().to_le_bytes()` is what
    /// [`Pcg32::fill_bytes`] would give for 8 bytes.
    #[inline]
    pub fn next_u64(&mut self) -> u64 {
        let low = u64::from(self.next_u32());
        let high = u64::from(self.next_u32());
        (high << 32) | low
    }

    /// A uniform value in `0..bound`, without modulo bias.
    ///
    /// For a `bound` up to `u32::MAX` this is the reference's
    /// `pcg32_boundedrand_r`: outputs below `2^32 mod bound` are drawn
    /// again, and the first other output is taken modulo `bound`. A larger
    /// `bound` does the same with [`Pcg32::next_u64`].
    ///
    /// # Panics
    ///
    /// If `bound` is 0: there is no value to return. The bound is the
    /// caller's choice, never input, so a caller whose bound can be 0 (an
    /// index into a slice that may be empty) checks first.
    #[inline]
    #[track_caller]
    pub fn range(&mut self, bound: usize) -> usize {
        assert!(bound != 0, "Pcg32::range needs a bound of at least 1");
        if let Ok(bound) = u32::try_from(bound) {
            let threshold = bound.wrapping_neg() % bound;
            loop {
                let r = self.next_u32();
                if r >= threshold {
                    // Lossless: usize is 64 bits on every target (ADR-0001).
                    return (r % bound) as usize;
                }
            }
        }
        // Lossless both ways: usize is 64 bits on every target (ADR-0001).
        let bound = bound as u64;
        let threshold = bound.wrapping_neg() % bound;
        loop {
            let r = self.next_u64();
            if r >= threshold {
                return (r % bound) as usize;
            }
        }
    }

    /// Shuffles `items` uniformly: Fisher-Yates from the last item down,
    /// swapping item `i - 1` with item `range(i)`, as the reference demo
    /// deals its cards. Draws nothing for fewer than two items.
    pub fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (2..=items.len()).rev() {
            let j = self.range(i);
            items.swap(j, i - 1);
        }
    }

    /// Fills `bytes` with random bytes: each 4 bytes are one
    /// [`Pcg32::next_u32`] in little-endian order, and a last group of 1 to
    /// 3 bytes takes the low bytes of one more output.
    pub fn fill_bytes(&mut self, bytes: &mut [u8]) {
        let (groups, tail) = bytes.as_chunks_mut::<4>();
        for group in groups {
            *group = self.next_u32().to_le_bytes();
        }
        if !tail.is_empty() {
            let word = self.next_u32().to_le_bytes();
            for (dst, src) in tail.iter_mut().zip(word) {
                *dst = src;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Pcg32;

    /// Round 1 of the reference demo, pcg-c-basic's `pcg32-demo`, seeded
    /// `pcg32_srandom_r(&rng, 42u, 54u)`, all drawn from one generator in
    /// this order. pcg-c-basic ships no expected output; the same values
    /// are round 1 of pcg-c's `test-high/expected/check-pcg32.out`, whose
    /// extra `Again:` line rewinds six steps and draws the same six values.
    const DEMO_OUTPUTS: [u32; 6] = [
        0xA15C_02B7,
        0x7B47_F409,
        0xBA1D_3330,
        0x83D2_F293,
        0xBFA4_784B,
        0xCBED_606E,
    ];
    const DEMO_COINS: &str = "HHTTTHTHHHTHTTTHHHHHTTTHHHTHTHTHTTHTTTHHHHHHTTTTHHTTTTTHTTTTTTTHT";
    const DEMO_ROLLS: [usize; 33] = [
        3, 4, 1, 1, 2, 2, 3, 2, 4, 3, 2, 4, 3, 3, 5, 2, 3, 1, 3, 1, 5, 1, 4, 1, 5, 6, 4, 6, 6, 2,
        6, 3, 3,
    ];
    const DEMO_CARDS: &str = "Qd Ks 6d 3s 3d 4c 3h Td Kc 5c Jh Kd Jd As 4s 4h Ad Th Ac Jc 7s Qs \
                              2s 7h Kh 2d 6c Ah 4d Qh 9h 6s 5s 2c 9c Ts 8d 9s 3c 8c Js 5d 2h 6h \
                              7d 8s 9d 5h 8h Qc 7c Tc";

    #[test]
    fn matches_the_reference_demo() {
        let mut rng = Pcg32::new(42, 54);
        let outputs: Vec<u32> = (0..6).map(|_| rng.next_u32()).collect();
        assert_eq!(outputs, DEMO_OUTPUTS);
        let coins: String = (0..65)
            .map(|_| if rng.range(2) == 1 { 'H' } else { 'T' })
            .collect();
        assert_eq!(coins, DEMO_COINS);
        let rolls: Vec<usize> = (0..33).map(|_| rng.range(6) + 1).collect();
        assert_eq!(rolls, DEMO_ROLLS);
        let mut cards: Vec<usize> = (0..52).collect();
        rng.shuffle(&mut cards);
        let names: Vec<String> = cards
            .iter()
            .map(|&c| {
                let number = char::from(b"A23456789TJQK"[c / 4]);
                let suit = char::from(b"hcds"[c % 4]);
                format!("{number}{suit}")
            })
            .collect();
        assert_eq!(names.join(" "), DEMO_CARDS);
    }

    #[test]
    fn next_u64_is_low_half_first() {
        let want = u64::from(DEMO_OUTPUTS[1]) << 32 | u64::from(DEMO_OUTPUTS[0]);
        assert_eq!(want, 0x7B47_F409_A15C_02B7);
        let mut rng = Pcg32::new(42, 54);
        assert_eq!(rng.next_u64(), want);
        assert_eq!(rng.next_u32(), DEMO_OUTPUTS[2]);
    }

    #[test]
    fn fill_bytes_is_little_endian_words() {
        let mut rng = Pcg32::new(42, 54);
        let mut bytes = [0; 7];
        rng.fill_bytes(&mut bytes);
        assert_eq!(bytes, [0xB7, 0x02, 0x5C, 0xA1, 0x09, 0xF4, 0x47]);
        assert_eq!(
            rng.next_u32(),
            DEMO_OUTPUTS[2],
            "a partial group uses one whole output"
        );
        rng.fill_bytes(&mut []);
        assert_eq!(
            rng.next_u32(),
            DEMO_OUTPUTS[3],
            "an empty fill draws nothing"
        );
        for len in 0..=9 {
            let mut a = Pcg32::new(7, 3);
            let mut b = a.clone();
            let mut bytes = vec![0; len];
            a.fill_bytes(&mut bytes);
            let want: Vec<u8> = (0..3).flat_map(|_| b.next_u32().to_le_bytes()).collect();
            assert_eq!(bytes, want[..len], "len {len}");
        }
        let mut a = Pcg32::new(7, 3);
        let mut b = a.clone();
        let mut eight = [0; 8];
        a.fill_bytes(&mut eight);
        assert_eq!(eight, b.next_u64().to_le_bytes());
    }

    #[test]
    fn range_edges() {
        let mut rng = Pcg32::new(42, 54);
        assert_eq!(rng.range(1), 0);
        assert_eq!(rng.next_u32(), DEMO_OUTPUTS[1], "range(1) draws once");
        for bound in [
            2,
            3,
            7,
            1 << 31,
            u32::MAX as usize,
            1 << 32,
            1 << 40,
            usize::MAX,
        ] {
            for _ in 0..1000 {
                assert!(rng.range(bound) < bound, "{bound}");
            }
        }
    }

    /// `range` at the edge of its rejection threshold, on both paths: each
    /// bound makes a demo output equal the threshold or fall one below it,
    /// and the next output shows how many were drawn.
    #[test]
    fn range_matches_the_reference_at_its_thresholds() {
        let o = DEMO_OUTPUTS.map(u64::from);
        let after_one = || {
            let mut rng = Pcg32::new(42, 54);
            rng.next_u32();
            rng
        };
        // 32 bits: the threshold of 2^32 - o[1] is o[1], so o[1] is taken.
        let bound = (1 << 32) - o[1];
        let mut rng = after_one();
        assert_eq!(rng.range(bound as usize), o[1] as usize);
        assert_eq!(rng.next_u32(), DEMO_OUTPUTS[2]);
        // One less, and the threshold is o[1] + 1: o[1] is rejected and
        // o[2] taken.
        let mut rng = after_one();
        let want = o[2] % (bound - 1);
        assert_eq!(rng.range((bound - 1) as usize), want as usize);
        assert_eq!(rng.next_u32(), DEMO_OUTPUTS[3]);
        // u32::MAX still takes the 32-bit path: one draw.
        let mut rng = Pcg32::new(42, 54);
        assert_eq!(rng.range(u32::MAX as usize), o[0] as usize);
        assert_eq!(rng.next_u32(), DEMO_OUTPUTS[1]);
        // 64 bits: the same edges with the first `next_u64`.
        let first = o[1] << 32 | o[0];
        let second = o[3] << 32 | o[2];
        let bound = first.wrapping_neg();
        let mut rng = Pcg32::new(42, 54);
        assert_eq!(rng.range(bound as usize), first as usize);
        assert_eq!(rng.next_u32(), DEMO_OUTPUTS[2]);
        let mut rng = Pcg32::new(42, 54);
        let want = second % (bound - 1);
        assert_eq!(rng.range((bound - 1) as usize), want as usize);
        assert_eq!(rng.next_u32(), DEMO_OUTPUTS[4]);
    }

    #[test]
    #[should_panic(expected = "Pcg32::range needs a bound of at least 1")]
    fn range_zero_panics() {
        Pcg32::new(0, 0).range(0);
    }

    #[test]
    fn range_rejects_the_biased_outputs() {
        // With these bounds plain modulo would put half the draws below a
        // third of the bound, instead of a third.
        let mut rng = Pcg32::new(0x5EED, 1);
        let bound = 3 << 30;
        let low = (0..30_000).filter(|_| rng.range(bound) < 1 << 30).count();
        assert!((9_000..11_000).contains(&low), "32-bit path: {low}");
        let bound = 3 << 62;
        let low = (0..30_000).filter(|_| rng.range(bound) < 1 << 62).count();
        assert!((9_000..11_000).contains(&low), "64-bit path: {low}");
    }

    #[test]
    fn shuffle_permutes_and_skips_short_slices() {
        let mut rng = Pcg32::new(7, 7);
        let mut empty: [u8; 0] = [];
        rng.shuffle(&mut empty);
        let mut one = [9];
        rng.shuffle(&mut one);
        assert_eq!(one, [9]);
        assert_eq!(rng.next_u32(), Pcg32::new(7, 7).next_u32(), "nothing drawn");
        let mut items: Vec<u32> = (0..1000).collect();
        rng.shuffle(&mut items);
        assert_ne!(items, (0..1000).collect::<Vec<_>>());
        items.sort_unstable();
        assert_eq!(items, (0..1000).collect::<Vec<_>>());
    }

    #[test]
    fn seeds_repeat_and_streams_differ() {
        let first = |seed, stream| {
            let mut rng = Pcg32::new(seed, stream);
            [
                rng.next_u32(),
                rng.next_u32(),
                rng.next_u32(),
                rng.next_u32(),
            ]
        };
        assert_eq!(first(1, 0), first(1, 0));
        assert_ne!(first(1, 0), first(1, 1));
        assert_ne!(first(1, 0), first(2, 0));
        assert_eq!(
            first(1, 5),
            first(1, 5 | 1 << 63),
            "the top bit of stream is ignored"
        );
    }
}
