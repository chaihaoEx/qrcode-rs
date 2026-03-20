use sqlx::MySqlPool;

use crate::models::{ExtractLog, QrCodeRecord};
use crate::utils::PAGE_SIZE;

/// List QR codes with optional keyword search, returns (total, records).
pub async fn list_qrcodes(
    pool: &MySqlPool,
    keyword: &str,
    offset: i64,
) -> Result<(i64, Vec<QrCodeRecord>), sqlx::Error> {
    if keyword.is_empty() {
        let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM qr_codes")
            .fetch_one(pool)
            .await?;

        let records = sqlx::query_as::<_, QrCodeRecord>(
            "SELECT * FROM qr_codes ORDER BY created_at DESC LIMIT ? OFFSET ?",
        )
        .bind(PAGE_SIZE)
        .bind(offset)
        .fetch_all(pool)
        .await?;

        Ok((total, records))
    } else {
        // Note: %keyword% LIKE on TEXT column cannot use index.
        // At current scale this is acceptable; consider full-text search if data grows large.
        let like_pattern = format!("%{keyword}%");

        let total: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM qr_codes WHERE text_content LIKE ? OR remark LIKE ?",
        )
        .bind(&like_pattern)
        .bind(&like_pattern)
        .fetch_one(pool)
        .await?;

        let records = sqlx::query_as::<_, QrCodeRecord>(
            "SELECT * FROM qr_codes WHERE text_content LIKE ? OR remark LIKE ? ORDER BY created_at DESC LIMIT ? OFFSET ?",
        )
        .bind(&like_pattern)
        .bind(&like_pattern)
        .bind(PAGE_SIZE)
        .bind(offset)
        .fetch_all(pool)
        .await?;

        Ok((total, records))
    }
}

/// Fetch a single QR code by UUID.
pub async fn get_by_uuid(
    pool: &MySqlPool,
    uuid: &str,
) -> Result<Option<QrCodeRecord>, sqlx::Error> {
    sqlx::query_as::<_, QrCodeRecord>("SELECT * FROM qr_codes WHERE uuid = ?")
        .bind(uuid)
        .fetch_optional(pool)
        .await
}

/// Create a new QR code, returns the generated UUID.
pub async fn create(
    pool: &MySqlPool,
    text_content_json: &str,
    remark: Option<&str>,
    max_count: u32,
) -> Result<String, sqlx::Error> {
    let uuid = uuid::Uuid::new_v4().to_string();

    sqlx::query(
        "INSERT INTO qr_codes (uuid, text_content, remark, max_count, used_count, created_at) VALUES (?, ?, ?, ?, 0, NOW())",
    )
    .bind(&uuid)
    .bind(text_content_json)
    .bind(remark)
    .bind(max_count)
    .execute(pool)
    .await?;

    Ok(uuid)
}

/// Update an existing QR code.
pub async fn update(
    pool: &MySqlPool,
    uuid: &str,
    text_content_json: &str,
    remark: Option<&str>,
    max_count: u32,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE qr_codes SET text_content = ?, remark = ?, max_count = ? WHERE uuid = ?")
        .bind(text_content_json)
        .bind(remark)
        .bind(max_count)
        .bind(uuid)
        .execute(pool)
        .await?;

    Ok(())
}

/// Delete a QR code by UUID.
pub async fn delete(pool: &MySqlPool, uuid: &str) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM qr_codes WHERE uuid = ?")
        .bind(uuid)
        .execute(pool)
        .await?;

    Ok(())
}

/// Reset slots for a QR code (delete browser slots and reset used_count).
pub async fn reset_slots(pool: &MySqlPool, uuid: &str) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;

    sqlx::query(
        "DELETE FROM qr_browser_slots WHERE qrcode_id = (SELECT id FROM qr_codes WHERE uuid = ?)",
    )
    .bind(uuid)
    .execute(&mut *tx)
    .await?;

    sqlx::query("UPDATE qr_codes SET used_count = 0 WHERE uuid = ?")
        .bind(uuid)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;
    Ok(())
}

/// List extract logs for a QR code, returns (total, logs).
pub async fn list_extract_logs(
    pool: &MySqlPool,
    qrcode_id: u64,
    offset: i64,
) -> Result<(i64, Vec<ExtractLog>), sqlx::Error> {
    let total: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM qr_extract_logs WHERE qrcode_id = ?")
            .bind(qrcode_id)
            .fetch_one(pool)
            .await?;

    let logs = sqlx::query_as::<_, ExtractLog>(
        "SELECT * FROM qr_extract_logs WHERE qrcode_id = ? ORDER BY extracted_at DESC LIMIT ? OFFSET ?",
    )
    .bind(qrcode_id)
    .bind(PAGE_SIZE)
    .bind(offset)
    .fetch_all(pool)
    .await?;

    Ok((total, logs))
}

/// Generate a QR code PNG image from a URL.
pub fn generate_qr_image(url: &str) -> Result<Vec<u8>, String> {
    use image::ImageEncoder;

    let qr = qrcode::QrCode::new(url.as_bytes())
        .map_err(|e| format!("Failed to generate QR code: {e}"))?;

    let img = qr
        .render::<image::Luma<u8>>()
        .min_dimensions(256, 256)
        .build();

    let mut buf = std::io::Cursor::new(Vec::new());
    let encoder = image::codecs::png::PngEncoder::new(&mut buf);
    encoder
        .write_image(
            img.as_raw(),
            img.width(),
            img.height(),
            image::ExtendedColorType::L8,
        )
        .map_err(|e| format!("Failed to encode PNG: {e}"))?;

    Ok(buf.into_inner())
}
