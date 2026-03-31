//! 二维码管理系统 — 应用入口
//!
//! 基于 Actix-web 4 的二维码内容分发管理平台。
//! 支持二维码 CRUD、基于浏览器槽位的内容提取、多管理员角色、
//! AI 评论生成、审计日志和 HTTPS/TLS 加密。
//!
//! 请求处理链：
//! `Request → SessionMiddleware → Logger → AuthGuard → Route Handler → Tera Template → Response`

mod config;
mod csrf;
mod db;
mod middleware;
mod models;
mod rate_limit;
mod routes;
mod services;
mod templates;
mod utils;

use actix_session::{config::PersistentSession, storage::CookieSessionStore, SessionMiddleware};
use actix_web::cookie::{time::Duration as CookieDuration, Key, SameSite};
use actix_web::{web, App, HttpRequest, HttpResponse, HttpServer};

/// 加载 TLS/Rustls 配置。
///
/// 从 PEM 文件中读取证书链和私钥，构建 Rustls 服务端配置。
/// 文件不存在或格式错误时 panic。
fn load_rustls_config(cert_path: &str, key_path: &str) -> rustls::ServerConfig {
    use rustls::pki_types::PrivateKeyDer;
    use std::io::BufReader;

    let cert_file =
        &mut BufReader::new(std::fs::File::open(cert_path).expect("Failed to open TLS cert file"));
    let key_file =
        &mut BufReader::new(std::fs::File::open(key_path).expect("Failed to open TLS key file"));

    let certs: Vec<_> = rustls_pemfile::certs(cert_file)
        .collect::<Result<Vec<_>, _>>()
        .expect("Failed to parse certificate PEM");

    let key = rustls_pemfile::private_key(key_file)
        .expect("Failed to read private key PEM")
        .expect("No private key found in PEM file");

    rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, PrivateKeyDer::from(key))
        .expect("Failed to build TLS config")
}

/// HTTP→HTTPS 重定向处理器。
///
/// 当同时启用 HTTP 和 HTTPS 时，HTTP 服务器将所有请求
/// 通过 301 永久重定向到对应的 HTTPS 地址。
async fn redirect_to_https(req: HttpRequest, config: web::Data<config::Config>) -> HttpResponse {
    let host = req.connection_info().host().to_string();
    let host_without_port = host.split(':').next().unwrap_or(&host);
    let https_port = config.server.https_port.unwrap();
    let path = req
        .uri()
        .path_and_query()
        .map(|pq| pq.as_str())
        .unwrap_or("/");
    // 标准 443 端口无需显式指定
    let target = if https_port == 443 {
        format!("https://{host_without_port}{path}")
    } else {
        format!("https://{host_without_port}:{https_port}{path}")
    };
    HttpResponse::MovedPermanently()
        .insert_header(("Location", target))
        .finish()
}

/// 构建会话中间件。
///
/// 配置 Cookie 会话存储，设置安全属性：
/// - `HttpOnly` — 防止 JavaScript 访问
/// - `SameSite=Strict` — 防止 CSRF
/// - `Secure` — 仅在 HTTPS 下发送（由 `secure` 参数控制）
/// - TTL 8 小时
fn build_session_middleware(
    secret_key: Key,
    secure: bool,
) -> SessionMiddleware<CookieSessionStore> {
    SessionMiddleware::builder(CookieSessionStore::default(), secret_key)
        .cookie_secure(secure)
        .cookie_same_site(SameSite::Strict)
        .cookie_http_only(true)
        .session_lifecycle(PersistentSession::default().session_ttl(CookieDuration::hours(8)))
        .build()
}

