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

//! CRC-32C (Castagnoli: reflected polynomial `0x82F63B78`, initial value
//! and final XOR `0xFFFFFFFF`), the checksum of every page, the file and
//! WAL headers, and every WAL frame (ADR-0009).
//!
//! Software only: slicing-by-8 over tables built at compile time. Hardware
//! CRC needs `unsafe` intrinsics (core.md, "Open questions").

/// The reflected Castagnoli polynomial.
const POLY: u32 = 0x82F6_3B78;

/// `TABLES[0][b]` is the register after feeding byte `b` into a zero
/// register; `TABLES[k][b]` is the same followed by `k` zero bytes, so
/// eight lookups advance the register by eight bytes. A `static`, so the
/// 8 KiB exist once.
static TABLES: [[u32; 256]; 8] = tables();

const fn tables() -> [[u32; 256]; 8] {
    let mut t = [[0u32; 256]; 8];
    let mut i = 0;
    while i < 256 {
        let mut crc = i as u32;
        let mut bit = 0;
        while bit < 8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ POLY
            } else {
                crc >> 1
            };
            bit += 1;
        }
        t[0][i] = crc;
        i += 1;
    }
    let mut k = 1;
    while k < 8 {
        let mut i = 0;
        while i < 256 {
            let prev = t[k - 1][i];
            t[k][i] = (prev >> 8) ^ t[0][(prev & 0xFF) as usize];
            i += 1;
        }
        k += 1;
    }
    t
}

/// The CRC-32C of `bytes`.
///
/// ```
/// use hardshell_core::crc32c::crc32c;
///
/// assert_eq!(crc32c(b"123456789"), 0xE306_9283);
/// assert_eq!(crc32c(b""), 0);
/// ```
pub fn crc32c(bytes: &[u8]) -> u32 {
    crc32c_append(0, bytes)
}

/// Continues a CRC-32C: given `crc == crc32c(a)`, returns the CRC-32C of
/// `a` followed by `bytes`.
///
/// The finished checksum is the whole state, so this is the WAL's chained
/// frame checksum ("continued from the previous frame's checksum") and how
/// one checksum covers bytes that are not contiguous. The CRC-32C of no
/// bytes is 0, so `crc32c_append(0, bytes) == crc32c(bytes)`.
///
/// ```
/// use hardshell_core::crc32c::{crc32c, crc32c_append};
///
/// assert_eq!(crc32c_append(crc32c(b"1234"), b"56789"), crc32c(b"123456789"));
/// ```
pub fn crc32c_append(crc: u32, bytes: &[u8]) -> u32 {
    let t = &TABLES;
    let mut reg = !crc;
    let (words, tail) = bytes.as_chunks::<8>();
    for &[b0, b1, b2, b3, b4, b5, b6, b7] in words {
        let [r0, r1, r2, r3] = (reg ^ u32::from_le_bytes([b0, b1, b2, b3])).to_le_bytes();
        // Every index is a u8, so every lookup is in bounds.
        reg = t[7][usize::from(r0)]
            ^ t[6][usize::from(r1)]
            ^ t[5][usize::from(r2)]
            ^ t[4][usize::from(r3)]
            ^ t[3][usize::from(b4)]
            ^ t[2][usize::from(b5)]
            ^ t[1][usize::from(b6)]
            ^ t[0][usize::from(b7)];
    }
    for &b in tail {
        let [low, ..] = reg.to_le_bytes();
        reg = (reg >> 8) ^ t[0][usize::from(low ^ b)];
    }
    !reg
}

#[cfg(test)]
mod tests {
    use super::{crc32c, crc32c_append};
    use crate::rng::Pcg32;

    /// Bit at a time, from the definition, sharing nothing with the tables.
    fn reference(bytes: &[u8]) -> u32 {
        let mut reg = !0u32;
        for &b in bytes {
            reg ^= u32::from(b);
            for _ in 0..8 {
                reg = if reg & 1 == 1 {
                    (reg >> 1) ^ 0x82F6_3B78
                } else {
                    reg >> 1
                };
            }
        }
        !reg
    }

    fn random_bytes(seed: u64, len: usize) -> Vec<u8> {
        let mut bytes = vec![0; len];
        Pcg32::new(seed, 0).fill_bytes(&mut bytes);
        bytes
    }

