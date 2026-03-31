//! 路由配置模块
//!
//! 注册所有 HTTP 路由并绑定到对应的处理函数。
//! 所有路由以 `context_path` 为前缀，支持虚拟目录部署。

pub mod admin;
pub mod auth;
pub mod extract;

use actix_files as fs;
use actix_web::web;

/// 配置所有路由，返回闭包供 `App::configure()` 使用。
///
/// 注册的路由分为三组：
/// - **管理路由**（需认证）：二维码 CRUD、AI 生成、用户管理、审计日志、密码修改
/// - **公开路由**（无需认证）：提取页面和领取接口
/// - **认证路由**：登录、登出
/// - **静态资源**：`/static/` 目录，启用 ETag 和 Last-Modified 缓存
pub fn configure(context_path: String) -> impl FnOnce(&mut web::ServiceConfig) {
    move |cfg: &mut web::ServiceConfig| {
        cfg.service(
            web::scope(&context_path)
                // ---- 管理路由 ----
                .route("/", web::get().to(admin::list_page))
                .route("/create", web::get().to(admin::create_page))
                .route("/create", web::post().to(admin::create_handler))
                .route("/qrcode-image/{uuid}", web::get().to(admin::download_image))
                .route(
                    "/extract/{uuid}/{hash}",
                    web::get().to(extract::extract_page),
                )
                .route(
                    "/extract/{uuid}/{hash}/claim",
                    web::post().to(extract::extract_claim_handler),
                )
                .route("/delete/{uuid}", web::post().to(admin::delete_handler))
                .route("/reset/{uuid}", web::post().to(admin::reset_handler))
                .route("/edit/{uuid}", web::get().to(admin::edit_page))
                .route("/edit/{uuid}", web::post().to(admin::edit_handler))
                .route("/logs/{uuid}", web::get().to(admin::extract_logs_page))
                .route("/audit-logs", web::get().to(admin::audit_logs_page))
                .route("/ai-generate", web::get().to(admin::ai_generate_page))
                .route("/ai-generate", web::post().to(admin::ai_generate_handler))
                .route(
                    "/ai-generate/create",
                    web::post().to(admin::ai_create_handler),
                )
                .route("/users", web::get().to(admin::users_page))
                .route("/users/create", web::post().to(admin::create_user_handler))
                .route("/users/toggle", web::post().to(admin::toggle_user_handler))
                .route(
                    "/change-password",
                    web::get().to(admin::change_password_page),
                )
                .route(
                    "/change-password",
                    web::post().to(admin::change_password_handler),
                )
                // ---- 认证路由 ----
                .route("/login", web::get().to(auth::login_page))
                .route("/login", web::post().to(auth::login_handler))
                .route("/logout", web::get().to(auth::logout))
                // ---- 静态资源 ----
                .service(
                    fs::Files::new("/static", "static")
                        .use_etag(true)
                        .use_last_modified(true),
                ),
        );
    }
}
