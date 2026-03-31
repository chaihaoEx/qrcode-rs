//! 应用配置模块
//!
//! 从 `config.toml` 文件反序列化为类型化的配置结构体。
//! 支持服务器、管理员、数据库和 AI 四大配置块。
//! 启动时进行关键参数校验（如 `secret_key` 最小长度）。

use serde::Deserialize;
use std::fs;

/// 应用顶层配置，包含所有子配置块。
///
/// 对应 `config.toml` 文件的根结构，参见 `config.example.toml` 获取完整示例。
#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    /// 服务器相关配置
    pub server: ServerConfig,
    /// 超级管理员账户配置（配置文件定义，非数据库）
    pub admin: AdminConfig,
    /// 数据库连接配置
    pub database: DatabaseConfig,
    /// AI 评论生成配置（可选，未配置时禁用 AI 功能）
    #[serde(default)]
    pub ai: Option<AiConfig>,
}

/// AI 服务配置，用于连接 OpenAI 兼容的 API 生成评论内容。
#[derive(Debug, Deserialize, Clone)]
pub struct AiConfig {
    /// API 基础 URL（如 `https://api.deepseek.com`）
    pub base_url: String,
    /// API 密钥
    pub api_key: String,
    /// 模型名称，默认为 `"deepseek-chat"`
    #[serde(default = "default_model")]
    pub model: String,
}

/// 返回默认 AI 模型名称。
fn default_model() -> String {
    "deepseek-chat".to_string()
}

/// 服务器配置，定义监听地址、安全参数和 TLS 设置。
#[derive(Debug, Deserialize, Clone)]
pub struct ServerConfig {
    /// 监听地址（如 `"127.0.0.1"`）
    pub host: String,
    /// HTTP 监听端口
    pub port: u16,
    /// 会话签名密钥，必须 ≥ 64 个字符
    pub secret_key: String,
    /// 虚拟目录前缀（如 `"/qrcode"`），影响所有路由和静态资源路径
    #[serde(default)]
    pub context_path: String,
    /// 公开访问地址，用于生成二维码图片中的提取 URL
    pub public_host: String,
    /// HMAC 签名盐值，用于生成提取 URL 中的哈希
    pub extract_salt: String,
    /// HTTPS 监听端口（可选，配置后启用 HTTPS）
    #[serde(default)]
    pub https_port: Option<u16>,
    /// TLS 证书文件路径
    #[serde(default)]
    pub tls_cert: Option<String>,
    /// TLS 私钥文件路径
    #[serde(default)]
    pub tls_key: Option<String>,
    /// 是否接受旧版 8 字符 HMAC 哈希（默认 true，向后兼容）
    #[serde(default)]
    pub legacy_hash_support: Option<bool>,
}

/// 超级管理员配置，通过配置文件定义。
///
/// 超级管理员拥有最高权限，包括审计日志查看和用户管理。
/// 与数据库中的 `admin_users` 表相互独立。
#[derive(Debug, Deserialize, Clone)]
pub struct AdminConfig {
    /// 超级管理员用户名
    pub username: String,
    /// bcrypt 密码哈希值
    pub password_hash: String,
}

/// 数据库连接配置。
#[derive(Debug, Deserialize, Clone)]
pub struct DatabaseConfig {
    /// MySQL 连接字符串（如 `"mysql://user:pass@host:port/dbname"`）
    pub url: String,
    /// 连接池最大连接数，默认 10
    #[serde(default = "default_max_connections")]
    pub max_connections: u32,
    /// 会话时区，默认 `"+08:00"`
    #[serde(default = "default_timezone")]
    pub timezone: String,
}

/// 返回默认最大连接数。
fn default_max_connections() -> u32 {
    10
}

/// 返回默认时区。
fn default_timezone() -> String {
    "+08:00".to_string()
}

impl Config {
    /// 从指定路径加载并解析配置文件。
    ///
    /// 读取 TOML 文件内容并反序列化为 `Config` 结构体，同时执行以下校验：
    /// - 去除 `context_path` 尾部斜杠
    /// - 验证 `secret_key` 长度 ≥ 64 个字符
    ///
    /// # 参数
    /// - `path` - 配置文件路径（如 `"config.toml"`）
    pub fn load(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        log::debug!("Loading config from: {path}");
        let content = fs::read_to_string(path)?;
        let mut config: Config = toml::from_str(&content)?;
        // 去除尾部斜杠
        config.server.context_path = config.server.context_path.trim_end_matches('/').to_string();

        if config.server.secret_key.len() < 64 {
            return Err("secret_key must be at least 64 characters".into());
        }

        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn valid_toml(secret_key: &str, context_path: &str) -> String {
        format!(
            r#"
[server]
host = "127.0.0.1"
port = 8080
secret_key = "{secret_key}"
context_path = "{context_path}"
public_host = "http://localhost:8080"
extract_salt = "test-salt"

[admin]
username = "admin"
password_hash = "$2b$12$test"

[database]
url = "mysql://user:pass@localhost/db"
"#
        )
    }

    fn write_config(content: &str) -> tempfile::NamedTempFile {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        f
    }

    #[test]
    fn test_default_model() {
        assert_eq!(default_model(), "deepseek-chat");
    }

    #[test]
    fn test_default_max_connections() {
        assert_eq!(default_max_connections(), 10);
    }

    #[test]
    fn test_default_timezone() {
        assert_eq!(default_timezone(), "+08:00");
    }

    #[test]
    fn test_load_valid_config() {
        let key = "a".repeat(64);
        let f = write_config(&valid_toml(&key, ""));
        let config = Config::load(f.path().to_str().unwrap()).unwrap();
        assert_eq!(config.server.host, "127.0.0.1");
        assert_eq!(config.server.port, 8080);
        assert_eq!(config.database.max_connections, 10);
        assert_eq!(config.database.timezone, "+08:00");
        assert!(config.ai.is_none());
    }

    #[test]
    fn test_load_config_trims_context_path() {
        let key = "a".repeat(64);
        let f = write_config(&valid_toml(&key, "/app/"));
        let config = Config::load(f.path().to_str().unwrap()).unwrap();
        assert_eq!(config.server.context_path, "/app");
    }

    #[test]
    fn test_load_config_short_secret_key() {
        let key = "a".repeat(63);
        let f = write_config(&valid_toml(&key, ""));
        let err = Config::load(f.path().to_str().unwrap()).unwrap_err();
        assert!(err.to_string().contains("secret_key"));
    }

    #[test]
    fn test_load_config_missing_file() {
        assert!(Config::load("/nonexistent/config.toml").is_err());
    }
}
