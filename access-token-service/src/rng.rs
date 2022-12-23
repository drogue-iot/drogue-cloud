use drogue_cloud_service_api::token::CreatedAccessToken;
use rand::{distributions::Alphanumeric, thread_rng, Rng};
use sha3::Digest;

const PREFIX_LENGTH: usize = 6;
const KEY_LENGTH: usize = 30;

const MIN_TOKEN_LENGTH: usize = 4 + PREFIX_LENGTH + 1 + KEY_LENGTH;
const MAX_TOKEN_LENGTH: usize = MIN_TOKEN_LENGTH + 6 /*max crc*/;

fn generate_key() -> String {
    thread_rng()
        .sample_iter(&Alphanumeric)
        .take(KEY_LENGTH)
        .map(char::from)
        .collect()
}

fn generate_prefix() -> String {
    let mut s = String::with_capacity(MAX_TOKEN_LENGTH);
    s.push_str("drg_");
    s.extend(
        thread_rng()
            .sample_iter(&Alphanumeric)
            .take(PREFIX_LENGTH)
            .map(char::from),
    );
    s
}

pub fn hash_token(token: &str) -> String {
    format!("{:x}", sha3::Sha3_512::digest(token.as_bytes()))
}

fn serialize_token(prefix: String, key: String) -> (CreatedAccessToken, String) {
    let token = format!("{}_{}", prefix, key);

    let crc = crc::crc32::checksum_ieee(token.as_bytes());
    let crc = base62::encode(crc);

    let token = token + &crc;

    let hashed = hash_token(&token);

    (CreatedAccessToken { prefix, token }, hashed)
}

/// Create a new (random) AccessToken.
///
/// It will return a tuple, consisting of the actual Access Token as well as the hashed version.
pub fn generate_access_token() -> (CreatedAccessToken, String) {
    let prefix = generate_prefix();
    let raw_key = generate_key();

    serialize_token(prefix, raw_key)
}

pub fn is_valid(token: &str) -> Option<&str> {
    let len = token.len();

    if !(MIN_TOKEN_LENGTH..=MAX_TOKEN_LENGTH).contains(&len) {
        return None;
    }

    let prefix_1 = &token[0..4];

    if prefix_1 != "drg_" {
        return None;
    }

    let key = &token[0..MIN_TOKEN_LENGTH];
    let expected_crc = &token[MIN_TOKEN_LENGTH..];

    let crc = crc::crc32::update(0u32, &crc::crc32::IEEE_TABLE, key.as_bytes());
    let actual_crc = base62::encode(crc);

    if expected_crc != actual_crc {
        return None;
    }

    Some(&token[0..PREFIX_LENGTH + 4])
}

#[cfg(test)]
mod test {

    use super::*;

    #[test]
    fn test_token() {
        let prefix = "drg_012345".into();
        let key = "012345678901234567890123456789".into();
        let token = serialize_token(prefix, key);

        assert_eq!("drg_012345", token.0.prefix);
        assert_eq!(
            "drg_012345_01234567890123456789012345678920BetF",
            token.0.token
        );
        assert_eq!("d08a9d562a28816d47c875fc34223031b31e3e8a311244ba41cda71497a32315c20293e41a044b32688b2b4bcff960f38e19144001b235888d3ce039053e5962", token.1)
    }

    #[test]
    fn test_valid() {
        assert_eq!(
            Some("drg_012345"),
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
            let token = generate_access_token();
            assert!(matches!(is_valid(&token.0.token), Some(_)))
        }
    }
}
