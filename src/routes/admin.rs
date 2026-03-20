use actix_session::Session;
use actix_web::{web, HttpResponse};
use image::ImageEncoder;
use sqlx::MySqlPool;
use tera::{Context, Tera};

use crate::config::Config;
use crate::csrf;
use crate::helpers::*;
use crate::models::*;
use crate::{db_try, db_try_optional};

pub async fn list_page(
    tmpl: web::Data<Tera>,
    pool: web::Data<MySqlPool>,
    session: Session,
    config: web::Data<Config>,
    query: web::Query<ListQuery>,
) -> HttpResponse {
    let base = &config.server.context_path;
    let username = session
        .get::<String>("user")
        .unwrap_or(None)
        .unwrap_or_default();
    let csrf_token = csrf::ensure_csrf_token(&session);
    let keyword = query.keyword.clone().unwrap_or_default();
    let (page, offset) = calc_page_offset(query.page);
    log::debug!("list_page: page={page}, keyword={keyword:?}");

    let (total, records) = if keyword.is_empty() {
        let total: i64 = db_try!(
            sqlx::query_scalar("SELECT COUNT(*) FROM qr_codes")
                .fetch_one(pool.get_ref())
                .await,
            &tmpl,
            base
        );

        let records = db_try!(
            sqlx::query_as::<_, QrCodeRecord>(
                "SELECT * FROM qr_codes ORDER BY created_at DESC LIMIT ? OFFSET ?",
            )
            .bind(PAGE_SIZE)
            .bind(offset)
            .fetch_all(pool.get_ref())
            .await,
            &tmpl,
            base
        );

        (total, records)
    } else {
        // Note: %keyword% LIKE on TEXT column cannot use index.
        // At current scale this is acceptable; consider full-text search if data grows large.
        let like_pattern = format!("%{keyword}%");

        let total: i64 = db_try!(
            sqlx::query_scalar(
                "SELECT COUNT(*) FROM qr_codes WHERE text_content LIKE ? OR remark LIKE ?",
            )
            .bind(&like_pattern)
            .bind(&like_pattern)
            .fetch_one(pool.get_ref())
            .await,
            &tmpl,
            base
        );

        let records = db_try!(
            sqlx::query_as::<_, QrCodeRecord>(
                "SELECT * FROM qr_codes WHERE text_content LIKE ? OR remark LIKE ? ORDER BY created_at DESC LIMIT ? OFFSET ?",
            )
            .bind(&like_pattern)
            .bind(&like_pattern)
            .bind(PAGE_SIZE)
            .bind(offset)
            .fetch_all(pool.get_ref())
            .await,
            &tmpl,
            base
        );

        (total, records)
    };

    let total_pages = calc_total_pages(total);

    let extract_hashes: std::collections::HashMap<String, String> = records
        .iter()
        .map(|r| {
            (
                r.uuid.clone(),
                generate_extract_hash(&r.uuid, &config.server.extract_salt),
            )
        })
        .collect();

    let display_texts: std::collections::HashMap<String, String> = records
        .iter()
        .map(|r| (r.uuid.clone(), truncate_display(&r.text_content)))
        .collect();

    let mut ctx = Context::new();
    ctx.insert("base", base);
    ctx.insert("username", &username);
    ctx.insert("csrf_token", &csrf_token);
    ctx.insert("records", &records);
    ctx.insert("extract_hashes", &extract_hashes);
    ctx.insert("display_texts", &display_texts);
    ctx.insert("page", &page);
    ctx.insert("total_pages", &total_pages);
    ctx.insert("keyword", &keyword);
    ctx.insert("ai_enabled", &config.ai.is_some());

    render_template(&tmpl, "list.html", &ctx)
}

