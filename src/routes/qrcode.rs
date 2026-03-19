use actix_session::Session;
use actix_web::{web, HttpRequest, HttpResponse};
use chrono::NaiveDateTime;
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

fn render_error(tmpl: &Tera, base: &str, message: &str, status: actix_web::http::StatusCode) -> HttpResponse {
    let mut ctx = Context::new();
    ctx.insert("base", base);
    ctx.insert("message", message);
    let rendered = tmpl.render("error.html", &ctx).unwrap();
    HttpResponse::build(status).content_type("text/html").body(rendered)
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
    pub created_at: NaiveDateTime,
    #[sqlx(default)]
    pub last_extract_at: Option<NaiveDateTime>,
}

#[derive(sqlx::FromRow, Serialize)]
pub struct ExtractLog {
    pub id: u64,
    pub qrcode_id: u64,
    pub client_ip: String,
    pub extracted_at: NaiveDateTime,
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
    let page = query.page.unwrap_or(1).max(1);
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

    let total_pages = ((total as f64) / (PAGE_SIZE as f64)).ceil() as i64;

    let mut ctx = Context::new();
    ctx.insert("base", base);
    ctx.insert("username", &username);
    ctx.insert("records", &records);
    ctx.insert("page", &page);
    ctx.insert("total_pages", &total_pages);
    ctx.insert("keyword", &keyword);

    let rendered = tmpl.render("list.html", &ctx).unwrap();
    HttpResponse::Ok().content_type("text/html").body(rendered)
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

    let rendered = tmpl.render("create.html", &ctx).unwrap();
    HttpResponse::Ok().content_type("text/html").body(rendered)
}

pub async fn create_handler(
    form: web::Form<CreateForm>,
    pool: web::Data<MySqlPool>,
    config: web::Data<Config>,
) -> HttpResponse {
    let base = &config.server.context_path;
    let uuid = uuid::Uuid::new_v4().to_string();
    let max_count = form.max_count.unwrap_or(5).max(1);
    let remark = form.remark.as_deref().filter(|s| !s.trim().is_empty());

    let _ = sqlx::query(
        "INSERT INTO qr_codes (uuid, text_content, remark, max_count, used_count, created_at) VALUES (?, ?, ?, ?, 0, NOW())",
    )
    .bind(&uuid)
    .bind(&form.text_content)
    .bind(remark)
    .bind(max_count)
    .execute(pool.get_ref())
    .await;

    log::info!("QR code created: uuid={uuid}, max_count={max_count}");

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

    let remaining = record.max_count.saturating_sub(record.used_count);

    let mut ctx = Context::new();
    ctx.insert("base", base);
    ctx.insert("uuid", &uuid);
    ctx.insert("hash", &hash);
    ctx.insert("created_at", &record.created_at.format("%Y-%m-%d %H:%M:%S").to_string());
    ctx.insert("remaining", &remaining);

    let rendered = tmpl.render("extract.html", &ctx).unwrap();
    HttpResponse::Ok().content_type("text/html").body(rendered)
}

pub async fn extract_handler(
    path: web::Path<(String, String)>,
    tmpl: web::Data<Tera>,
    pool: web::Data<MySqlPool>,
    config: web::Data<Config>,
    req: HttpRequest,
) -> HttpResponse {
    let (uuid, hash) = path.into_inner();
    let base = &config.server.context_path;

    if !verify_extract_hash(&uuid, &hash, &config.server.extract_salt) {
        return render_error(&tmpl, base, "无效二维码", actix_web::http::StatusCode::BAD_REQUEST);
    }

    let client_ip = req
        .connection_info()
        .realip_remote_addr()
        .unwrap_or("unknown")
        .to_string();

    // 原子更新，防止并发超额提取
    let result = sqlx::query(
        "UPDATE qr_codes SET used_count = used_count + 1, last_extract_ip = ?, last_extract_at = NOW() WHERE uuid = ? AND used_count < max_count",
    )
    .bind(&client_ip)
    .bind(&uuid)
    .execute(pool.get_ref())
    .await;

    let rows_affected = match &result {
        Ok(r) => r.rows_affected(),
        Err(e) => {
            log::warn!("Extract UPDATE failed: uuid={uuid}, error={e}");
            0
        }
    };

    if rows_affected > 0 {
        // 记录提取日志
        if let Err(e) = sqlx::query(
            "INSERT INTO qr_extract_logs (qrcode_id, client_ip, extracted_at) SELECT id, ?, NOW() FROM qr_codes WHERE uuid = ?",
        )
        .bind(&client_ip)
        .bind(&uuid)
        .execute(pool.get_ref())
        .await
        {
            log::warn!("Extract log INSERT failed: uuid={uuid}, error={e}");
        }
    }

    if rows_affected == 0 {
        // 可能是 uuid 不存在或次数已用完
        let record = sqlx::query_as::<_, QrCodeRecord>(
            "SELECT * FROM qr_codes WHERE uuid = ?",
        )
        .bind(&uuid)
        .fetch_optional(pool.get_ref())
        .await
        .unwrap_or(None);

        return match record {
            None => {
                log::warn!("Extract failed: uuid={uuid} not found, ip={client_ip}");
                render_error(&tmpl, base, "二维码不存在", actix_web::http::StatusCode::NOT_FOUND)
            }
            Some(_) => {
                log::warn!("Extract failed: uuid={uuid} exhausted, ip={client_ip}");
                let mut ctx = Context::new();
                ctx.insert("base", base);
                ctx.insert("uuid", &uuid);
                ctx.insert("hash", &hash);
                ctx.insert("remaining", &0u32);
                ctx.insert("created_at", &"");
                let rendered = tmpl.render("extract.html", &ctx).unwrap();
                HttpResponse::Ok().content_type("text/html").body(rendered)
            }
        };
    }

    // 提取成功
    log::info!("Extract success: uuid={uuid}, ip={client_ip}");
    let record = sqlx::query_as::<_, QrCodeRecord>(
        "SELECT * FROM qr_codes WHERE uuid = ?",
    )
    .bind(&uuid)
    .fetch_one(pool.get_ref())
    .await
    .unwrap();

    let remaining = record.max_count.saturating_sub(record.used_count);

    let mut ctx = Context::new();
    ctx.insert("base", base);
    ctx.insert("text_content", &record.text_content);
    ctx.insert("remaining", &remaining);

    let rendered = tmpl.render("extract_result.html", &ctx).unwrap();
    HttpResponse::Ok().content_type("text/html").body(rendered)
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
    let page = query.page.unwrap_or(1).max(1);
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

    let total_pages = ((total as f64) / (PAGE_SIZE as f64)).ceil() as i64;

    let mut ctx = Context::new();
    ctx.insert("base", base);
    ctx.insert("username", &username);
    ctx.insert("record", &record);
    ctx.insert("logs", &logs);
    ctx.insert("page", &page);
    ctx.insert("total_pages", &total_pages);
    ctx.insert("list_page", &list_page);
    ctx.insert("list_keyword", &list_keyword);

    let rendered = tmpl.render("logs.html", &ctx).unwrap();
    HttpResponse::Ok().content_type("text/html").body(rendered)
}
