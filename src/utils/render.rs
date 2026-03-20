use actix_web::HttpResponse;
use tera::{Context, Tera};

/// DB error -> HTML error page. Use in admin handlers.
/// Usage: `let val = db_try!(query.await, &tmpl, base);`
#[macro_export]
macro_rules! db_try {
    ($expr:expr, $tmpl:expr, $base:expr) => {
        match $expr {
            Ok(v) => v,
            Err(e) => {
                log::warn!("DB query failed: {e}");
                return $crate::utils::render::render_error(
                    $tmpl,
                    $base,
                    "数据库查询失败",
                    actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
                );
            }
        }
    };
}

/// DB fetch_optional -> HTML error page, with 404 on None.
/// Usage: `let record = db_try_optional!(query.await, &tmpl, base, "二维码不存在");`
#[macro_export]
macro_rules! db_try_optional {
    ($expr:expr, $tmpl:expr, $base:expr, $not_found_msg:expr) => {
        match $expr {
            Ok(Some(v)) => v,
            Ok(None) => {
                return $crate::utils::render::render_error(
                    $tmpl,
                    $base,
                    $not_found_msg,
                    actix_web::http::StatusCode::NOT_FOUND,
                );
            }
            Err(e) => {
                log::warn!("DB query failed: {e}");
                return $crate::utils::render::render_error(
                    $tmpl,
                    $base,
                    "数据库查询失败",
                    actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
                );
            }
        }
    };
}

pub fn render_template(tmpl: &Tera, template: &str, ctx: &Context) -> HttpResponse {
    match tmpl.render(template, ctx) {
        Ok(rendered) => HttpResponse::Ok().content_type("text/html").body(rendered),
        Err(e) => {
            log::warn!("Template render failed: template={template}, error={e}");
            HttpResponse::InternalServerError()
                .content_type("text/plain")
                .body("Internal Server Error")
        }
    }
}

pub fn render_template_with_status(
    tmpl: &Tera,
    template: &str,
    ctx: &Context,
    status: actix_web::http::StatusCode,
) -> HttpResponse {
    match tmpl.render(template, ctx) {
        Ok(rendered) => HttpResponse::build(status)
            .content_type("text/html")
            .body(rendered),
        Err(e) => {
            log::warn!("Template render failed: template={template}, error={e}");
            HttpResponse::InternalServerError()
                .content_type("text/plain")
                .body("Internal Server Error")
        }
    }
}

pub fn render_error(
    tmpl: &Tera,
    base: &str,
    message: &str,
    status: actix_web::http::StatusCode,
) -> HttpResponse {
    let mut ctx = Context::new();
    ctx.insert("base", base);
    ctx.insert("message", message);
    render_template_with_status(tmpl, "error.html", &ctx, status)
}
