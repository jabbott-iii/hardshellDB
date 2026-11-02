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

//! Unsigned LEB128 varints for `u64`: seven bits per byte, low group first,
//! the high bit set on every byte but the last. A `u64` takes 1 to 10
//! bytes.
//!
//! The decoder reads hostile bytes (`.claude/rules/untrusted-input.md`): it
//! never panics, reads at most 10 bytes, and accepts only the shortest
//! encoding of each value, so every value has exactly one encoding.

use crate::error::CoreError;

/// The longest encoding of a `u64`: 64 bits in groups of seven. A property
/// of the format, not a setting.
const MAX_LEN: usize = 10;

/// Appends the shortest encoding of `value` to `out`: 1 byte below 128, up
/// to 10 bytes for `u64::MAX`.
///
/// ```
/// use hardshell_core::varint::encode_u64;
///
/// let mut out = vec![1]; // a row's version byte, say
/// encode_u64(300, &mut out);
/// assert_eq!(out, [1, 0xAC, 0x02]);
/// ```
#[inline]
pub fn encode_u64(mut value: u64, out: &mut Vec<u8>) {
    while value >= 0x80 {
        // The low seven bits with the continuation bit; `as` keeps the low byte.
        out.push(value as u8 | 0x80);
        value >>= 7;
    }
    out.push(value as u8);
}

/// Decodes the varint at the start of `input`, returning its value and the
/// number of bytes it used. Bytes after it are not read.
///
/// The value is as untrusted as `input`: check a length against the
/// remaining input and the caller's limit before using it.
///
/// ```
/// use hardshell_core::error::CoreError;
/// use hardshell_core::varint::decode_u64;
///
/// assert_eq!(decode_u64(&[0xAC, 0x02, 0xFF]), Ok((300, 2)));
/// assert_eq!(decode_u64(&[0xAC]), Err(CoreError::VarintTruncated));
/// assert_eq!(decode_u64(&[0xAC, 0x80, 0x00]), Err(CoreError::VarintOverlong));
/// ```
///
/// # Errors
///
/// - [`CoreError::VarintTruncated`]: `input` ends before a byte with its
///   high bit clear, within fewer than 10 bytes (an empty `input` included).
/// - [`CoreError::VarintOverlong`]: the last byte is `0x00` and is not the
///   first, so a shorter encoding of the value exists.
/// - [`CoreError::VarintOverflow`]: the tenth byte is more than `0x01`: the
///   value needs more than 64 bits, or the encoding continues past ten
///   bytes (an eleventh byte is never read).
#[inline]
pub fn decode_u64(input: &[u8]) -> Result<(u64, usize), CoreError> {
    let mut value = 0u64;
    for (i, &byte) in input.iter().take(MAX_LEN).enumerate() {
        // The tenth byte holds bit 63 alone: 0x01, or 0x00 (overlong, below).
        if i == MAX_LEN - 1 && byte > 1 {
            return Err(CoreError::VarintOverflow);
        }
        // i <= 9, so the shift is at most 63.
        value |= u64::from(byte & 0x7F) << (7 * i);
        if byte & 0x80 == 0 {
            return if byte == 0 && i > 0 {
                Err(CoreError::VarintOverlong)
            } else {
                Ok((value, i + 1))
            };
        }
    }
    // Fewer than 10 bytes, all with the continuation bit: 10 return above.
    Err(CoreError::VarintTruncated)
}

#[cfg(test)]
mod tests {
    use super::{decode_u64, encode_u64};
    use crate::error::CoreError;
    use crate::rng::Pcg32;

    /// What `decode_u64` returns.
    type Decoded = Result<(u64, usize), CoreError>;

    fn enc(value: u64) -> Vec<u8> {
        let mut out = Vec::new();
        encode_u64(value, &mut out);
        out
    }

    /// The length of the shortest encoding: one byte per started group of
    /// seven significant bits, and at least one.
    fn expected_len(value: u64) -> usize {
        let bits = 64 - value.leading_zeros() as usize;
        bits.div_ceil(7).max(1)
    }

