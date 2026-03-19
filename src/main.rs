mod config;
mod db;
mod middleware;
mod routes;
mod templates;

use actix_session::{storage::CookieSessionStore, SessionMiddleware};
use actix_web::cookie::Key;
use actix_web::{web, App, HttpServer};

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

    let config_data = web::Data::new(config);
    let tera_data = web::Data::new(tera);

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
    server.run().await
}
