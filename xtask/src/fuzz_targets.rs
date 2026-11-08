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

//! The `cargo xtask fuzz-smoke` targets: one for each parser or decoder of
//! untrusted bytes, registered by the task that adds it
//! (`docs/04-quality.md`, "Fuzz smoke"). Each target's seeds are the files
//! in `fuzz/corpus/<name>/`.

use hardshell_core::error::CoreError;
use hardshell_core::varint::{decode_u64, encode_u64};

/// A fuzz target.
pub(crate) struct Target {
    /// What `--target` names, and the directory of its seeds under
    /// `fuzz/corpus/` and of its findings under `fuzz/crashes/`.
    pub(crate) name: &'static str,
    /// The longest input: no seed may be longer, and the mutator cuts what
    /// it makes to this.
    pub(crate) max_len: usize,
    /// Runs one input, and panics on a bug: the code under test panicking,
    /// or the target's own check of its answer failing.
    pub(crate) run: fn(&[u8]),
}

/// Every target, in the order a run takes them.
pub(crate) const TARGETS: &[Target] = &[Target {
    name: "varint",
    max_len: 32,
    run: varint,
}];

/// `varint::decode_u64` on any bytes: an error, or a value whose shortest
/// encoding is exactly the bytes it used, since the decoder accepts only
/// that one.
fn varint(input: &[u8]) {
    check_varint(decode_u64, input);
}

/// A decoder of `varint::decode_u64`'s shape.
type DecodeU64 = fn(&[u8]) -> Result<(u64, usize), CoreError>;

/// The `varint` target with `decode` as the decoder, so a test can show
/// that it catches a wrong one.
fn check_varint(decode: DecodeU64, input: &[u8]) {
    if let Ok((value, used)) = decode(input) {
        let mut encoded = Vec::new();
        encode_u64(value, &mut encoded);
        assert_eq!(
            input.get(..used),
            Some(encoded.as_slice()),
            "{input:02x?} decodes to {value} in {used} bytes, but {value} encodes as {encoded:02x?}"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::{TARGETS, check_varint, varint};
    use crate::fuzz_smoke::load_seeds;
    use hardshell_core::error::CoreError;
    use hardshell_core::testing::check;
    use std::fs;
    use std::path::Path;

    #[test]
    fn the_varint_target_passes_every_input_of_up_to_two_bytes() {
        varint(&[]);
        for first in 0..=u8::MAX {
            varint(&[first]);
            for second in 0..=u8::MAX {
                varint(&[first, second]);
            }
        }
    }

    #[test]
    fn the_varint_target_passes_long_inputs() {
        check(
            "varint target on long inputs",
            10_000,
            |rng| {
                let mut bytes = vec![0; rng.range(16)];
                rng.fill_bytes(&mut bytes);
                // Mostly continuation bytes, so most inputs run long.
                for byte in &mut bytes {
                    if rng.range(8) != 0 {
                        *byte |= 0x80;
                    }
                }
                bytes
            },
            |bytes| {
                varint(bytes);
                Ok(())
            },
        );
    }

    /// A decoder without the shortest-encoding rule.
    fn lax(input: &[u8]) -> Result<(u64, usize), CoreError> {
        let mut value = 0;
        for (i, &byte) in input.iter().take(10).enumerate() {
            value |= u64::from(byte & 0x7F) << (7 * i);
            if byte & 0x80 == 0 {
                return Ok((value, i + 1));
            }
        }
        Err(CoreError::VarintTruncated)
    }

    #[test]
    #[should_panic(
        expected = "[ac, 82, 00] decodes to 300 in 3 bytes, but 300 encodes as [ac, 02]"
    )]
    fn the_varint_check_catches_a_decoder_that_takes_an_overlong_encoding() {
        check_varint(lax, &[0xAC, 0x02]);
        check_varint(lax, &[0xAC, 0x82, 0x00]);
    }

    /// A decoder that claims more bytes than it was given.
    fn greedy(input: &[u8]) -> Result<(u64, usize), CoreError> {
        Ok((0, input.len() + 1))
    }

    #[test]
    #[should_panic(expected = "[00] decodes to 0 in 2 bytes, but 0 encodes as [00]")]
    fn the_varint_check_catches_a_decoder_that_overreads() {
        check_varint(greedy, &[0x00]);
    }

    #[test]
    fn target_names_are_unique_and_plain() {
        for (i, target) in TARGETS.iter().enumerate() {
            let name = target.name;
            assert!(
                name.starts_with(|c: char| c.is_ascii_lowercase())
                    && name
                        .bytes()
                        .all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'-')),
                "{name}"
            );
            assert!(
                TARGETS[..i].iter().all(|other| other.name != name),
                "{name} twice"
            );
            assert!(target.max_len > 0, "{name}");
        }
    }

    /// Every target has seeds in `fuzz/corpus/<name>/`, each within its
    /// limit and passing it, and every corpus directory has a target.
    #[test]
    fn every_target_passes_its_own_seeds() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let mut dirs: Vec<String> = fs::read_dir(root.join("fuzz/corpus"))
            .unwrap()
            .map(|entry| entry.unwrap())
            .filter(|entry| entry.path().is_dir())
            .map(|entry| entry.file_name().into_string().unwrap())
            .filter(|name| !name.starts_with('.'))
            .collect();
        dirs.sort();
        let mut names: Vec<&str> = TARGETS.iter().map(|target| target.name).collect();
        names.sort_unstable();
        assert_eq!(dirs, names);
        for target in TARGETS {
            let seeds = load_seeds(&root, target).unwrap();
            for seed in &seeds {
                (target.run)(seed);
            }
        }
    }
}
