//! 公开提取路由处理模块
//!
//! 处理二维码内容的公开提取请求，无需登录即可访问。
//! 包含提取页面渲染（GET）和槽位领取接口（POST JSON）。

use actix_web::{web, HttpRequest, HttpResponse};
use sqlx::MySqlPool;
use tera::{Context, Tera};

use crate::config::Config;
use crate::db_try;
use crate::services;
use crate::utils::crypto::*;
use crate::utils::render::*;
use crate::utils::validation::get_client_ip;

/// 校验 browser_id 格式。
///
/// 要求：长度不超过 36 字符，仅包含字母、数字和连字符。
/// 返回去除首尾空白后的 browser_id。
pub(crate) fn validate_browser_id(browser_id: &str) -> Result<String, &'static str> {
    if browser_id.len() > 36 {
        return Err("invalid browser_id");
    }
    let trimmed = browser_id.trim().to_string();
    if trimmed.is_empty()
        || !trimmed
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        return Err("invalid browser_id");
    }
    Ok(trimmed)
}

/// 提取页面（GET `/extract/{uuid}/{hash}`）。
///
/// 验证 HMAC 签名和二维码存在性后，渲染提取页面骨架。
/// 实际内容通过 AJAX 调用 `/claim` 接口获取。
pub async fn extract_page(
    path: web::Path<(String, String)>,
    tmpl: web::Data<Tera>,
    pool: web::Data<MySqlPool>,
    config: web::Data<Config>,
) -> HttpResponse {
    let (uuid, hash) = path.into_inner();
    let base = &config.server.context_path;
    let legacy_support = config.server.legacy_hash_support.unwrap_or(true);
    log::debug!("Extract page visited");

    // 验证 HMAC 签名
    if !verify_extract_hash(&uuid, &hash, &config.server.extract_salt, legacy_support) {
        return render_error(
            &tmpl,
            base,
            "无效二维码",
            actix_web::http::StatusCode::BAD_REQUEST,
        );
    }

    // 检查二维码是否存在
    let exists: Option<u64> = db_try!(
        services::extract::check_exists(pool.get_ref(), &uuid).await,
        &tmpl,
        base
    );

    if exists.is_none() {
        return render_error(
            &tmpl,
            base,
            "二维码不存在",
            actix_web::http::StatusCode::NOT_FOUND,
        );
    }

    let mut ctx = Context::new();
    ctx.insert("base", base);
    ctx.insert("uuid", &uuid);
    ctx.insert("hash", &hash);

    render_template(&tmpl, "extract.html", &ctx)
}

/// 领取槽位接口（POST `/extract/{uuid}/{hash}/claim`）。
///
/// 接收 JSON 请求体 `{ "browser_id": "..." }`，为该浏览器分配一个文本分段。
/// 返回 JSON 响应，包含状态和分配的内容。
pub async fn extract_claim_handler(
    path: web::Path<(String, String)>,
    body: web::Json<crate::models::ClaimRequest>,
    pool: web::Data<MySqlPool>,
    config: web::Data<Config>,
    req: HttpRequest,
) -> HttpResponse {
    let (uuid, hash) = path.into_inner();
    let legacy_support = config.server.legacy_hash_support.unwrap_or(true);

    // 验证 HMAC 签名
    if !verify_extract_hash(&uuid, &hash, &config.server.extract_salt, legacy_support) {
        return HttpResponse::BadRequest()
            .json(serde_json::json!({"status": "error", "message": "invalid hash"}));
    }

    // 校验 browser_id 格式
    let browser_id = match validate_browser_id(&body.browser_id) {
        Ok(id) => id,
        Err(_) => {
            return HttpResponse::BadRequest()
                .json(serde_json::json!({"status": "error", "message": "invalid browser_id"}));
        }
    };

    let client_ip = get_client_ip(&req);

    // 调用服务层领取槽位
    match services::extract::claim_slot(pool.get_ref(), &uuid, &browser_id, &client_ip).await {
        Ok(response) => {
            if response.status == "not_found" {
                HttpResponse::NotFound()
                    .json(serde_json::json!({"status": "error", "message": "not found"}))
            } else {
                HttpResponse::Ok().json(response)
            }
        }
        Err(e) => {
            log::warn!("Extract claim failed: error={e}");
            HttpResponse::InternalServerError().json(serde_json::json!({"status": "error"}))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_browser_id_valid_uuid() {
        assert!(validate_browser_id("550e8400-e29b-41d4-a716-446655440000").is_ok());
    }

    #[test]
    fn test_browser_id_short() {
        assert_eq!(validate_browser_id("abc-123").unwrap(), "abc-123");
    }

    #[test]
    fn test_browser_id_too_long() {
        let id = "a".repeat(37);
        assert!(validate_browser_id(&id).is_err());
    }

    #[test]
    fn test_browser_id_empty() {
        assert!(validate_browser_id("").is_err());
    }

    #[test]
    fn test_browser_id_spaces_only() {
        assert!(validate_browser_id("   ").is_err());
    }

    #[test]
    fn test_browser_id_special_chars() {
        assert!(validate_browser_id("abc@123").is_err());
    }

    #[test]
    fn test_browser_id_trims() {
        assert_eq!(validate_browser_id(" abc ").unwrap(), "abc");
    }
}
