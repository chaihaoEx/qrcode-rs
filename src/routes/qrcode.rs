use actix_session::Session;
use actix_web::{web, HttpRequest, HttpResponse};
use chrono::NaiveDateTime;

mod datetime_format {
    use chrono::NaiveDateTime;
    use serde::{self, Serializer};

    const FORMAT: &str = "%Y-%m-%d %H:%M:%S";

    pub fn serialize<S>(date: &NaiveDateTime, serializer: S) -> Result<S::Ok, S::Error>
    where S: Serializer {
        serializer.serialize_str(&date.format(FORMAT).to_string())
    }
}

mod option_datetime_format {
    use chrono::NaiveDateTime;
    use serde::{self, Serializer};

    const FORMAT: &str = "%Y-%m-%d %H:%M:%S";

    pub fn serialize<S>(date: &Option<NaiveDateTime>, serializer: S) -> Result<S::Ok, S::Error>
    where S: Serializer {
        match date {
            Some(d) => serializer.serialize_str(&d.format(FORMAT).to_string()),
            None => serializer.serialize_none(),
        }
    }
}
use image::ImageEncoder;
use serde::{Deserialize, Serialize};
use sqlx::MySqlPool;
use tera::{Context, Tera};

use crate::config::Config;

const PAGE_SIZE: i64 = 20;

/// 基于 UUID 和盐值生成 HMAC-SHA256 校验哈希（取前 8 位 hex）
pub fn generate_extract_hash(uuid: &str, salt: &str) -> String {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    let mut mac = Hmac::<Sha256>::new_from_slice(salt.as_bytes()).unwrap();
    mac.update(uuid.as_bytes());
    let result = mac.finalize().into_bytes();
    format!("{:02x}{:02x}{:02x}{:02x}", result[0], result[1], result[2], result[3])
}

fn verify_extract_hash(uuid: &str, hash: &str, salt: &str) -> bool {
    generate_extract_hash(uuid, salt) == hash
}