pub async fn create_page(
    tmpl: web::Data<Tera>,
    session: Session,
    config: web::Data<Config>,
) -> HttpResponse {
    let base = &config.server.context_path;
    let username = session
        .get::<String>("user")
        .unwrap_or(None)
        .unwrap_or_default();
    let csrf_token = csrf::ensure_csrf_token(&session);

    let mut ctx = Context::new();
    ctx.insert("base", base);
    ctx.insert("username", &username);
    ctx.insert("csrf_token", &csrf_token);

    render_template(&tmpl, "create.html", &ctx)
}

pub async fn create_handler(
    form: web::Form<CreateForm>,
    tmpl: web::Data<Tera>,
    pool: web::Data<MySqlPool>,
    session: Session,
    config: web::Data<Config>,
) -> HttpResponse {
    let base = &config.server.context_path;

    if !csrf::validate_csrf_token(&session, &form.csrf_token) {
        return render_error(
            &tmpl,
            base,
            "CSRF 校验失败，请刷新页面重试",
            actix_web::http::StatusCode::FORBIDDEN,
        );
    }

    let (segments, text_content_json) = match validate_segments(&form.text_content) {
        Ok(result) => result,
        Err(msg) => {
            return render_error(&tmpl, base, msg, actix_web::http::StatusCode::BAD_REQUEST)
        }
    };

    let uuid = uuid::Uuid::new_v4().to_string();
    let max_count = form.max_count.unwrap_or(5).clamp(1, MAX_COUNT_UPPER);
    let remark = form.remark.as_deref().filter(|s| !s.trim().is_empty());

    if let Err(e) = sqlx::query(
        "INSERT INTO qr_codes (uuid, text_content, remark, max_count, used_count, created_at) VALUES (?, ?, ?, ?, 0, NOW())",
    )
    .bind(&uuid)
    .bind(&text_content_json)
    .bind(remark)
    .bind(max_count)
    .execute(pool.get_ref())
    .await
    {
        log::warn!("QR code insert failed: uuid={uuid}, error={e}");
        return render_error(
            &tmpl,
            base,
            "创建失败，请稍后重试",
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
        );
    }

    log::info!(
        "QR code created: uuid={uuid}, max_count={max_count}, segments={}",
        segments.len()
    );

    HttpResponse::Found()
        .insert_header(("Location", format!("{base}/")))
        .finish()
}

pub async fn download_image(
    path: web::Path<String>,
    config: web::Data<Config>,
) -> HttpResponse {
    let uuid = path.into_inner();
    // Sanitize UUID for Content-Disposition header safety
    let safe_uuid: String = uuid
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .take(36)
        .collect();
    log::info!("Download QR image: uuid={safe_uuid}");
    let hash = generate_extract_hash(&safe_uuid, &config.server.extract_salt);
    let url = format!(
        "{}{}/extract/{safe_uuid}/{hash}",
        config.server.public_host, config.server.context_path
    );

    let qr = match qrcode::QrCode::new(url.as_bytes()) {
        Ok(qr) => qr,
        Err(_) => return HttpResponse::InternalServerError().body("Failed to generate QR code"),
    };

    let img = qr
        .render::<image::Luma<u8>>()
        .min_dimensions(256, 256)
        .build();

    let mut buf = std::io::Cursor::new(Vec::new());
    let encoder = image::codecs::png::PngEncoder::new(&mut buf);
    if encoder
        .write_image(
            img.as_raw(),
            img.width(),
            img.height(),
            image::ExtendedColorType::L8,
        )
        .is_err()
    {
        return HttpResponse::InternalServerError().body("Failed to encode PNG");
    }

    HttpResponse::Ok()
        .content_type("image/png")
        .insert_header((
            "Content-Disposition",
            format!("attachment; filename=\"qrcode-{safe_uuid}.png\""),
        ))
        .body(buf.into_inner())
}

