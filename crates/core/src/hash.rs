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

//! Hashing: [`fnv1a64`], and [`FastHasher`] behind [`FastHashMap`] and
//! [`FastHashSet`].
//!
//! Both are deterministic and non-cryptographic, and neither resists keys
//! chosen to collide: where a remote party picks the keys, use `std`'s
//! `RandomState` instead (`docs/02-dependency-policy.md`, item 12).

use std::collections::{HashMap, HashSet};
use std::hash::{BuildHasherDefault, Hasher};

/// FNV-1a 64 of `bytes`: from the 64-bit offset basis, each byte is XORed
/// in, then the hash is multiplied by the 64-bit FNV prime.
///
/// The algorithm fixes every value, so unlike [`FastHasher`]'s these may be
/// stored, printed or compared across builds and targets.
///
/// ```
/// use hardshell_core::hash::fnv1a64;
///
/// assert_eq!(fnv1a64(b"foobar"), 0x8594_4171_F739_67E8);
/// ```
pub fn fnv1a64(bytes: &[u8]) -> u64 {
    const OFFSET_BASIS: u64 = 0xCBF2_9CE4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01B3; // 2^40 + 2^8 + 0xB3
    bytes.iter().fold(OFFSET_BASIS, |hash, &byte| {
        (hash ^ u64::from(byte)).wrapping_mul(PRIME)
    })
}

/// The state before any input: the first 64 bits of pi's fraction. Not 0,
/// which a zero word would leave unchanged.
const SEED: u64 = 0x243F_6A88_85A3_08D3;
/// The multiplier for each word: 2^64 over the golden ratio, odd.
const MIX: u64 = 0x9E37_79B9_7F4A_7C15;
/// The multiplier for `finish`: the first 64 bits of e's fraction with the
/// low bit set, to make it odd.
const FINISH: u64 = 0xB7E1_5162_8AED_2A6B;

/// Multiplies into 128 bits and XORs the two halves, so every bit of `a`
/// reaches both the low and the high bits of the result.
#[inline]
fn fold(a: u64, b: u64) -> u64 {
    let product = u128::from(a) * u128::from(b);
    // Truncation intended: the low and high halves.
    (product as u64) ^ ((product >> 64) as u64)
}

/// The hasher behind [`FastHashMap`] and [`FastHashSet`]: fast and
/// deterministic, not cryptographic.
///
/// Each 64-bit word of input is folded into the state, and `finish` folds
/// once more, so both the low bits `HashMap` takes a bucket from and the
/// top 7 bits it keeps as a tag depend on every input bit. An integer of
/// up to 64 bits is one word (a 128-bit one goes through `write`). `write`
/// takes the 8-byte little-endian words of its input,
/// then one last word holding the 0 to 7 bytes left over and, in its top
/// byte, how many there are, so inputs of different lengths never give
/// the same words.
///
/// A value's hash is the same in every run and on every target. The
/// algorithm is not a format and may change: never store or print its
/// hashes (use [`fnv1a64`]).
#[derive(Debug)]
pub struct FastHasher {
    state: u64,
}

impl Default for FastHasher {
    #[inline]
    fn default() -> FastHasher {
        FastHasher { state: SEED }
    }
}

impl FastHasher {
    #[inline]
    fn add(&mut self, word: u64) {
        self.state = fold(self.state ^ word, MIX);
    }
}