fn render_template(tmpl: &Tera, template: &str, ctx: &Context) -> HttpResponse {
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

fn render_template_with_status(tmpl: &Tera, template: &str, ctx: &Context, status: actix_web::http::StatusCode) -> HttpResponse {
    match tmpl.render(template, ctx) {
        Ok(rendered) => HttpResponse::build(status).content_type("text/html").body(rendered),
        Err(e) => {
            log::warn!("Template render failed: template={template}, error={e}");
            HttpResponse::InternalServerError()
                .content_type("text/plain")
                .body("Internal Server Error")
        }
    }
}

fn render_error(tmpl: &Tera, base: &str, message: &str, status: actix_web::http::StatusCode) -> HttpResponse {
    let mut ctx = Context::new();
    ctx.insert("base", base);
    ctx.insert("message", message);
    render_template_with_status(tmpl, "error.html", &ctx, status)
}

/// 解析 text_content 为多段文字。如果不是 JSON 数组，回退为单段。
fn parse_segments(text_content: &str) -> Vec<String> {
    match serde_json::from_str::<Vec<String>>(text_content) {
        Ok(segments) if !segments.is_empty() => segments,
        _ => vec![text_content.to_string()],
    }
}

/// 截断显示：取第一段前12个字符 + "..."
fn truncate_display(text_content: &str) -> String {
    let segments = parse_segments(text_content);
    let first = segments.first().map(|s| s.as_str()).unwrap_or("");
    // 将换行替换为空格，方便在列表中显示
    let first = first.replace('\n', " ").replace('\r', "");
    let chars: Vec<char> = first.chars().collect();
    if chars.len() > 12 {
        format!("{}...", chars[..12].iter().collect::<String>())
    } else {
        first.to_string()
    }
}

#[derive(sqlx::FromRow, Serialize)]
pub struct QrCodeRecord {
    pub id: u64,
    pub uuid: String,
    pub text_content: String,
    pub remark: Option<String>,
    pub max_count: u32,
    pub used_count: u32,
    pub last_extract_ip: Option<String>,
    #[serde(serialize_with = "datetime_format::serialize")]
    pub created_at: NaiveDateTime,
    #[sqlx(default)]
    #[serde(serialize_with = "option_datetime_format::serialize")]
    pub last_extract_at: Option<NaiveDateTime>,
}

#[derive(sqlx::FromRow, Serialize)]
pub struct ExtractLog {
    pub id: u64,
    pub qrcode_id: u64,
    pub client_ip: String,
    pub browser_id: String,
    pub segment_index: Option<u32>,
    #[serde(serialize_with = "datetime_format::serialize")]
    pub extracted_at: NaiveDateTime,
}

#[derive(Deserialize)]
pub struct ClaimRequest {
    pub browser_id: String,
}

#[derive(Serialize)]
pub struct ClaimResponse {
    pub status: String,
    pub text_content: Option<String>,
    pub segment_index: Option<u32>,
}

#[derive(Deserialize)]
pub struct ListQuery {
    pub page: Option<i64>,
    pub keyword: Option<String>,
}

#[derive(Deserialize)]
pub struct LogsQuery {
    pub page: Option<i64>,
    pub list_page: Option<i64>,
    pub list_keyword: Option<String>,
}

#[derive(Deserialize)]
pub struct CreateForm {
    pub text_content: String,
    pub remark: Option<String>,
    pub max_count: Option<u32>,
}

// ---- 管理页面 (需认证) ----

pub async fn list_page(
    tmpl: web::Data<Tera>,
    pool: web::Data<MySqlPool>,
    session: Session,
    config: web::Data<Config>,
    query: web::Query<ListQuery>,
) -> HttpResponse {
    let base = &config.server.context_path;
    let username = session.get::<String>("user").unwrap_or(None).unwrap_or_default();
    let page = query.page.unwrap_or(1).clamp(1, 100_000);
    let keyword = query.keyword.clone().unwrap_or_default();
    let offset = (page - 1) * PAGE_SIZE;
    log::debug!("list_page: page={page}, keyword={keyword:?}");

    let (total, records) = if keyword.is_empty() {
        let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM qr_codes")
            .fetch_one(pool.get_ref())
            .await
            .unwrap_or(0);

        let records = sqlx::query_as::<_, QrCodeRecord>(
            "SELECT * FROM qr_codes ORDER BY created_at DESC LIMIT ? OFFSET ?",
        )
        .bind(PAGE_SIZE)
        .bind(offset)
        .fetch_all(pool.get_ref())
        .await
        .unwrap_or_default();

        (total, records)
    } else {
        let like_pattern = format!("%{keyword}%");

        let total: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM qr_codes WHERE text_content LIKE ? OR remark LIKE ?",
        )
        .bind(&like_pattern)
        .bind(&like_pattern)
        .fetch_one(pool.get_ref())
        .await
        .unwrap_or(0);

        let records = sqlx::query_as::<_, QrCodeRecord>(
            "SELECT * FROM qr_codes WHERE text_content LIKE ? OR remark LIKE ? ORDER BY created_at DESC LIMIT ? OFFSET ?",
        )
        .bind(&like_pattern)
        .bind(&like_pattern)
        .bind(PAGE_SIZE)
        .bind(offset)
        .fetch_all(pool.get_ref())
        .await
        .unwrap_or_default();

        (total, records)
    };

    let total_pages = (total + PAGE_SIZE - 1) / PAGE_SIZE;

    let extract_hashes: std::collections::HashMap<String, String> = records
        .iter()
        .map(|r| (r.uuid.clone(), generate_extract_hash(&r.uuid, &config.server.extract_salt)))
        .collect();

    // 计算截断显示文字
    let display_texts: std::collections::HashMap<String, String> = records
        .iter()
        .map(|r| (r.uuid.clone(), truncate_display(&r.text_content)))
        .collect();

    let mut ctx = Context::new();
    ctx.insert("base", base);
    ctx.insert("username", &username);
    ctx.insert("records", &records);
    ctx.insert("extract_hashes", &extract_hashes);
    ctx.insert("display_texts", &display_texts);
    ctx.insert("page", &page);
    ctx.insert("total_pages", &total_pages);
    ctx.insert("keyword", &keyword);

    render_template(&tmpl, "list.html", &ctx)
}

pub async fn create_page(
    tmpl: web::Data<Tera>,
    session: Session,
    config: web::Data<Config>,
) -> HttpResponse {
    let base = &config.server.context_path;
    let username = session.get::<String>("user").unwrap_or(None).unwrap_or_default();

    let mut ctx = Context::new();
    ctx.insert("base", base);
    ctx.insert("username", &username);

    render_template(&tmpl, "create.html", &ctx)
}

