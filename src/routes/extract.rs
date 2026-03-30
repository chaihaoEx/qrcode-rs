use actix_web::{web, HttpRequest, HttpResponse};
use sqlx::MySqlPool;
use tera::{Context, Tera};

use crate::config::Config;
use crate::db_try;
use crate::services;
use crate::utils::crypto::*;
use crate::utils::render::*;
use crate::utils::validation::get_client_ip;

/// Extract landing page (GET): validates HMAC and UUID, renders skeleton for AJAX
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

    if !verify_extract_hash(&uuid, &hash, &config.server.extract_salt, legacy_support) {
        return render_error(
            &tmpl,
            base,
            "无效二维码",
            actix_web::http::StatusCode::BAD_REQUEST,
        );
    }

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

/// Claim a slot (POST /extract/{uuid}/{hash}/claim)
/// Each browser_id gets one segment sequentially
pub async fn extract_claim_handler(
    path: web::Path<(String, String)>,
    body: web::Json<crate::models::ClaimRequest>,
    pool: web::Data<MySqlPool>,
    config: web::Data<Config>,
    req: HttpRequest,
) -> HttpResponse {
    let (uuid, hash) = path.into_inner();
    let legacy_support = config.server.legacy_hash_support.unwrap_or(true);

    if !verify_extract_hash(&uuid, &hash, &config.server.extract_salt, legacy_support) {
        return HttpResponse::BadRequest()
            .json(serde_json::json!({"status": "error", "message": "invalid hash"}));
    }

    if body.browser_id.len() > 36 {
        return HttpResponse::BadRequest()
            .json(serde_json::json!({"status": "error", "message": "invalid browser_id"}));
    }
    let browser_id = body.browser_id.trim().to_string();
    if browser_id.is_empty()
        || !browser_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        return HttpResponse::BadRequest()
            .json(serde_json::json!({"status": "error", "message": "invalid browser_id"}));
    }

    let client_ip = get_client_ip(&req);

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
