use actix_web::{web, HttpRequest, HttpResponse};
use sqlx::MySqlPool;
use tera::{Context, Tera};

use crate::config::Config;
use crate::helpers::*;
use crate::models::*;

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
    log::debug!("Extract page visited: uuid={uuid}");

    if !verify_extract_hash(&uuid, &hash, &config.server.extract_salt, legacy_support) {
        return render_error(
            &tmpl,
            base,
            "无效二维码",
            actix_web::http::StatusCode::BAD_REQUEST,
        );
    }

    let exists: Option<u64> = match sqlx::query_scalar("SELECT id FROM qr_codes WHERE uuid = ?")
        .bind(&uuid)
        .fetch_optional(pool.get_ref())
        .await
    {
        Ok(v) => v,
        Err(e) => {
            log::warn!("DB query failed: {e}");
            return render_error(
                &tmpl,
                base,
                "数据库查询失败",
                actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
            );
        }
    };

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
    body: web::Json<ClaimRequest>,
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

    let browser_id = body.browser_id.trim().to_string();
    if browser_id.is_empty()
        || browser_id.len() > 36
        || !browser_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        return HttpResponse::BadRequest()
            .json(serde_json::json!({"status": "error", "message": "invalid browser_id"}));
    }

    let client_ip = req
        .connection_info()
        .realip_remote_addr()
        .unwrap_or("unknown")
        .to_string();

    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            log::warn!("Extract claim: begin transaction failed: uuid={uuid}, error={e}");
            return HttpResponse::InternalServerError()
                .json(serde_json::json!({"status": "error"}));
        }
    };

    // Row lock
    let record = match sqlx::query_as::<_, QrCodeRecord>(
        "SELECT * FROM qr_codes WHERE uuid = ? FOR UPDATE",
    )
    .bind(&uuid)
    .fetch_optional(&mut *tx)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            let _ = tx.rollback().await;
            log::warn!("Extract claim: query failed: uuid={uuid}, error={e}");
            return HttpResponse::InternalServerError()
                .json(serde_json::json!({"status": "error"}));
        }
    };

    let record = match record {
        Some(r) => r,
        None => {
            let _ = tx.rollback().await;
            return HttpResponse::NotFound()
                .json(serde_json::json!({"status": "error", "message": "not found"}));
        }
    };

    // Idempotency: check if browser_id already has a slot
    let existing_slot: Option<u32> = match sqlx::query_scalar(
        "SELECT segment_index FROM qr_browser_slots WHERE qrcode_id = ? AND browser_id = ?",
    )
    .bind(record.id)
    .bind(&browser_id)
    .fetch_optional(&mut *tx)
    .await
    {
        Ok(v) => v,
        Err(e) => {
            let _ = tx.rollback().await;
            log::warn!("Extract claim: query slot failed: uuid={uuid}, error={e}");
            return HttpResponse::InternalServerError()
                .json(serde_json::json!({"status": "error"}));
        }
    };

    if let Some(seg_idx) = existing_slot {
        let _ = tx.commit().await;
        let segments = parse_segments(&record.text_content);
        let text = segments.get(seg_idx as usize).cloned().unwrap_or_default();
        log::debug!(
            "Extract claim idempotent: uuid={uuid}, browser_id={browser_id}, segment={seg_idx}"
        );
        return HttpResponse::Ok().json(ClaimResponse {
            status: "ok".to_string(),
            text_content: Some(text),
            segment_index: Some(seg_idx),
        });
    }

    let segments = parse_segments(&record.text_content);

    if record.used_count as usize >= segments.len() {
        let _ = tx.rollback().await;
        log::warn!(
            "Extract claim exhausted: uuid={uuid}, used={}, segments={}",
            record.used_count,
            segments.len()
        );
        return HttpResponse::Ok().json(ClaimResponse {
            status: "exhausted".to_string(),
            text_content: None,
            segment_index: None,
        });
    }

    let segment_index = record.used_count as u32;
    let text = segments[segment_index as usize].clone();

    // Insert slot
    if let Err(e) = sqlx::query(
        "INSERT INTO qr_browser_slots (qrcode_id, browser_id, segment_index, client_ip) VALUES (?, ?, ?, ?)",
    )
    .bind(record.id)
    .bind(&browser_id)
    .bind(segment_index)
    .bind(&client_ip)
    .execute(&mut *tx)
    .await
    {
        let _ = tx.rollback().await;
        log::warn!("Extract claim: insert slot failed: uuid={uuid}, error={e}");
        return HttpResponse::InternalServerError()
            .json(serde_json::json!({"status": "error"}));
    }

    // Update main table
    if let Err(e) = sqlx::query(
        "UPDATE qr_codes SET used_count = used_count + 1, last_extract_ip = ?, last_extract_at = NOW() WHERE id = ?",
    )
    .bind(&client_ip)
    .bind(record.id)
    .execute(&mut *tx)
    .await
    {
        let _ = tx.rollback().await;
        log::warn!("Extract claim: update qr_codes failed: uuid={uuid}, error={e}");
        return HttpResponse::InternalServerError()
            .json(serde_json::json!({"status": "error"}));
    }

    // Write extract log — failure rolls back the entire transaction
    if let Err(e) = sqlx::query(
        "INSERT INTO qr_extract_logs (qrcode_id, client_ip, browser_id, segment_index, extracted_at) VALUES (?, ?, ?, ?, NOW())",
    )
    .bind(record.id)
    .bind(&client_ip)
    .bind(&browser_id)
    .bind(segment_index)
    .execute(&mut *tx)
    .await
    {
        let _ = tx.rollback().await;
        log::warn!("Extract claim: insert log failed: uuid={uuid}, error={e}");
        return HttpResponse::InternalServerError()
            .json(serde_json::json!({"status": "error"}));
    }

    if let Err(e) = tx.commit().await {
        log::warn!("Extract claim: commit failed: uuid={uuid}, error={e}");
        return HttpResponse::InternalServerError()
            .json(serde_json::json!({"status": "error"}));
    }

    log::info!("Extract claim success: uuid={uuid}, ip={client_ip}, browser_id={browser_id}, segment={segment_index}");

    HttpResponse::Ok().json(ClaimResponse {
        status: "ok".to_string(),
        text_content: Some(text),
        segment_index: Some(segment_index),
    })
}