pub async fn create_handler(
    form: web::Form<CreateForm>,
    tmpl: web::Data<Tera>,
    pool: web::Data<MySqlPool>,
    config: web::Data<Config>,
) -> HttpResponse {
    let base = &config.server.context_path;

    // text_content 应该是 JSON 数组格式
    let text_content = form.text_content.trim();
    let segments: Vec<String> = match serde_json::from_str::<Vec<String>>(text_content) {
        Ok(segs) => segs.into_iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(),
        Err(_) => {
            // 回退：当作单段文字
            let trimmed = text_content.to_string();
            if trimmed.is_empty() {
                vec![]
            } else {
                vec![trimmed]
            }
        }
    };

    if segments.is_empty() {
        return render_error(
            &tmpl,
            base,
            "文字内容不能为空",
            actix_web::http::StatusCode::BAD_REQUEST,
        );
    }

    let total_len: usize = segments.iter().map(|s| s.len()).sum();
    if total_len > 5000 {
        return render_error(
            &tmpl,
            base,
            "文字内容总长度不能超过 5000 字符",
            actix_web::http::StatusCode::BAD_REQUEST,
        );
    }

    // 存储为 JSON 数组
    let text_content_json = serde_json::to_string(&segments).unwrap_or_default();

    let uuid = uuid::Uuid::new_v4().to_string();
    let max_count = form.max_count.unwrap_or(5).clamp(1, 10000);
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

    log::info!("QR code created: uuid={uuid}, max_count={max_count}, segments={}", segments.len());

    HttpResponse::Found()
        .insert_header(("Location", format!("{base}/")))
        .finish()
}

pub async fn download_image(
    path: web::Path<String>,
    config: web::Data<Config>,
) -> HttpResponse {
    let uuid = path.into_inner();
    log::info!("Download QR image: uuid={uuid}");
    let hash = generate_extract_hash(&uuid, &config.server.extract_salt);
    let url = format!(
        "{}{}/extract/{uuid}/{hash}",
        config.server.public_host, config.server.context_path
    );

    let qr = match qrcode::QrCode::new(url.as_bytes()) {
        Ok(qr) => qr,
        Err(_) => return HttpResponse::InternalServerError().body("Failed to generate QR code"),
    };

    let img = qr.render::<image::Luma<u8>>().min_dimensions(256, 256).build();

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
            format!("attachment; filename=\"qrcode-{uuid}.png\""),
        ))
        .body(buf.into_inner())
}

// ---- 公开页面 (无需认证) ----

/// 扫码落地页（GET）：仅验证 HMAC 和 UUID 存在性，渲染骨架，内容由前端 AJAX 获取
pub async fn extract_page(
    path: web::Path<(String, String)>,
    tmpl: web::Data<Tera>,
    pool: web::Data<MySqlPool>,
    config: web::Data<Config>,
) -> HttpResponse {
    let (uuid, hash) = path.into_inner();
    let base = &config.server.context_path;
    log::debug!("Extract page visited: uuid={uuid}");

    if !verify_extract_hash(&uuid, &hash, &config.server.extract_salt) {
        return render_error(&tmpl, base, "无效二维码", actix_web::http::StatusCode::BAD_REQUEST);
    }

    let exists: Option<u64> = sqlx::query_scalar("SELECT id FROM qr_codes WHERE uuid = ?")
        .bind(&uuid)
        .fetch_optional(pool.get_ref())
        .await
        .unwrap_or(None);

    if exists.is_none() {
        return render_error(&tmpl, base, "二维码不存在", actix_web::http::StatusCode::NOT_FOUND);
    }

    let mut ctx = Context::new();
    ctx.insert("base", base);
    ctx.insert("uuid", &uuid);
    ctx.insert("hash", &hash);

    render_template(&tmpl, "extract.html", &ctx)
}

