//! A small, stable content hash for change detection.
//!
//! FNV-1a (64-bit) is deterministic across runs — unlike `std`'s `DefaultHasher`, whose output is
//! not stable — so a hash persisted in the state store still means the same thing after a restart.
//! This is a change-detection digest, not a cryptographic one.

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// The FNV-1a hash of `bytes`, as a zero-padded lowercase hex string.
#[must_use]
pub fn content_hash(bytes: &[u8]) -> String {
    let mut hash = FNV_OFFSET;
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_stable_and_distinguishes_content() {
        assert_eq!(content_hash(b"return 1"), content_hash(b"return 1"));
        assert_ne!(content_hash(b"return 1"), content_hash(b"return 2"));
    }
}
