//! HMAC 签名与验证模块
//!
//! 基于 HMAC-SHA256 生成和验证二维码提取 URL 中的哈希签名。
//! 使用 `subtle` 库进行恒定时间比较，防止时序攻击。
//! 支持新版 16 字符哈希和旧版 8 字符哈希的向后兼容。

/// 生成提取 URL 的 HMAC-SHA256 签名哈希。
///
/// 使用 `uuid + salt` 计算 HMAC-SHA256，取前 8 字节（64 位）
/// 输出为 16 个十六进制字符的字符串。
///
/// # 参数
/// - `uuid` - 二维码的唯一标识
/// - `salt` - HMAC 签名盐值（来自配置 `server.extract_salt`）
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

/// 验证提取 URL 中的哈希签名，使用恒定时间比较防止时序攻击。
///
/// 支持两种哈希长度：
/// - 16 字符（新版，64 位安全性）
/// - 8 字符（旧版，仅在 `legacy_support` 为 `true` 时接受）
///
/// # 参数
/// - `uuid` - 二维码的唯一标识
/// - `hash` - URL 中的哈希值
/// - `salt` - HMAC 签名盐值
/// - `legacy_support` - 是否接受旧版 8 字符哈希
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

    const TEST_UUID: &str = "test-uuid";
    const TEST_UUID_1: &str = "uuid-1";
    const TEST_UUID_2: &str = "uuid-2";
    const TEST_SALT: &str = "test-salt-not-for-production";

    #[test]
    fn test_generate_extract_hash_deterministic() {
        let h1 = generate_extract_hash(TEST_UUID, TEST_SALT);
        let h2 = generate_extract_hash(TEST_UUID, TEST_SALT);
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 16);
    }

    #[test]
    fn test_generate_extract_hash_different_inputs() {
        let h1 = generate_extract_hash(TEST_UUID_1, TEST_SALT);
        let h2 = generate_extract_hash(TEST_UUID_2, TEST_SALT);
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_verify_extract_hash_correct() {
        let hash = generate_extract_hash(TEST_UUID, TEST_SALT);
        assert!(verify_extract_hash(TEST_UUID, &hash, TEST_SALT, false));
    }

    #[test]
    fn test_verify_extract_hash_wrong() {
        assert!(!verify_extract_hash(
            TEST_UUID,
            "0000000000000000",
            TEST_SALT,
            false
        ));
    }

    #[test]
    fn test_verify_extract_hash_legacy() {
        let full_hash = generate_extract_hash(TEST_UUID, TEST_SALT);
        let legacy = &full_hash[..8];
        assert!(verify_extract_hash(TEST_UUID, legacy, TEST_SALT, true));
        assert!(!verify_extract_hash(TEST_UUID, legacy, TEST_SALT, false));
    }

    #[test]
    fn test_verify_extract_hash_wrong_length() {
        assert!(!verify_extract_hash(TEST_UUID, "abc", TEST_SALT, true));
        assert!(!verify_extract_hash(TEST_UUID, "", TEST_SALT, true));
        assert!(!verify_extract_hash(
            TEST_UUID,
            "0000000000000000000000",
            TEST_SALT,
            true
        ));
    }
}