pub async fn delete_handler(
    path: web::Path<String>,
    form: web::Form<ActionForm>,
    tmpl: web::Data<Tera>,
    pool: web::Data<MySqlPool>,
    session: Session,
    config: web::Data<Config>,
) -> HttpResponse {
    let uuid = path.into_inner();
    let base = &config.server.context_path;

    if !csrf::validate_csrf_token(&session, &form.csrf_token) {
        return render_error(
            &tmpl,
            base,
            "CSRF 校验失败，请刷新页面重试",
            actix_web::http::StatusCode::FORBIDDEN,
        );
    }

    if let Err(e) = sqlx::query("DELETE FROM qr_codes WHERE uuid = ?")
        .bind(&uuid)
        .execute(pool.get_ref())
        .await
    {
        log::warn!("Delete failed: uuid={uuid}, error={e}");
        return render_error(
            &tmpl,
            base,
            "删除失败，请稍后重试",
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
        );
    }

    log::info!("QR code deleted: uuid={uuid}");
    HttpResponse::Found()
        .insert_header(("Location", format!("{base}/")))
        .finish()
}

pub async fn reset_handler(
    path: web::Path<String>,
    form: web::Form<ActionForm>,
    tmpl: web::Data<Tera>,
    pool: web::Data<MySqlPool>,
    session: Session,
    config: web::Data<Config>,
) -> HttpResponse {
    let uuid = path.into_inner();
    let base = &config.server.context_path;

    if !csrf::validate_csrf_token(&session, &form.csrf_token) {
        return render_error(
            &tmpl,
            base,
            "CSRF 校验失败，请刷新页面重试",
            actix_web::http::StatusCode::FORBIDDEN,
        );
    }

    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            log::warn!("Reset failed: begin transaction: uuid={uuid}, error={e}");
            return render_error(
                &tmpl,
                base,
                "重置失败，请稍后重试",
                actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
            );
        }
    };

    if let Err(e) = sqlx::query(
        "DELETE FROM qr_browser_slots WHERE qrcode_id = (SELECT id FROM qr_codes WHERE uuid = ?)",
    )
    .bind(&uuid)
    .execute(&mut *tx)
    .await
    {
        let _ = tx.rollback().await;
        log::warn!("Reset failed: delete slots: uuid={uuid}, error={e}");
        return render_error(
            &tmpl,
            base,
            "重置失败，请稍后重试",
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
        );
    }

    if let Err(e) = sqlx::query("UPDATE qr_codes SET used_count = 0 WHERE uuid = ?")
        .bind(&uuid)
        .execute(&mut *tx)
        .await
    {
        let _ = tx.rollback().await;
        log::warn!("Reset failed: update used_count: uuid={uuid}, error={e}");
        return render_error(
            &tmpl,
            base,
            "重置失败，请稍后重试",
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
        );
    }

    if let Err(e) = tx.commit().await {
        log::warn!("Reset failed: commit: uuid={uuid}, error={e}");
        return render_error(
            &tmpl,
            base,
            "重置失败，请稍后重试",
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
        );
    }

    log::info!("QR code slots reset: uuid={uuid}");
    HttpResponse::Found()
        .insert_header(("Location", format!("{base}/")))
        .finish()
}

pub async fn edit_page(
    path: web::Path<String>,
    tmpl: web::Data<Tera>,
    pool: web::Data<MySqlPool>,
    session: Session,
    config: web::Data<Config>,
) -> HttpResponse {
    let uuid = path.into_inner();
    let base = &config.server.context_path;
    let username = session
        .get::<String>("user")
        .unwrap_or(None)
        .unwrap_or_default();
    let csrf_token = csrf::ensure_csrf_token(&session);

    let record = db_try_optional!(
        sqlx::query_as::<_, QrCodeRecord>("SELECT * FROM qr_codes WHERE uuid = ?",)
            .bind(&uuid)
            .fetch_optional(pool.get_ref())
            .await,
        &tmpl,
        base,
        "二维码不存在"
    );

    let segments = parse_segments(&record.text_content);

    let mut ctx = Context::new();
    ctx.insert("base", base);
    ctx.insert("username", &username);
    ctx.insert("csrf_token", &csrf_token);
    ctx.insert("record", &record);
    ctx.insert("segments", &segments);

    render_template(&tmpl, "edit.html", &ctx)
}