/// 领取槽位（POST /extract/{uuid}/{hash}/claim）
/// browser_id 唯一标识浏览器，每个 browser_id 顺序独占一段
pub async fn extract_claim_handler(
    path: web::Path<(String, String)>,
    body: web::Json<ClaimRequest>,
    pool: web::Data<MySqlPool>,
    config: web::Data<Config>,
    req: HttpRequest,
) -> HttpResponse {
    let (uuid, hash) = path.into_inner();

    if !verify_extract_hash(&uuid, &hash, &config.server.extract_salt) {
        return HttpResponse::BadRequest().json(serde_json::json!({"status": "error", "message": "invalid hash"}));
    }

    let browser_id = body.browser_id.trim().to_string();
    // 校验：仅允许 UUID 格式（字母数字和连字符，长度 ≤ 36）
    if browser_id.is_empty() || browser_id.len() > 36 || !browser_id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return HttpResponse::BadRequest().json(serde_json::json!({"status": "error", "message": "invalid browser_id"}));
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
            return HttpResponse::InternalServerError().json(serde_json::json!({"status": "error"}));
        }
    };

    // 行锁
    let record = sqlx::query_as::<_, QrCodeRecord>(
        "SELECT * FROM qr_codes WHERE uuid = ? FOR UPDATE",
    )
    .bind(&uuid)
    .fetch_optional(&mut *tx)
    .await
    .unwrap_or(None);

    let record = match record {
        Some(r) => r,
        None => {
            let _ = tx.rollback().await;
            return HttpResponse::NotFound().json(serde_json::json!({"status": "error", "message": "not found"}));
        }
    };

    // 幂等：检查该 browser_id 是否已有槽位
    let existing_slot: Option<u32> = sqlx::query_scalar(
        "SELECT segment_index FROM qr_browser_slots WHERE qrcode_id = ? AND browser_id = ?",
    )
    .bind(record.id)
    .bind(&browser_id)
    .fetch_optional(&mut *tx)
    .await
    .unwrap_or(None);

    if let Some(seg_idx) = existing_slot {
        let _ = tx.commit().await;
        let segments = parse_segments(&record.text_content);
        let text = segments.get(seg_idx as usize).cloned().unwrap_or_default();
        log::debug!("Extract claim idempotent: uuid={uuid}, browser_id={browser_id}, segment={seg_idx}");
        return HttpResponse::Ok().json(ClaimResponse {
            status: "ok".to_string(),
            text_content: Some(text),
            segment_index: Some(seg_idx),
        });
    }

    let segments = parse_segments(&record.text_content);

    if record.used_count as usize >= segments.len() {
        let _ = tx.rollback().await;
        log::warn!("Extract claim exhausted: uuid={uuid}, used={}, segments={}", record.used_count, segments.len());
        return HttpResponse::Ok().json(ClaimResponse {
            status: "exhausted".to_string(),
            text_content: None,
            segment_index: None,
        });
    }

    let segment_index = record.used_count as u32;
    let text = segments[segment_index as usize].clone();

    // 插入槽位
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
        return HttpResponse::InternalServerError().json(serde_json::json!({"status": "error"}));
    }

    // 更新主表
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
        return HttpResponse::InternalServerError().json(serde_json::json!({"status": "error"}));
    }

    // 写提取日志
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
        log::warn!("Extract claim: insert log failed: uuid={uuid}, error={e}");
        // 日志失败不中止事务
    }

    if let Err(e) = tx.commit().await {
        log::warn!("Extract claim: commit failed: uuid={uuid}, error={e}");
        return HttpResponse::InternalServerError().json(serde_json::json!({"status": "error"}));
    }

    log::info!("Extract claim success: uuid={uuid}, ip={client_ip}, browser_id={browser_id}, segment={segment_index}");

    HttpResponse::Ok().json(ClaimResponse {
        status: "ok".to_string(),
        text_content: Some(text),
        segment_index: Some(segment_index),
    })
}

// ---- 删除 (需认证) ----

pub async fn delete_handler(
    path: web::Path<String>,
    tmpl: web::Data<Tera>,
    pool: web::Data<MySqlPool>,
    config: web::Data<Config>,
) -> HttpResponse {
    let uuid = path.into_inner();
    let base = &config.server.context_path;

    if let Err(e) = sqlx::query("DELETE FROM qr_codes WHERE uuid = ?")
        .bind(&uuid)
        .execute(pool.get_ref())
        .await
    {
        log::warn!("Delete failed: uuid={uuid}, error={e}");
        return render_error(&tmpl, base, "删除失败，请稍后重试", actix_web::http::StatusCode::INTERNAL_SERVER_ERROR);
    }

    log::info!("QR code deleted: uuid={uuid}");
    HttpResponse::Found()
        .insert_header(("Location", format!("{base}/")))
        .finish()
}

// ---- 重置提取次数 (需认证) ----
// 清除所有槽位并将 used_count 归零

pub async fn reset_handler(
    path: web::Path<String>,
    tmpl: web::Data<Tera>,
    pool: web::Data<MySqlPool>,
    config: web::Data<Config>,
) -> HttpResponse {
    let uuid = path.into_inner();
    let base = &config.server.context_path;

    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            log::warn!("Reset failed: begin transaction: uuid={uuid}, error={e}");
            return render_error(&tmpl, base, "重置失败，请稍后重试", actix_web::http::StatusCode::INTERNAL_SERVER_ERROR);
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
        return render_error(&tmpl, base, "重置失败，请稍后重试", actix_web::http::StatusCode::INTERNAL_SERVER_ERROR);
    }

    if let Err(e) = sqlx::query("UPDATE qr_codes SET used_count = 0 WHERE uuid = ?")
        .bind(&uuid)
        .execute(&mut *tx)
        .await
    {
        let _ = tx.rollback().await;
        log::warn!("Reset failed: update used_count: uuid={uuid}, error={e}");
        return render_error(&tmpl, base, "重置失败，请稍后重试", actix_web::http::StatusCode::INTERNAL_SERVER_ERROR);
    }

    if let Err(e) = tx.commit().await {
        log::warn!("Reset failed: commit: uuid={uuid}, error={e}");
        return render_error(&tmpl, base, "重置失败，请稍后重试", actix_web::http::StatusCode::INTERNAL_SERVER_ERROR);
    }

    log::info!("QR code slots reset: uuid={uuid}");
    HttpResponse::Found()
        .insert_header(("Location", format!("{base}/")))
        .finish()
}

