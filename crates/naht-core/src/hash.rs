//! A small, stable content hash for change detection.
//!
//! FNV-1a (64-bit) is deterministic across runs, unlike `std`'s `DefaultHasher`, whose output is
//! not stable, so a hash persisted in the state store still means the same thing after a restart.
//! This is a change-detection digest, not a cryptographic one.

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

const FIRST_MODULUS: u64 = 1_000_000_007;
const SECOND_MODULUS: u64 = 998_244_353;
const FIRST_BASE: u64 = 257;
const SECOND_BASE: u64 = 911;

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

/// The source fingerprint of `bytes`: the byte length and two polynomial hashes modulo primes.
///
/// Every step stays below 2^53, so the Studio plugin computes the same string in Luau doubles.
#[must_use]
pub fn source_fingerprint(bytes: &[u8]) -> String {
    let (first, second) = bytes.iter().fold((0_u64, 0_u64), |(first, second), &byte| {
        (
            first
                .wrapping_mul(FIRST_BASE)
                .wrapping_add(u64::from(byte))
                .wrapping_rem(FIRST_MODULUS),
            second
                .wrapping_mul(SECOND_BASE)
                .wrapping_add(u64::from(byte))
                .wrapping_rem(SECOND_MODULUS),
        )
    });
    format!("{:x}-{first:08x}{second:08x}", bytes.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_stable_and_distinguishes_content() {
        assert_eq!(content_hash(b"return 1"), content_hash(b"return 1"));
        assert_ne!(content_hash(b"return 1"), content_hash(b"return 2"));
    }

    #[test]
    fn source_fingerprint_matches_the_plugin_golden_values() {
        assert_eq!(source_fingerprint(b"return 1"), "8-0a60e4b81d078f22");
        assert_eq!(source_fingerprint(b"return 2"), "8-0a60e4b91d078f23");
        assert_eq!(source_fingerprint(b""), "0-0000000000000000");
    }
}