pub async fn edit_handler(
    path: web::Path<String>,
    form: web::Form<CreateForm>,
    tmpl: web::Data<Tera>,
    pool: web::Data<MySqlPool>,
    session: Session,
    config: web::Data<Config>,
) -> HttpResponse {
    let uuid = path.into_inner();
    let base = &config.server.context_path;

    if !csrf::validate_csrf_token(&session, &form.csrf_token) {
        return render_error(
            &tmpl,
            base,
            "CSRF 校验失败，请刷新页面重试",
            actix_web::http::StatusCode::FORBIDDEN,
        );
    }

    let (_segments, text_content_json) = match validate_segments(&form.text_content) {
        Ok(result) => result,
        Err(msg) => {
            return render_error(&tmpl, base, msg, actix_web::http::StatusCode::BAD_REQUEST)
        }
    };

    let max_count = form.max_count.unwrap_or(5).clamp(1, MAX_COUNT_UPPER);
    let remark = form.remark.as_deref().filter(|s| !s.trim().is_empty());

    if let Err(e) = sqlx::query(
        "UPDATE qr_codes SET text_content = ?, remark = ?, max_count = ? WHERE uuid = ?",
    )
    .bind(&text_content_json)
    .bind(remark)
    .bind(max_count)
    .bind(&uuid)
    .execute(pool.get_ref())
    .await
    {
        log::warn!("QR code update failed: uuid={uuid}, error={e}");
        return render_error(
            &tmpl,
            base,
            "更新失败，请稍后重试",
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
        );
    }

    log::info!("QR code updated: uuid={uuid}");
    HttpResponse::Found()
        .insert_header(("Location", format!("{base}/")))
        .finish()
}

pub async fn extract_logs_page(
    path: web::Path<String>,
    tmpl: web::Data<Tera>,
    pool: web::Data<MySqlPool>,
    session: Session,
    config: web::Data<Config>,
    query: web::Query<LogsQuery>,
) -> HttpResponse {
    let uuid = path.into_inner();
    let base = &config.server.context_path;
    let username = session
        .get::<String>("user")
        .unwrap_or(None)
        .unwrap_or_default();
    let (page, offset) = calc_page_offset(query.page);
    let list_page = query.list_page.unwrap_or(1);
    let list_keyword = query.list_keyword.clone().unwrap_or_default();
    log::debug!("Extract logs page: uuid={uuid}, page={page}");

    let record = db_try_optional!(
        sqlx::query_as::<_, QrCodeRecord>("SELECT * FROM qr_codes WHERE uuid = ?",)
            .bind(&uuid)
            .fetch_optional(pool.get_ref())
            .await,
        &tmpl,
        base,
        "二维码不存在"
    );

    let total: i64 = db_try!(
        sqlx::query_scalar("SELECT COUNT(*) FROM qr_extract_logs WHERE qrcode_id = ?",)
            .bind(record.id)
            .fetch_one(pool.get_ref())
            .await,
        &tmpl,
        base
    );

    let logs = db_try!(
        sqlx::query_as::<_, ExtractLog>(
            "SELECT * FROM qr_extract_logs WHERE qrcode_id = ? ORDER BY extracted_at DESC LIMIT ? OFFSET ?",
        )
        .bind(record.id)
        .bind(PAGE_SIZE)
        .bind(offset)
        .fetch_all(pool.get_ref())
        .await,
        &tmpl,
        base
    );

    let total_pages = calc_total_pages(total);
    let display_text = truncate_display(&record.text_content);

    let mut ctx = Context::new();
    ctx.insert("base", base);
    ctx.insert("username", &username);
    ctx.insert("record", &record);
    ctx.insert("display_text", &display_text);
    ctx.insert("logs", &logs);
    ctx.insert("page", &page);
    ctx.insert("total_pages", &total_pages);
    ctx.insert("list_page", &list_page);
    ctx.insert("list_keyword", &list_keyword);

    render_template(&tmpl, "logs.html", &ctx)
}

// ---- AI 评论生成 ----

