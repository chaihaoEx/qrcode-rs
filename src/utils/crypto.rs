/// Generate HMAC-SHA256 hash (first 8 bytes = 16 hex chars)
pub fn generate_extract_hash(uuid: &str, salt: &str) -> String {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    use std::fmt::Write;

    let mut mac = Hmac::<Sha256>::new_from_slice(salt.as_bytes()).unwrap();
    mac.update(uuid.as_bytes());
    let result = mac.finalize().into_bytes();
    let mut hex = String::with_capacity(16);
    for b in &result[..8] {
        let _ = write!(hex, "{:02x}", b);
    }
    hex
}

/// Verify extract hash with constant-time comparison and optional legacy (8-char) support
pub fn verify_extract_hash(uuid: &str, hash: &str, salt: &str, legacy_support: bool) -> bool {
    use subtle::ConstantTimeEq;

    let expected = generate_extract_hash(uuid, salt);

    // New 16-char hash
    if hash.len() == 16 {
        return expected.as_bytes().ct_eq(hash.as_bytes()).into();
    }

    // Legacy 8-char hash fallback
    if legacy_support && hash.len() == 8 {
        return expected[..8].as_bytes().ct_eq(hash.as_bytes()).into();
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_extract_hash_deterministic() {
        let h1 = generate_extract_hash("test-uuid", "salt");
        let h2 = generate_extract_hash("test-uuid", "salt");
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 16);
    }

    #[test]
    fn test_generate_extract_hash_different_inputs() {
        let h1 = generate_extract_hash("uuid-1", "salt");
        let h2 = generate_extract_hash("uuid-2", "salt");
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_verify_extract_hash_correct() {
        let hash = generate_extract_hash("test-uuid", "salt");
        assert!(verify_extract_hash("test-uuid", &hash, "salt", false));
    }

    #[test]
    fn test_verify_extract_hash_wrong() {
        assert!(!verify_extract_hash(
            "test-uuid",
            "0000000000000000",
            "salt",
            false
        ));
    }

    #[test]
    fn test_verify_extract_hash_legacy() {
        let full_hash = generate_extract_hash("test-uuid", "salt");
        let legacy = &full_hash[..8];
        assert!(verify_extract_hash("test-uuid", legacy, "salt", true));
        assert!(!verify_extract_hash("test-uuid", legacy, "salt", false));
    }

    #[test]
    fn test_verify_extract_hash_wrong_length() {
        assert!(!verify_extract_hash("test-uuid", "abc", "salt", true));
        assert!(!verify_extract_hash("test-uuid", "", "salt", true));
        assert!(!verify_extract_hash(
            "test-uuid",
            "0000000000000000000000",
            "salt",
            true
        ));
    }
}