    /// A decoder shaped differently from `decode_u64`: find the last byte
    /// among the first ten, add the groups up in 128 bits, then judge.
    fn model(input: &[u8]) -> Decoded {
        let Some(last) = input.iter().take(10).position(|&b| b < 0x80) else {
            return Err(if input.len() >= 10 {
                CoreError::VarintOverflow
            } else {
                CoreError::VarintTruncated
            });
        };
        let wide = input[..=last]
            .iter()
            .rev()
            .fold(0u128, |v, &b| (v << 7) | u128::from(b & 0x7F));
        let Ok(value) = u64::try_from(wide) else {
            return Err(CoreError::VarintOverflow);
        };
        if last > 0 && input[last] == 0 {
            return Err(CoreError::VarintOverlong);
        }
        Ok((value, last + 1))
    }

    /// The decoder's whole contract on one input: it agrees with the model
    /// (a panic fails the test), and a decoded value re-encodes to exactly
    /// the bytes it used.
    fn check(input: &[u8]) {
        let got = decode_u64(input);
        assert_eq!(got, model(input), "{input:02x?}");
        if let Ok((value, used)) = got {
            assert_eq!(enc(value), input[..used], "{input:02x?}");
        }
    }

    #[test]
    fn hand_made_vectors() {
        let cases: &[(u64, &[u8])] = &[
            (0, &[0x00]),
            (1, &[0x01]),
            (127, &[0x7F]),
            (128, &[0x80, 0x01]),
            (300, &[0xAC, 0x02]),
            (16_383, &[0xFF, 0x7F]),
            (16_384, &[0x80, 0x80, 0x01]),
            (624_485, &[0xE5, 0x8E, 0x26]),
            (u64::from(u32::MAX), &[0xFF, 0xFF, 0xFF, 0xFF, 0x0F]),
            (
                1 << 63,
                &[0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x01],
            ),
            (
                u64::MAX,
                &[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x01],
            ),
        ];
        for &(value, bytes) in cases {
            assert_eq!(enc(value), bytes, "encode {value}");
            assert_eq!(
                decode_u64(bytes),
                Ok((value, bytes.len())),
                "decode {value}"
            );
        }
    }

    #[test]
    fn encode_appends() {
        let mut out = vec![0xEE];
        encode_u64(300, &mut out);
        encode_u64(0, &mut out);
        assert_eq!(out, [0xEE, 0xAC, 0x02, 0x00]);
    }

    #[test]
    fn hand_made_rejections_and_trailing_bytes() {
        use CoreError::{VarintOverflow, VarintOverlong, VarintTruncated};
        let cases: &[(&[u8], Decoded)] = &[
            (&[0x01, 0xFF, 0xFF], Ok((1, 1))),
            (&[0x00, 0x80], Ok((0, 1))),
            (&[], Err(VarintTruncated)),
            (&[0x80], Err(VarintTruncated)),
            (&[0xFF; 9], Err(VarintTruncated)),
            (&[0x80, 0x00], Err(VarintOverlong)),
            (&[0xFF, 0x80, 0x00], Err(VarintOverlong)),
            (
                &[0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x00],
                Err(VarintOverlong),
            ),
            (
                &[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x02],
                Err(VarintOverflow),
            ),
            (
                &[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x7F],
                Err(VarintOverflow),
            ),
            (&[0x80; 10], Err(VarintOverflow)),
            (&[0x80; 64], Err(VarintOverflow)),
        ];
        for (input, want) in cases {
            assert_eq!(decode_u64(input), *want, "{input:02x?}");
            check(input);
        }
    }

    /// Nine continuation bytes, then every tenth byte, then an eleventh
    /// that must not change the result (it is never read).
    #[test]
    fn every_tenth_byte() {
        for prefix in [0x80u8, 0xFF] {
            for tenth in 0..=u8::MAX {
                let mut bytes = vec![prefix; 9];
                bytes.push(tenth);
                let want = match (tenth, prefix) {
                    (0x00, _) => Err(CoreError::VarintOverlong),
                    (0x01, 0x80) => Ok((1 << 63, 10)),
                    (0x01, _) => Ok((u64::MAX, 10)),
                    _ => Err(CoreError::VarintOverflow),
                };
                assert_eq!(decode_u64(&bytes), want, "{bytes:02x?}");
                check(&bytes);
                for eleventh in [0x00, 0x01, 0x80, 0xFF] {
                    bytes.push(eleventh);
                    assert_eq!(decode_u64(&bytes), want, "{bytes:02x?}");
                    bytes.pop();
                }
            }
        }
    }