impl Hasher for FastHasher {
    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        let (words, tail) = bytes.as_chunks::<8>();
        for word in words {
            self.add(u64::from_le_bytes(*word));
        }
        // A shift loop, not a copy into a buffer: the copy compiled to a
        // memcpy call and measured about 3 times slower on short names.
        let last = tail
            .iter()
            .enumerate()
            .fold((tail.len() as u64) << 56, |word, (i, &byte)| {
                word | u64::from(byte) << (8 * i)
            });
        self.add(last);
    }

    #[inline]
    fn write_u8(&mut self, n: u8) {
        self.add(u64::from(n));
    }

    #[inline]
    fn write_u16(&mut self, n: u16) {
        self.add(u64::from(n));
    }

    #[inline]
    fn write_u32(&mut self, n: u32) {
        self.add(u64::from(n));
    }

    #[inline]
    fn write_u64(&mut self, n: u64) {
        self.add(n);
    }

    #[inline]
    fn write_usize(&mut self, n: usize) {
        // Lossless: usize is 64 bits on every target (ADR-0001).
        self.add(n as u64);
    }

    #[inline]
    fn finish(&self) -> u64 {
        fold(self.state, FINISH)
    }
}

/// A `HashMap` hashed with [`FastHasher`]: make one with
/// `FastHashMap::default()`, or `with_capacity_and_hasher(n,
/// Default::default())`.
///
/// Its iteration order depends only on the operations applied to it, never
/// on per-process random state. It can still differ between targets and
/// `std` releases (the table's probe groups are not the same width
/// everywhere), so output whose order matters sorts.
pub type FastHashMap<K, V> = HashMap<K, V, BuildHasherDefault<FastHasher>>;

/// A `HashSet` hashed with [`FastHasher`]: make one with
/// `FastHashSet::default()`. Its order is as [`FastHashMap`]'s.
pub type FastHashSet<T> = HashSet<T, BuildHasherDefault<FastHasher>>;

#[cfg(test)]
mod tests {
    use super::{FastHashMap, FastHashSet, FastHasher, fnv1a64};
    use std::hash::{BuildHasher, BuildHasherDefault, Hash, Hasher};

    fn hash<T: Hash + ?Sized>(value: &T) -> u64 {
        BuildHasherDefault::<FastHasher>::default().hash_one(value)
    }

    fn raw(bytes: &[u8]) -> u64 {
        let mut h = FastHasher::default();
        h.write(bytes);
        h.finish()
    }

    /// From `test_fnv.c` (the FNV reference test suite): `TEST` vectors
    /// hash the string alone, `TEST0` vectors also its trailing NUL.
    #[test]
    fn fnv1a64_reference_vectors() {
        let vectors: [(&[u8], u64); 9] = [
            (b"", 0xCBF2_9CE4_8422_2325),
            (b"a", 0xAF63_DC4C_8601_EC8C),
            (b"b", 0xAF63_DF4C_8601_F1A5),
            (b"fo", 0x0898_5907_B541_D342),
            (b"fooba", 0xCAC1_65AF_A2FE_F40A),
            (b"foobar", 0x8594_4171_F739_67E8),
            (b"\0", 0xAF63_BD4C_8601_B7DF),
            (b"a\0", 0x089B_E207_B544_F1E4),
            (b"foobar\0", 0x3453_1CA7_168B_8F38),
        ];
        for (bytes, want) in vectors {
            assert_eq!(fnv1a64(bytes), want, "{bytes:02x?}");
        }
    }

    /// Computed by this implementation, not published vectors. A hash is
    /// the same on every target, so CI on x86_64 and aarch64 checks that;
    /// changing these changes every map's iteration order, so do it on
    /// purpose.
    #[test]
    fn fast_hashes_are_pinned() {
        assert_eq!(FastHasher::default().finish(), 0x0119_9719_BDC2_F07B);
        assert_eq!(hash(&7u32), 0x48B7_398D_A959_A35B);
        assert_eq!(hash(&7u16), 0x48B7_398D_A959_A35B, "one word, as 7u32");
        assert_eq!(hash(&u64::MAX), 0x45BC_FA50_05BC_F136);
        assert_eq!(hash(&0x0123_4567_89AB_CDEFu64), 0xC8FE_4592_CF33_7A99);
        assert_eq!(hash("hs_tables"), 0xC832_3ED4_B294_FB42);
        assert_eq!(hash(&[1u8, 2, 3][..]), 0x740E_ADDA_E14E_78F8);
    }

