pub mod auth;

use actix_files as fs;
use actix_web::web;

pub fn configure(context_path: String) -> impl FnOnce(&mut web::ServiceConfig) {
    move |cfg: &mut web::ServiceConfig| {
        cfg.service(
            web::scope(&context_path)
                .route("/", web::get().to(auth::welcome_page))
                .route("/login", web::get().to(auth::login_page))
                .route("/login", web::post().to(auth::login_handler))
                .route("/logout", web::get().to(auth::logout))
                .service(fs::Files::new("/static", "static")),
        );
    }
}
