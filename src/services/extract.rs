use sqlx::MySqlPool;

use crate::models::{ClaimResponse, QrCodeRecord};
use crate::utils::validation::parse_segments;

/// Check if a QR code exists by UUID. Returns its id if found.
pub async fn check_exists(pool: &MySqlPool, uuid: &str) -> Result<Option<u64>, sqlx::Error> {
    sqlx::query_scalar("SELECT id FROM qr_codes WHERE uuid = ?")
        .bind(uuid)
        .fetch_optional(pool)
        .await
}

/// Claim a slot for a browser_id. Idempotent: returns cached segment if already claimed.
/// Uses SELECT ... FOR UPDATE row lock + transaction to prevent concurrent over-allocation.
pub async fn claim_slot(
    pool: &MySqlPool,
    uuid: &str,
    browser_id: &str,
    client_ip: &str,
) -> Result<ClaimResponse, String> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| format!("begin transaction failed: {e}"))?;

    // Row lock
    let record = sqlx::query_as::<_, QrCodeRecord>(
        "SELECT * FROM qr_codes WHERE uuid = ? FOR UPDATE",
    )
    .bind(uuid)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| {
        format!("query failed: {e}")
    })?;

    let record = match record {
        Some(r) => r,
        None => {
            let _ = tx.rollback().await;
            return Ok(ClaimResponse {
                status: "not_found".to_string(),
                text_content: None,
                segment_index: None,
            });
        }
    };

    // Idempotency: check if browser_id already has a slot
    let existing_slot: Option<u32> = sqlx::query_scalar(
        "SELECT segment_index FROM qr_browser_slots WHERE qrcode_id = ? AND browser_id = ?",
    )
    .bind(record.id)
    .bind(browser_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| format!("query slot failed: {e}"))?;

    if let Some(seg_idx) = existing_slot {
        let _ = tx.commit().await;
        let segments = parse_segments(&record.text_content);
        let text = segments.get(seg_idx as usize).cloned().unwrap_or_default();
        log::debug!(
            "Extract claim idempotent: uuid={uuid}, browser_id={browser_id}, segment={seg_idx}"
        );
        return Ok(ClaimResponse {
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
        return Ok(ClaimResponse {
            status: "exhausted".to_string(),
            text_content: None,
            segment_index: None,
        });
    }

    let segment_index = record.used_count as u32;
    let text = segments[segment_index as usize].clone();

    // Insert slot
    sqlx::query(
        "INSERT INTO qr_browser_slots (qrcode_id, browser_id, segment_index, client_ip) VALUES (?, ?, ?, ?)",
    )
    .bind(record.id)
    .bind(browser_id)
    .bind(segment_index)
    .bind(client_ip)
    .execute(&mut *tx)
    .await
    .map_err(|e| format!("insert slot failed: {e}"))?;

    // Update main table
    sqlx::query(
        "UPDATE qr_codes SET used_count = used_count + 1, last_extract_ip = ?, last_extract_at = NOW() WHERE id = ?",
    )
    .bind(client_ip)
    .bind(record.id)
    .execute(&mut *tx)
    .await
    .map_err(|e| format!("update qr_codes failed: {e}"))?;

    // Write extract log
    sqlx::query(
        "INSERT INTO qr_extract_logs (qrcode_id, client_ip, browser_id, segment_index, extracted_at) VALUES (?, ?, ?, ?, NOW())",
    )
    .bind(record.id)
    .bind(client_ip)
    .bind(browser_id)
    .bind(segment_index)
    .execute(&mut *tx)
    .await
    .map_err(|e| format!("insert log failed: {e}"))?;

    tx.commit()
        .await
        .map_err(|e| format!("commit failed: {e}"))?;

    log::info!("Extract claim success: uuid={uuid}, ip={client_ip}, browser_id={browser_id}, segment={segment_index}");

    Ok(ClaimResponse {
        status: "ok".to_string(),
        text_content: Some(text),
        segment_index: Some(segment_index),
    })
}
