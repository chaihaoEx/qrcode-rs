pub mod auth;
pub mod qrcode;

use actix_files as fs;
use actix_web::web;

pub fn configure(context_path: String) -> impl FnOnce(&mut web::ServiceConfig) {
    move |cfg: &mut web::ServiceConfig| {
        cfg.service(
            web::scope(&context_path)
                .route("/", web::get().to(qrcode::list_page))
                .route("/create", web::get().to(qrcode::create_page))
                .route("/create", web::post().to(qrcode::create_handler))
                .route("/qrcode-image/{uuid}", web::get().to(qrcode::download_image))
                .route("/extract/{uuid}/{hash}", web::get().to(qrcode::extract_page))
                .route("/delete/{uuid}", web::post().to(qrcode::delete_handler))
                .route("/reset/{uuid}", web::post().to(qrcode::reset_handler))
                .route("/edit/{uuid}", web::get().to(qrcode::edit_page))
                .route("/edit/{uuid}", web::post().to(qrcode::edit_handler))
                .route("/logs/{uuid}", web::get().to(qrcode::extract_logs_page))
                .route("/login", web::get().to(auth::login_page))
                .route("/login", web::post().to(auth::login_handler))
                .route("/logout", web::get().to(auth::logout))
                .service(fs::Files::new("/static", "static")),
        );
    }
}
