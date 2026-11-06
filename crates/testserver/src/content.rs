//! Deterministic file content that can be read at any offset without storing it.

use sha2::{Digest, Sha256};

const WORD: u64 = 8;

/// The SplitMix64 finaliser: spreads similar inputs across all 64 bits.
fn mix(value: u64) -> u64 {
    let mut z = value.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Fills `buf` with the bytes at `offset` of the file identified by `seed`.
pub fn fill(seed: u64, offset: u64, buf: &mut [u8]) {
    let base = mix(seed);
    let mut word_index = offset / WORD;
    let mut skip = (offset % WORD) as usize;
    let mut written = 0;

    while written < buf.len() {
        let word = mix(base ^ word_index).to_le_bytes();
        let count = (word.len() - skip).min(buf.len() - written);
        buf[written..written + count].copy_from_slice(&word[skip..skip + count]);
        written += count;
        word_index += 1;
        skip = 0;
    }
}

/// Returns the lowercase hex SHA-256 of a complete file.
pub fn sha256(seed: u64, size: u64) -> String {
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    let mut offset = 0;

    while offset < size {
        let count = (size - offset).min(buf.len() as u64) as usize;
        fill(seed, offset, &mut buf[..count]);
        hasher.update(&buf[..count]);
        offset += count as u64;
    }

    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reading_at_an_offset_matches_a_sequential_read() {
        let mut whole = vec![0u8; 1000];
        fill(7, 0, &mut whole);

        for (start, end) in [(0, 1), (3, 11), (8, 16), (13, 999), (999, 1000)] {
            let mut part = vec![0u8; end - start];
            fill(7, start as u64, &mut part);
            assert_eq!(part, whole[start..end], "bytes {start}..{end}");
        }
    }

    #[test]
    fn different_seeds_are_not_shifted_copies_of_each_other() {
        let mut first = vec![0u8; 64];
        let mut second = vec![0u8; 64];
        fill(1, 8, &mut first);
        fill(0, 0, &mut second);
        assert_ne!(first, second);
    }

    #[test]
    fn checksum_covers_exactly_the_requested_size() {
        assert_eq!(
            sha256(1, 0),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_ne!(sha256(1, 100), sha256(1, 101));
    }
}