    #[test]
    fn inputs_of_different_lengths_differ() {
        // Zero tails of 0 to 7 bytes differ only in the length in the last
        // word, so it must tell all eight apart.
        let mut tails: Vec<u64> = (0..8).map(|n| raw(&[0; 7][..n])).collect();
        tails.sort_unstable();
        tails.dedup();
        assert_eq!(tails.len(), 8);
        // Without the length, `x` and `x\0` would collide; without a last
        // word for an empty tail, `abcdefg` and `abcdefg\x07` would.
        assert_ne!(raw(b"x"), raw(b"x\0"));
        assert_ne!(raw(b"abcdefg"), raw(b"abcdefg\x07"));
        assert_ne!(hash("abcdefg"), hash("abcdefg\x07"));
        // A zero state would let leading zero words vanish, and these would
        // collide.
        assert_ne!(raw(b""), raw(&[0; 8]));
        assert_ne!(raw(&[0; 8]), raw(&[0; 16]));
        let mut a = FastHasher::default();
        a.write_u64(0);
        a.write_u64(5);
        let mut b = FastHasher::default();
        b.write_u64(5);
        assert_ne!(a.finish(), b.finish());
    }

    /// Occupied buckets at 7/8 load (`HashMap`'s maximum), against what
    /// uniform random hashes would fill; and how many of the 128 top-7-bit
    /// tags occur.
    fn spread<T: Hash>(keys: &[T]) -> (f64, usize) {
        let buckets = 1usize << 17;
        let mut used = vec![false; buckets];
        let mut tags = [false; 128];
        for key in keys {
            let h = hash(key);
            used[(h as usize) & (buckets - 1)] = true;
            tags[(h >> 57) as usize] = true;
        }
        let n = keys.len() as f64;
        let m = buckets as f64;
        let expected = m * (1.0 - (-n / m).exp());
        let filled = used.iter().filter(|&&u| u).count() as f64;
        (filled / expected, tags.iter().filter(|&&t| t).count())
    }

    /// The key shapes the consumers use: page numbers, strided and
    /// high-bit integers, pairs, and short names. Without the finishing
    /// fold, `i << 32` keys fill about a fifth of the buckets they should;
    /// with a plain multiply in place of each fold, `i << 32` keys share one
    /// bucket and `i * 4096` keys only 32.
    #[test]
    fn keys_spread_over_low_and_high_bits() {
        let n = 114_688u32;
        let sequential: Vec<u32> = (1..=n).collect();
        let strided: Vec<u32> = (1..=n).map(|i| i * 4096).collect();
        let high: Vec<u64> = (1..=u64::from(n)).map(|i| i << 32).collect();
        let pairs: Vec<(u32, u64)> = (1..=n).map(|i| (i % 7, u64::from(i) << 20)).collect();
        let names: Vec<String> = (1..=n).map(|i| format!("t{i}")).collect();
        for (shape, (fill, tags)) in [
            ("sequential", spread(&sequential)),
            ("strided", spread(&strided)),
            ("high bits", spread(&high)),
            ("pairs", spread(&pairs)),
            ("names", spread(&names)),
        ] {
            assert!(fill > 0.95, "{shape}: {fill}");
            assert_eq!(tags, 128, "{shape}");
        }
    }

    #[test]
    fn iteration_order_depends_only_on_the_operations() {
        let build = || {
            let mut map = FastHashMap::default();
            for i in 0..1000u32 {
                map.insert(format!("table_{i}"), i);
            }
            for i in (0..1000u32).step_by(7) {
                map.remove(&format!("table_{i}"));
            }
            map.into_iter().collect::<Vec<_>>()
        };
        assert_eq!(build(), build());
        let set: FastHashSet<u32> = (0..100).collect();
        let again: FastHashSet<u32> = (0..100).collect();
        assert!(set.iter().eq(again.iter()));
    }
}
