//! 模板渲染工具模块
//!
//! 提供 Tera 模板渲染的统一封装函数和数据库操作的错误处理宏。
//! 所有 HTML 响应的渲染均通过本模块完成，确保错误处理一致性。

use actix_web::HttpResponse;
use tera::{Context, Tera};

/// 数据库查询错误处理宏：将 `Result<T, E>` 转换为 HTML 错误页面。
///
/// 当查询失败时记录警告日志并返回 500 错误页面。
/// 用法：`let val = db_try!(query.await, &tmpl, base);`
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

/// 数据库可选查询错误处理宏：处理 `Result<Option<T>, E>`。
///
/// 查询失败返回 500 错误页面，结果为 `None` 时返回 404 错误页面。
/// 用法：`let record = db_try_optional!(query.await, &tmpl, base, "二维码不存在");`
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

/// 渲染模板并返回 200 OK 的 HTML 响应。
///
/// 渲染失败时返回 500 纯文本错误响应。
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

/// 渲染模板并返回指定 HTTP 状态码的 HTML 响应。
///
/// 用于需要返回非 200 状态码的场景（如表单校验失败返回 400）。
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

/// 渲染通用错误页面（使用 `error.html` 模板）。
///
/// # 参数
/// - `tmpl` - Tera 模板引擎实例
/// - `base` - 虚拟目录前缀（`context_path`）
/// - `message` - 错误提示信息
/// - `status` - HTTP 状态码
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
