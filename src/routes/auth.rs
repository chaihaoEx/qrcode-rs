use actix_session::Session;
use actix_web::{web, HttpRequest, HttpResponse};
use serde::Deserialize;
use tera::{Context, Tera};

use crate::config::Config;
use crate::csrf;
use crate::rate_limit::RateLimiter;
use crate::services;

fn get_client_ip(req: &HttpRequest) -> String {
    req.connection_info()
        .realip_remote_addr()
        .unwrap_or("unknown")
        .to_string()
}

const MAX_USERNAME_LEN: usize = 100;
const MAX_PASSWORD_LEN: usize = 200;

#[derive(Deserialize)]
pub struct LoginForm {
    pub username: String,
    pub password: String,
    pub csrf_token: String,
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

    let csrf_token = csrf::ensure_csrf_token(&session);

    let mut ctx = Context::new();
    ctx.insert("base", base);
    ctx.insert("csrf_token", &csrf_token);
    let query = req.query_string();
    if query.contains("error=1") {
        ctx.insert("error", &"用户名或密码错误");
    } else if query.contains("error=rate") {
        ctx.insert("error", &"登录尝试过于频繁，请稍后再试");
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
    rate_limiter: web::Data<RateLimiter>,
    pool: web::Data<sqlx::MySqlPool>,
) -> HttpResponse {
    let base = &config.server.context_path;
    let client_ip = get_client_ip(&req);

    if form.username.len() > MAX_USERNAME_LEN || form.password.len() > MAX_PASSWORD_LEN {
        return HttpResponse::Found()
            .insert_header(("Location", format!("{base}/login?error=1")))
            .finish();
    }

    if !csrf::validate_csrf_token(&session, &form.csrf_token) {
        return HttpResponse::Found()
            .insert_header(("Location", format!("{base}/login?error=1")))
            .finish();
    }

    if !rate_limiter.check_and_increment(&client_ip) {
        log::warn!("Login rate limited");
        return HttpResponse::Found()
            .insert_header(("Location", format!("{base}/login?error=rate")))
            .finish();
    }

    if form.username == config.admin.username
        && bcrypt::verify(&form.password, &config.admin.password_hash).unwrap_or(false)
    {
        if let Err(e) = session.insert("user", &form.username) {
            log::warn!("Session insert failed: error={e}");
            return HttpResponse::InternalServerError().body("Session error");
        }
        rate_limiter.reset(&client_ip);
        services::audit::log_action(pool.get_ref(), &form.username, "login_success", None, None, &client_ip).await;
        HttpResponse::Found()
            .insert_header(("Location", format!("{base}/")))
            .finish()
    } else {
        services::audit::log_action(pool.get_ref(), &form.username, "login_failed", None, None, &client_ip).await;
        HttpResponse::Found()
            .insert_header(("Location", format!("{base}/login?error=1")))
            .finish()
    }
}

pub async fn logout(
    session: Session,
    config: web::Data<Config>,
    pool: web::Data<sqlx::MySqlPool>,
    req: HttpRequest,
) -> HttpResponse {
    let base = &config.server.context_path;
    let username = session
        .get::<String>("user")
        .unwrap_or(None)
        .unwrap_or_default();
    let client_ip = get_client_ip(&req);
    services::audit::log_action(pool.get_ref(), &username, "logout", None, None, &client_ip).await;
    session.purge();
    HttpResponse::Found()
        .insert_header(("Location", format!("{base}/login")))
        .finish()
}
