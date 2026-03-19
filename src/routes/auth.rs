use actix_session::Session;
use actix_web::{web, HttpRequest, HttpResponse};
use serde::Deserialize;
use tera::{Context, Tera};

use crate::config::Config;

fn get_client_ip(req: &HttpRequest) -> String {
    req.connection_info()
        .realip_remote_addr()
        .unwrap_or("unknown")
        .to_string()
}

#[derive(Deserialize)]
pub struct LoginForm {
    pub username: String,
    pub password: String,
}

pub async fn login_page(
    tmpl: web::Data<Tera>,
    session: Session,
    req: HttpRequest,
    config: web::Data<Config>,
) -> HttpResponse {
    let base = &config.server.context_path;

    if session.get::<String>("user").unwrap_or(None).is_some() {
        return HttpResponse::Found()
            .insert_header(("Location", format!("{base}/")))
            .finish();
    }

    let mut ctx = Context::new();
    ctx.insert("base", base);
    let query = req.query_string();
    if query.contains("error=1") {
        ctx.insert("error", &"用户名或密码错误");
    }

    match tmpl.render("login.html", &ctx) {
        Ok(rendered) => HttpResponse::Ok().content_type("text/html").body(rendered),
        Err(e) => {
            log::warn!("Template render failed: login.html, error={e}");
            HttpResponse::InternalServerError().body("Internal Server Error")
        }
    }
}

pub async fn login_handler(
    form: web::Form<LoginForm>,
    session: Session,
    config: web::Data<Config>,
    req: HttpRequest,
) -> HttpResponse {
    let base = &config.server.context_path;
    let client_ip = get_client_ip(&req);

    if form.username == config.admin.username
        && bcrypt::verify(&form.password, &config.admin.password_hash).unwrap_or(false)
    {
        if let Err(e) = session.insert("user", &form.username) {
            log::warn!("Session insert failed: error={e}");
            return HttpResponse::InternalServerError().body("Session error");
        }
        log::info!("Login success: user={}, ip={client_ip}", form.username);
        HttpResponse::Found()
            .insert_header(("Location", format!("{base}/")))
            .finish()
    } else {
        log::warn!("Login failed: user={}, ip={client_ip}", form.username);
        HttpResponse::Found()
            .insert_header(("Location", format!("{base}/login?error=1")))
            .finish()
    }
}

pub async fn logout(session: Session, config: web::Data<Config>) -> HttpResponse {
    let base = &config.server.context_path;
    let username = session.get::<String>("user").unwrap_or(None).unwrap_or_default();
    log::info!("Logout: user={username}");
    session.purge();
    HttpResponse::Found()
        .insert_header(("Location", format!("{base}/login")))
        .finish()
}