// ---- 编辑 (需认证) ----

pub async fn edit_page(
    path: web::Path<String>,
    tmpl: web::Data<Tera>,
    pool: web::Data<MySqlPool>,
    session: Session,
    config: web::Data<Config>,
) -> HttpResponse {
    let uuid = path.into_inner();
    let base = &config.server.context_path;
    let username = session.get::<String>("user").unwrap_or(None).unwrap_or_default();

    let record = sqlx::query_as::<_, QrCodeRecord>(
        "SELECT * FROM qr_codes WHERE uuid = ?",
    )
    .bind(&uuid)
    .fetch_optional(pool.get_ref())
    .await
    .unwrap_or(None);

    let record = match record {
        Some(r) => r,
        None => return render_error(&tmpl, base, "二维码不存在", actix_web::http::StatusCode::NOT_FOUND),
    };

    let segments = parse_segments(&record.text_content);

    let mut ctx = Context::new();
    ctx.insert("base", base);
    ctx.insert("username", &username);
    ctx.insert("record", &record);
    ctx.insert("segments", &segments);

    render_template(&tmpl, "edit.html", &ctx)
}

pub async fn edit_handler(
    path: web::Path<String>,
    form: web::Form<CreateForm>,
    tmpl: web::Data<Tera>,
    pool: web::Data<MySqlPool>,
    config: web::Data<Config>,
) -> HttpResponse {
    let uuid = path.into_inner();
    let base = &config.server.context_path;

    // text_content 应该是 JSON 数组格式
    let text_content = form.text_content.trim();
    let segments: Vec<String> = match serde_json::from_str::<Vec<String>>(text_content) {
        Ok(segs) => segs.into_iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(),
        Err(_) => {
            let trimmed = text_content.to_string();
            if trimmed.is_empty() {
                vec![]
            } else {
                vec![trimmed]
            }
        }
    };

    if segments.is_empty() {
        return render_error(
            &tmpl,
            base,
            "文字内容不能为空",
            actix_web::http::StatusCode::BAD_REQUEST,
        );
    }

    let total_len: usize = segments.iter().map(|s| s.len()).sum();
    if total_len > 5000 {
        return render_error(
            &tmpl,
            base,
            "文字内容总长度不能超过 5000 字符",
            actix_web::http::StatusCode::BAD_REQUEST,
        );
    }

    let text_content_json = serde_json::to_string(&segments).unwrap_or_default();
    let max_count = form.max_count.unwrap_or(5).clamp(1, 10000);
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

// ---- 提取记录页面 (需认证) ----

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
    let username = session.get::<String>("user").unwrap_or(None).unwrap_or_default();
    let page = query.page.unwrap_or(1).clamp(1, 100_000);
    let list_page = query.list_page.unwrap_or(1);
    let list_keyword = query.list_keyword.clone().unwrap_or_default();
    let offset = (page - 1) * PAGE_SIZE;
    log::debug!("Extract logs page: uuid={uuid}, page={page}");

    let record = sqlx::query_as::<_, QrCodeRecord>(
        "SELECT * FROM qr_codes WHERE uuid = ?",
    )
    .bind(&uuid)
    .fetch_optional(pool.get_ref())
    .await
    .unwrap_or(None);

    let record = match record {
        Some(r) => r,
        None => return render_error(&tmpl, base, "二维码不存在", actix_web::http::StatusCode::NOT_FOUND),
    };

    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM qr_extract_logs WHERE qrcode_id = ?",
    )
    .bind(record.id)
    .fetch_one(pool.get_ref())
    .await
    .unwrap_or(0);

    let logs = sqlx::query_as::<_, ExtractLog>(
        "SELECT * FROM qr_extract_logs WHERE qrcode_id = ? ORDER BY extracted_at DESC LIMIT ? OFFSET ?",
    )
    .bind(record.id)
    .bind(PAGE_SIZE)
    .bind(offset)
    .fetch_all(pool.get_ref())
    .await
    .unwrap_or_default();

    let total_pages = (total + PAGE_SIZE - 1) / PAGE_SIZE;

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