pub async fn ai_generate_page(
    tmpl: web::Data<Tera>,
    session: Session,
    config: web::Data<Config>,
) -> HttpResponse {
    let base = &config.server.context_path;

    if config.ai.is_none() {
        return render_error(
            &tmpl,
            base,
            "AI 功能未配置",
            actix_web::http::StatusCode::NOT_FOUND,
        );
    }

    let username = session
        .get::<String>("user")
        .unwrap_or(None)
        .unwrap_or_default();
    let csrf_token = csrf::ensure_csrf_token(&session);

    let mut ctx = Context::new();
    ctx.insert("base", base);
    ctx.insert("username", &username);
    ctx.insert("csrf_token", &csrf_token);

    render_template(&tmpl, "ai_generate.html", &ctx)
}

pub async fn ai_generate_handler(
    body: web::Json<AiGenerateRequest>,
    session: Session,
    config: web::Data<Config>,
) -> HttpResponse {
    if !csrf::validate_csrf_token(&session, &body.csrf_token) {
        return HttpResponse::Forbidden()
            .json(serde_json::json!({"status": "error", "message": "CSRF 校验失败"}));
    }

    let ai_config = match &config.ai {
        Some(c) => c,
        None => {
            return HttpResponse::BadRequest()
                .json(serde_json::json!({"status": "error", "message": "AI 功能未配置"}));
        }
    };

    let topic = body.topic.trim();
    if topic.is_empty() {
        return HttpResponse::BadRequest()
            .json(serde_json::json!({"status": "error", "message": "主题不能为空"}));
    }

    let count = body.count.unwrap_or(10).clamp(1, 50);
    let style = body.style.as_deref().unwrap_or("").trim();
    let examples = body.examples.as_deref().unwrap_or("").trim();

    match crate::ai::generate_comments(ai_config, topic, count, style, examples).await {
        Ok(comments) => {
            HttpResponse::Ok().json(serde_json::json!({"status": "ok", "comments": comments}))
        }
        Err(e) => {
            log::warn!("AI generate failed: {e}");
            HttpResponse::Ok()
                .json(serde_json::json!({"status": "error", "message": e}))
        }
    }
}

pub async fn ai_create_handler(
    form: web::Form<AiCreateForm>,
    tmpl: web::Data<Tera>,
    pool: web::Data<MySqlPool>,
    session: Session,
    config: web::Data<Config>,
) -> HttpResponse {
    let base = &config.server.context_path;

    if !csrf::validate_csrf_token(&session, &form.csrf_token) {
        return render_error(
            &tmpl,
            base,
            "CSRF 校验失败，请刷新页面重试",
            actix_web::http::StatusCode::FORBIDDEN,
        );
    }

    let (segments, text_content_json) = match validate_segments(&form.comments) {
        Ok(result) => result,
        Err(msg) => {
            return render_error(&tmpl, base, msg, actix_web::http::StatusCode::BAD_REQUEST)
        }
    };

    let uuid = uuid::Uuid::new_v4().to_string();
    let max_count = form.max_count.unwrap_or(segments.len() as u32).clamp(1, MAX_COUNT_UPPER);
    let remark = form.remark.as_deref().filter(|s| !s.trim().is_empty());

    if let Err(e) = sqlx::query(
        "INSERT INTO qr_codes (uuid, text_content, remark, max_count, used_count, created_at) VALUES (?, ?, ?, ?, 0, NOW())",
    )
    .bind(&uuid)
    .bind(&text_content_json)
    .bind(remark)
    .bind(max_count)
    .execute(pool.get_ref())
    .await
    {
        log::warn!("QR code insert failed: uuid={uuid}, error={e}");
        return render_error(
            &tmpl,
            base,
            "创建失败，请稍后重试",
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
        );
    }

    log::info!(
        "QR code created via AI: uuid={uuid}, max_count={max_count}, segments={}",
        segments.len()
    );

    HttpResponse::Found()
        .insert_header(("Location", format!("{base}/")))
        .finish()
}
