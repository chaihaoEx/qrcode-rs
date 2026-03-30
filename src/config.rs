use serde::Deserialize;
use std::fs;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub server: ServerConfig,
    pub admin: AdminConfig,
    pub database: DatabaseConfig,
    #[serde(default)]
    pub ai: Option<AiConfig>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AiConfig {
    pub base_url: String,
    pub api_key: String,
    #[serde(default = "default_model")]
    pub model: String,
}

fn default_model() -> String {
    "deepseek-chat".to_string()
}

#[derive(Debug, Deserialize, Clone)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub secret_key: String,
    #[serde(default)]
    pub context_path: String,
    pub public_host: String,
    pub extract_salt: String,
    #[serde(default)]
    pub https_port: Option<u16>,
    #[serde(default)]
    pub tls_cert: Option<String>,
    #[serde(default)]
    pub tls_key: Option<String>,
    /// Whether to accept legacy 8-char HMAC hashes (default: true for backward compatibility)
    #[serde(default)]
    pub legacy_hash_support: Option<bool>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AdminConfig {
    pub username: String,
    pub password_hash: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DatabaseConfig {
    pub url: String,
    #[serde(default = "default_max_connections")]
    pub max_connections: u32,
    #[serde(default = "default_timezone")]
    pub timezone: String,
}

fn default_max_connections() -> u32 {
    10
}

fn default_timezone() -> String {
    "+08:00".to_string()
}

impl Config {
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
