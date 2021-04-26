use crate::data::ApiKeyCreated;
use rand::{distributions::Alphanumeric, thread_rng, Rng};
use sha3::Digest;

const PREFIX_LENGTH: usize = 6;
const KEY_LENGTH: usize = 30;

const MIN_KEY_LENGTH: usize = 4 + PREFIX_LENGTH + 1 + KEY_LENGTH;
const MAX_KEY_LENGTH: usize = MIN_KEY_LENGTH + 6 /*max crc*/;

fn generate_key() -> String {
    thread_rng()
        .sample_iter(&Alphanumeric)
        .take(KEY_LENGTH)
        .map(char::from)
        .collect()
}

fn generate_prefix() -> String {
    let mut s = String::with_capacity(MAX_KEY_LENGTH);
    s.push_str("drg_");
    s.extend(
        thread_rng()
            .sample_iter(&Alphanumeric)
            .take(PREFIX_LENGTH)
            .map(char::from),
    );
    s
}

pub fn hash_key(key: &str) -> String {
    format!("{:x}", sha3::Sha3_512::digest(key.as_bytes()))
}

fn serialize_key(prefix: String, key: String) -> (ApiKeyCreated, String) {
    let key = format!("{}_{}", prefix, key);

    let crc = crc::crc32::checksum_ieee(key.as_bytes());
    let crc = base62::encode(crc);

    let hashed = hash_key(&key);

    (
        ApiKeyCreated {
            prefix,
            key: key + &crc,
        },
        hashed,
    )
}

/// Create a new (random) API key.
///
/// It will return a tuple, consisting of the actual API key as well as the hashed version.
pub fn generate_api_key() -> (ApiKeyCreated, String) {
    let prefix = generate_prefix();
    let raw_key = generate_key();

    serialize_key(prefix, raw_key)
}

pub fn is_valid(key: &str) -> Option<&str> {
    let len = key.len();

    if !(MIN_KEY_LENGTH..=MAX_KEY_LENGTH).contains(&len) {
        return None;
    }

    let prefix_1 = &key[0..4];

    if prefix_1 != "drg_" {
        return None;
    }

    let key_part = &key[0..MIN_KEY_LENGTH];
    let expected_crc = &key[MIN_KEY_LENGTH..];

    let crc = crc::crc32::update(0u32, &crc::crc32::IEEE_TABLE, key_part.as_bytes());
    let actual_crc = base62::encode(crc);

    if expected_crc != actual_crc {
        return None;
    }

    Some(&key[4..4 + PREFIX_LENGTH])
}

#[cfg(test)]
mod test {

    use super::*;

    #[test]
    fn test_key() {
        let prefix = "drg_012345".into();
        let raw_key = "012345678901234567890123456789".into();
        let key = serialize_key(prefix, raw_key);

        assert_eq!("drg_012345", key.0.prefix);
        assert_eq!("drg_012345_01234567890123456789012345678920BetF", key.0.key);
        assert_eq!("dcdd17f2fef5b61e27a8887881c47175f3449898230defe8eb45face1d131968a6db191fca131f6fb4c26deb5a62d1b08f7768d6df3339144f1f91914a521a0f", key.1)
    }

    #[test]
    fn test_valid() {
        assert_eq!(
            Some("012345"),
            is_valid("drg_012345_01234567890123456789012345678920BetF")
        );
    }

    #[test]
    fn test_invalid_empty() {
        assert_eq!(None, is_valid(""));
    }

    #[test]
    fn test_invalid_missing_content() {
        assert_eq!(None, is_valid("drg_"));
    }

    #[test]
    fn test_invalid_too_much_content() {
        assert_eq!(
            None,
            is_valid("drg_012345_01234567890123456789012345678920BetF_way_too_much_content")
        );
    }

    #[test]
    fn test_invalid_invalid_prefix() {
        assert_eq!(
            None,
            is_valid("abc_012345_01234567890123456789012345678920BetF")
        );
    }

    #[test]
    fn test_invalid_invalid_checksum() {
        assert_eq!(
            None,
            is_valid("drg_012345_01234567890123456789012345678920BetE")
        );
    }

    #[test]
    fn test_valid_generator() {
        for _ in 0..1000 {
            let key = generate_api_key();
            assert!(matches!(is_valid(&key.0.key), Some(_)))
        }
    }
}
