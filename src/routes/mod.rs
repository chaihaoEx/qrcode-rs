pub mod admin;
pub mod auth;
pub mod extract;

use actix_files as fs;
use actix_web::web;

pub fn configure(context_path: String) -> impl FnOnce(&mut web::ServiceConfig) {
    move |cfg: &mut web::ServiceConfig| {
        cfg.service(
            web::scope(&context_path)
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
                .route("/login", web::get().to(auth::login_page))
                .route("/login", web::post().to(auth::login_handler))
                .route("/logout", web::get().to(auth::logout))
                .service(
                    fs::Files::new("/static", "static")
                        .use_etag(true)
                        .use_last_modified(true),
                ),
        );
    }
}
