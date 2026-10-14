//! Identifiers for stored items (dictionary entries, snippets, history records).

use std::collections::hash_map::RandomState;
use std::fmt::Write;
use std::hash::{BuildHasher, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// A new random identifier in the form the macOS app stores (`UUID().uuidString`): an
/// uppercase, hyphenated version 4 UUID such as `E621E1F8-C36C-495A-93FC-0C247A3E6E5F`.
///
/// The randomness comes from the standard library's per-process hash keys (seeded by the OS),
/// mixed with the time and a counter: unique and unpredictable enough for IDs, not for secrets.
pub fn new_id() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let count = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |elapsed| elapsed.as_nanos());
    let half = |salt: u64| {
        let mut hasher = RandomState::new().build_hasher();
        hasher.write_u64(salt);
        hasher.write_u64(count);
        hasher.write_u128(nanos);
        hasher.finish()
    };
    let mut bytes = [0u8; 16];
    bytes[..8].copy_from_slice(&half(0x9E37_79B9_7F4A_7C15).to_be_bytes());
    bytes[8..].copy_from_slice(&half(0xD1B5_4A32_D192_ED03).to_be_bytes());
    bytes[6] = (bytes[6] & 0x0F) | 0x40; // version 4
    bytes[8] = (bytes[8] & 0x3F) | 0x80; // RFC 4122 variant

    let mut id = String::with_capacity(36);
    for (index, byte) in bytes.iter().enumerate() {
        if matches!(index, 4 | 6 | 8 | 10) {
            id.push('-');
        }
        let _ = write!(id, "{byte:02X}");
    }
    id
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_uppercase_uuids() {
        let a = new_id();
        let b = new_id();
        assert_ne!(a, b);
        assert_eq!(a.len(), 36);
        assert_eq!(a.as_bytes()[14], b'4');
        assert!(a.chars().all(|c| c == '-' || c.is_ascii_digit() || c.is_ascii_uppercase()));
    }
}
