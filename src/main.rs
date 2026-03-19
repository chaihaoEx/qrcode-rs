mod config;
mod db;
mod middleware;
mod routes;
mod templates;

use actix_session::{storage::CookieSessionStore, SessionMiddleware};
use actix_web::cookie::Key;
use actix_web::{web, App, HttpRequest, HttpResponse, HttpServer};

fn load_rustls_config(cert_path: &str, key_path: &str) -> rustls::ServerConfig {
    use rustls::pki_types::PrivateKeyDer;
    use std::io::BufReader;

    let cert_file = &mut BufReader::new(
        std::fs::File::open(cert_path).expect(&format!("Failed to open cert file: {cert_path}")),
    );
    let key_file = &mut BufReader::new(
        std::fs::File::open(key_path).expect(&format!("Failed to open key file: {key_path}")),
    );

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

async fn redirect_to_https(
    req: HttpRequest,
    config: web::Data<config::Config>,
) -> HttpResponse {
    let host = req.connection_info().host().to_string();
    let host_without_port = host.split(':').next().unwrap_or(&host);
    let https_port = config.server.https_port.unwrap();
    let path = req.uri().path_and_query().map(|pq| pq.as_str()).unwrap_or("/");
    let target = if https_port == 443 {
        format!("https://{host_without_port}{path}")
    } else {
        format!("https://{host_without_port}:{https_port}{path}")
    };
    HttpResponse::MovedPermanently()
        .insert_header(("Location", target))
        .finish()
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init();

    // 支持 hash-password 子命令
    let args: Vec<String> = std::env::args().collect();
    if args.len() == 3 && args[1] == "hash-password" {
        let hash = bcrypt::hash(&args[2], 12).expect("Failed to hash password");
        println!("{hash}");
        return Ok(());
    }

    // 加载配置
    let config = config::Config::load("config.toml").expect("Failed to load config.toml");
    log::info!("Config loaded: context_path={}, public_host={}", config.server.context_path, config.server.public_host);
    let bind_addr = format!("{}:{}", config.server.host, config.server.port);

    // 初始化数据库连接池（可选，连接失败仅警告）
    let db_pool = match db::init_pool(&config.database.url).await {
        Ok(pool) => {
            log::info!("Database connected");
            Some(pool)
        }
        Err(e) => {
            log::warn!("Database connection failed: {e}. Continuing without database.");
            None
        }
    };

    // 初始化模板引擎
    let tera = templates::init_templates();
    log::info!("Template engine initialized");

    // Session 密钥
    let secret_key = Key::from(config.server.secret_key.as_bytes());

    let context_path = config.server.context_path.clone();

    // 判断是否启用 HTTPS
    let https_enabled = config.server.https_port.is_some()
        && config.server.tls_cert.is_some()
        && config.server.tls_key.is_some();

    let config_data = web::Data::new(config.clone());
    let tera_data = web::Data::new(tera);

    if https_enabled {
        let tls_config = load_rustls_config(
            config.server.tls_cert.as_ref().unwrap(),
            config.server.tls_key.as_ref().unwrap(),
        );
        let https_port = config.server.https_port.unwrap();
        let https_addr = format!("{}:{}", config.server.host, https_port);

        log::info!("Starting HTTPS server at https://{https_addr}{context_path}");
        log::info!("Starting HTTP redirect server at http://{bind_addr} -> https");

        // HTTPS 主服务器
        let config_data_clone = config_data.clone();
        let tera_data_clone = tera_data.clone();
        let context_path_clone = context_path.clone();
        let secret_key_clone = secret_key.clone();
        let db_pool_clone = db_pool.clone();

        let https_server = HttpServer::new(move || {
            let mut app = App::new()
                .wrap(middleware::AuthGuard { context_path: context_path_clone.clone() })
                .wrap(actix_web::middleware::Logger::default())
                .wrap(SessionMiddleware::builder(CookieSessionStore::default(), secret_key_clone.clone()).build())
                .app_data(config_data_clone.clone())
                .app_data(tera_data_clone.clone());

            if let Some(ref pool) = db_pool_clone {
                app = app.app_data(web::Data::new(pool.clone()));
            }

            app.configure(routes::configure(context_path_clone.clone()))
        })
        .bind_rustls_0_23(&https_addr, tls_config)?
        .run();

        // HTTP 跳转服务器
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
        log::info!("Starting server at http://{bind_addr}{context_path}");

        let mut server = HttpServer::new(move || {
            let mut app = App::new()
                .wrap(middleware::AuthGuard { context_path: context_path.clone() })
                .wrap(actix_web::middleware::Logger::default())
                .wrap(SessionMiddleware::builder(CookieSessionStore::default(), secret_key.clone()).build())
                .app_data(config_data.clone())
                .app_data(tera_data.clone());

            if let Some(ref pool) = db_pool {
                app = app.app_data(web::Data::new(pool.clone()));
            }

            app.configure(routes::configure(context_path.clone()))
        });

        server = server.bind(&bind_addr)?;
        server.run().await?;
    }

    Ok(())
}