/// 应用主入口函数。
///
/// 启动流程：
/// 1. 处理 `hash-password` 子命令（用于生成 bcrypt 密码哈希）
/// 2. 加载配置文件
/// 3. 初始化数据库连接池
/// 4. 初始化模板引擎
/// 5. 根据配置启动 HTTPS+HTTP 重定向 或 纯 HTTP 服务器
#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init();

    // ---- CLI 子命令处理 ----
    let args: Vec<String> = std::env::args().collect();
    if args.len() == 3 && args[1] == "hash-password" {
        let hash = bcrypt::hash(&args[2], 12).expect("Failed to hash password");
        println!("{hash}");
        return Ok(());
    }

    // ---- 加载配置 ----
    let config = config::Config::load("config.toml").expect("Failed to load config.toml");
    log::info!(
        "Config loaded: context_path={}, public_host={}",
        config.server.context_path,
        config.server.public_host
    );
    let bind_addr = format!("{}:{}", config.server.host, config.server.port);

    // ---- 初始化数据库连接池 ----
    let db_pool = db::init_pool(
        &config.database.url,
        config.database.max_connections,
        &config.database.timezone,
    )
    .await
    .expect("Database connection required. Check database config and restart.");
    log::info!(
        "Database connected (max_connections={}, timezone={})",
        config.database.max_connections,
        config.database.timezone
    );

    // ---- 初始化模板引擎 ----
    let tera = templates::init_templates();
    log::info!("Template engine initialized");

    // ---- 准备共享状态 ----
    let secret_key = Key::from(config.server.secret_key.as_bytes());
    let context_path = config.server.context_path.clone();

    let https_enabled = config.server.https_port.is_some()
        && config.server.tls_cert.is_some()
        && config.server.tls_key.is_some();

    let config_data = web::Data::new(config.clone());
    let tera_data = web::Data::new(tera);
    let pool_data = web::Data::new(db_pool);
    // 登录限流：每个 IP 5 分钟内最多 10 次尝试
    let rate_limiter = web::Data::new(rate_limit::RateLimiter::new(10, 300));

    // ---- 启动服务器 ----
    if https_enabled {
        let tls_config = load_rustls_config(
            config.server.tls_cert.as_ref().unwrap(),
            config.server.tls_key.as_ref().unwrap(),
        );
        let https_port = config.server.https_port.unwrap();
        let https_addr = format!("{}:{}", config.server.host, https_port);

        log::info!("Starting HTTPS server at https://{https_addr}{context_path}");
        log::info!("Starting HTTP redirect server at http://{bind_addr} -> https");

        let config_data_clone = config_data.clone();
        let tera_data_clone = tera_data.clone();
        let pool_data_clone = pool_data.clone();
        let rate_limiter_clone = rate_limiter.clone();
        let context_path_clone = context_path.clone();
        let secret_key_clone = secret_key.clone();

        // HTTPS 主服务器
        let https_server = HttpServer::new(move || {
            App::new()
                .wrap(middleware::AuthGuard {
                    context_path: context_path_clone.clone(),
                })
                .wrap(actix_web::middleware::Logger::default())
                .wrap(build_session_middleware(secret_key_clone.clone(), true))
                .app_data(config_data_clone.clone())
                .app_data(tera_data_clone.clone())
                .app_data(pool_data_clone.clone())
                .app_data(rate_limiter_clone.clone())
                .app_data(web::JsonConfig::default().limit(4096))
                .app_data(web::FormConfig::default().limit(65536))
                .configure(routes::configure(context_path_clone.clone()))
        })
        .bind_rustls_0_23(&https_addr, tls_config)?
        .run();

        // HTTP 重定向服务器
        let config_data_redirect = config_data.clone();
        let http_server = HttpServer::new(move || {
            App::new()
                .app_data(config_data_redirect.clone())
                .default_service(web::to(redirect_to_https))
        })
        .bind(&bind_addr)?
        .run();

        tokio::try_join!(https_server, http_server)?;
    } else {
        // 纯 HTTP 模式
        log::info!("Starting server at http://{bind_addr}{context_path}");

        let server = HttpServer::new(move || {
            App::new()
                .wrap(middleware::AuthGuard {
                    context_path: context_path.clone(),
                })
                .wrap(actix_web::middleware::Logger::default())
                .wrap(build_session_middleware(secret_key.clone(), false))
                .app_data(config_data.clone())
                .app_data(tera_data.clone())
                .app_data(pool_data.clone())
                .app_data(rate_limiter.clone())
                .app_data(web::JsonConfig::default().limit(4096))
                .app_data(web::FormConfig::default().limit(65536))
                .configure(routes::configure(context_path.clone()))
        });

        server.bind(&bind_addr)?.run().await?;
    }

    Ok(())
}