    #[test]
    fn every_input_of_up_to_two_bytes() {
        check(&[]);
        for a in 0..=u8::MAX {
            check(&[a]);
            for b in 0..=u8::MAX {
                check(&[a, b]);
            }
        }
    }

    /// Every path through the decoder: up to ten continuation bytes, each
    /// with no payload (0x80) or all ones (0xFF), then nothing or a last
    /// byte of each kind, with and without a byte after it.
    #[test]
    fn every_continuation_prefix_then_every_kind_of_last_byte() {
        let mut input = Vec::new();
        for len in 0..=10u32 {
            for bits in 0..(1u32 << len) {
                for last in [None, Some(0x00), Some(0x01), Some(0x02), Some(0x7F)] {
                    input.clear();
                    input.extend((0..len).map(|i| if bits >> i & 1 == 1 { 0xFF } else { 0x80 }));
                    input.extend(last);
                    check(&input);
                    input.push(0x80);
                    check(&input);
                }
            }
        }
    }

    #[test]
    fn random_inputs_never_panic_and_match_the_model() {
        // Most bytes come from the boundary values, so long inputs reach
        // the tenth byte often; a quarter of the inputs are uniform bytes.
        const PICKS: [u8; 8] = [0x00, 0x01, 0x02, 0x7F, 0x80, 0x81, 0xFE, 0xFF];
        let mut rng = Pcg32::new(0x5EED, 1);
        let mut input = Vec::new();
        for case in 0..200_000 {
            input.clear();
            let uniform = rng.range(4) == 0;
            for _ in 0..rng.range(13) {
                let byte = if uniform {
                    rng.next_u32().to_le_bytes()[0]
                } else {
                    PICKS[rng.range(PICKS.len())]
                };
                input.push(byte);
            }
            let got = decode_u64(&input);
            assert_eq!(got, model(&input), "case {case}: {input:02x?}");
            if let Ok((value, used)) = got {
                assert_eq!(enc(value), input[..used], "case {case}: {input:02x?}");
            }
        }
    }

    /// The property test: boundary values and 100,000 random values whose
    /// bit lengths are uniform, so every encoded length is well covered.
    #[test]
    fn round_trips_and_rejects_cut_and_padded_forms() {
        let mut values = vec![0, u64::MAX];
        for k in 1..64 {
            let p = 1u64 << k;
            values.extend([p - 1, p, p + 1]);
        }
        let mut rng = Pcg32::new(0x5EED, 2);
        for _ in 0..100_000 {
            values.push(rng.next_u64() >> rng.range(64));
        }
        for value in values {
            let mut bytes = enc(value);
            let len = bytes.len();
            assert_eq!(len, expected_len(value), "{value}");
            assert_eq!(decode_u64(&bytes), Ok((value, len)), "{value}");
            for cut in 0..len {
                assert_eq!(
                    decode_u64(&bytes[..cut]),
                    Err(CoreError::VarintTruncated),
                    "{value} cut to {cut}"
                );
            }
            // One more zero group: overlong up to ten bytes, too long after.
            bytes[len - 1] |= 0x80;
            bytes.push(0x00);
            let want = if len < 10 {
                CoreError::VarintOverlong
            } else {
                CoreError::VarintOverflow
            };
            assert_eq!(decode_u64(&bytes), Err(want), "{value} padded");
            // Trailing bytes are not used.
            bytes[len - 1] &= 0x7F;
            bytes[len] = 0xFF;
            assert_eq!(decode_u64(&bytes), Ok((value, len)), "{value} + junk");
        }
    }

    #[test]
    fn rejects_every_padding_up_to_ten_bytes() {
        for value in [0, 1, 127, 128, 300, 1 << 56] {
            let mut bytes = enc(value);
            let last = bytes.len() - 1;
            bytes[last] |= 0x80;
            while bytes.len() < 10 {
                let mut padded = bytes.clone();
                padded.push(0x00);
                assert_eq!(
                    decode_u64(&padded),
                    Err(CoreError::VarintOverlong),
                    "{padded:02x?}"
                );
                bytes.push(0x80);
            }
        }
    }

    #[test]
    fn round_trips_every_value_below_two_to_the_14() {
        let mut bytes = Vec::with_capacity(2);
        for value in 0..(1u64 << 14) {
            bytes.clear();
            encode_u64(value, &mut bytes);
            assert_eq!(decode_u64(&bytes), Ok((value, bytes.len())), "{value}");
        }
    }
}