    /// The CRC catalogue's `check` for CRC-32/ISCSI.
    #[test]
    fn check_value() {
        assert_eq!(crc32c(b"123456789"), 0xE306_9283);
        assert_eq!(reference(b"123456789"), 0xE306_9283);
        assert_eq!(crc32c(b""), 0);
    }

    /// RFC 3720 (iSCSI), appendix B.4. The RFC prints each CRC's bytes in
    /// wire order, least significant first: `aa 36 91 8a` is 0x8A91_36AA.
    #[test]
    fn rfc_3720_vectors() {
        let ascending: Vec<u8> = (0..32).collect();
        let descending: Vec<u8> = (0..32).rev().collect();
        let read_pdu: [u8; 48] = [
            0x01, 0xC0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x14, 0x00, 0x00, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x00, 0x14,
            0x00, 0x00, 0x00, 0x18, 0x28, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ];
        let vectors: [(&[u8], u32); 5] = [
            (&[0x00; 32], 0x8A91_36AA),
            (&[0xFF; 32], 0x62A8_AB43),
            (&ascending, 0x46DD_794E),
            (&descending, 0x113F_DB5C),
            (&read_pdu, 0xD996_3A56),
        ];
        for (bytes, want) in vectors {
            assert_eq!(crc32c(bytes), want, "{bytes:02x?}");
            assert_eq!(reference(bytes), want, "{bytes:02x?}");
        }
    }

    #[test]
    fn tables_match_the_bitwise_reference() {
        for b in 0..=u8::MAX {
            assert_eq!(crc32c(&[b]), reference(&[b]), "byte {b:#04x}");
        }
        let buf = random_bytes(1, 65_536 + 8);
        // Every tail length and word count up to 80 bytes, at every alignment.
        for start in 0..8 {
            for len in 0..=80 {
                let bytes = &buf[start..start + len];
                assert_eq!(crc32c(bytes), reference(bytes), "len {len} at {start}");
            }
        }
        for len in (81..=272).chain([4092, 4096, 65_536]) {
            let bytes = &buf[1..=len];
            assert_eq!(crc32c(bytes), reference(bytes), "len {len}");
        }
        for page in [[0x00u8; 4096], [0xFF; 4096]] {
            assert_eq!(crc32c(&page), reference(&page));
        }
    }

    #[test]
    fn append_equals_one_shot_at_every_split() {
        let buf = random_bytes(2, 100);
        let whole = crc32c(&buf);
        assert_eq!(crc32c_append(0, &buf), whole);
        assert_eq!(crc32c_append(whole, &[]), whole, "no bytes changes nothing");
        for i in 0..=buf.len() {
            let (a, rest) = buf.split_at(i);
            assert_eq!(crc32c_append(crc32c(a), rest), whole, "split at {i}");
            for j in 0..=rest.len() {
                let (b, c) = rest.split_at(j);
                let chained = crc32c_append(crc32c_append(crc32c(a), b), c);
                assert_eq!(chained, whole, "splits at {i} and {}", i + j);
            }
        }
    }

    #[test]
    fn many_appends_equal_one_shot() {
        let buf = random_bytes(3, 4096 + 24);
        let whole = crc32c(&buf);
        let mut rng = Pcg32::new(3, 1);
        for case in 0..200 {
            let mut crc = 0;
            let mut rest = &buf[..];
            while !rest.is_empty() {
                let (piece, after) = rest.split_at((1 + rng.range(100)).min(rest.len()));
                crc = crc32c_append(crc, piece);
                rest = after;
            }
            assert_eq!(crc, whole, "case {case}");
        }
    }

    /// The WAL's chain (wal-and-transactions.md): the header's CRC over its
    /// bytes 0..28, then for each frame bytes 0..20 of its header and its
    /// page, continued from the previous checksum. Each link equals one CRC
    /// over everything so far.
    #[test]
    fn a_wal_chain_equals_one_crc_over_everything_so_far() {
        let header = random_bytes(4, 28);
        let mut chain = crc32c(&header);
        let mut all = header.clone();
        for frame in 0..3 {
            let head = random_bytes(5 + frame, 24);
            let page = random_bytes(8 + frame, 4096);
            chain = crc32c_append(crc32c_append(chain, &head[..20]), &page);
            all.extend_from_slice(&head[..20]);
            all.extend_from_slice(&page);
            assert_eq!(chain, crc32c(&all), "frame {frame}");
        }
    }
}
