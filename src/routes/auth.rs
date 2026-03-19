use actix_session::Session;
use actix_web::{web, HttpRequest, HttpResponse};
use serde::Deserialize;
use tera::{Context, Tera};

use crate::config::Config;

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

    let rendered = tmpl.render("login.html", &ctx).unwrap();
    HttpResponse::Ok().content_type("text/html").body(rendered)
}

pub async fn login_handler(
    form: web::Form<LoginForm>,
    session: Session,
    config: web::Data<Config>,
) -> HttpResponse {
    let base = &config.server.context_path;

    if form.username == config.admin.username
        && bcrypt::verify(&form.password, &config.admin.password_hash).unwrap_or(false)
    {
        session.insert("user", &form.username).unwrap();
        HttpResponse::Found()
            .insert_header(("Location", format!("{base}/")))
            .finish()
    } else {
        HttpResponse::Found()
            .insert_header(("Location", format!("{base}/login?error=1")))
            .finish()
    }
}

pub async fn welcome_page(
    tmpl: web::Data<Tera>,
    session: Session,
    config: web::Data<Config>,
) -> HttpResponse {
    let base = &config.server.context_path;
    let username = session
        .get::<String>("user")
        .unwrap_or(None)
        .unwrap_or_default();

    let mut ctx = Context::new();
    ctx.insert("base", base);
    ctx.insert("username", &username);

    let rendered = tmpl.render("welcome.html", &ctx).unwrap();
    HttpResponse::Ok().content_type("text/html").body(rendered)
}

pub async fn logout(session: Session, config: web::Data<Config>) -> HttpResponse {
    let base = &config.server.context_path;
    session.purge();
    HttpResponse::Found()
        .insert_header(("Location", format!("{base}/login")))
        .finish()
}
