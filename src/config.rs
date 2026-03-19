use serde::Deserialize;
use std::fs;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub server: ServerConfig,
    pub admin: AdminConfig,
    pub database: DatabaseConfig,
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
}

#[derive(Debug, Deserialize, Clone)]
pub struct AdminConfig {
    pub username: String,
    pub password_hash: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DatabaseConfig {
    pub url: String,
}

impl Config {
    pub fn load(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        log::debug!("Loading config from: {path}");
        let content = fs::read_to_string(path)?;
        let mut config: Config = toml::from_str(&content)?;
        // 去除尾部斜杠
        config.server.context_path = config.server.context_path.trim_end_matches('/').to_string();
        Ok(config)
    }
}
