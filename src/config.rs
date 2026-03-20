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
