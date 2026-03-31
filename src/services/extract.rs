//! 二维码内容提取服务
//!
//! 实现基于 `browser_id` + 槽位模型的内容提取逻辑。
//! 每个浏览器通过唯一的 `browser_id` 按顺序领取一个文本分段，
//! 使用数据库行锁和事务保证并发安全和幂等性。

use sqlx::MySqlPool;

use crate::models::{ClaimResponse, QrCodeRecord};
use crate::utils::validation::parse_segments;

/// 检查指定 UUID 的二维码是否存在，存在则返回其数据库 ID。
pub async fn check_exists(pool: &MySqlPool, uuid: &str) -> Result<Option<u64>, sqlx::Error> {
    sqlx::query_scalar("SELECT id FROM qr_codes WHERE uuid = ?")
        .bind(uuid)
        .fetch_optional(pool)
        .await
}

/// 为指定浏览器领取一个文本分段槽位。
///
/// 核心提取逻辑，具有以下特性：
/// - **幂等性**：同一 `browser_id` 重复请求返回已缓存的分段
/// - **并发安全**：使用 `SELECT ... FOR UPDATE` 行锁 + 事务防止超发
/// - **顺序分配**：按 `used_count` 递增顺序依次分配分段
///
/// # 参数
/// - `pool` - 数据库连接池
/// - `uuid` - 二维码 UUID
/// - `browser_id` - 浏览器唯一标识（UUID v4，客户端生成）
/// - `client_ip` - 客户端 IP 地址
///
/// # 返回
/// `ClaimResponse` 包含状态（`ok` / `exhausted` / `not_found`）和分配的内容
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

    // ---- 行锁查询二维码记录 ----
    let record =
        sqlx::query_as::<_, QrCodeRecord>("SELECT * FROM qr_codes WHERE uuid = ? FOR UPDATE")
            .bind(uuid)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| format!("query failed: {e}"))?;

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

    // ---- 幂等性检查：该浏览器是否已领取过 ----
    let existing_slot: Option<u32> = sqlx::query_scalar(
        "SELECT segment_index FROM qr_browser_slots WHERE qrcode_id = ? AND browser_id = ?",
    )
    .bind(record.id)
    .bind(browser_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| format!("query slot failed: {e}"))?;

    if let Some(seg_idx) = existing_slot {
        // 已领取过，直接返回缓存的分段
        let _ = tx.commit().await;
        let segments = parse_segments(&record.text_content);
        let text = segments.get(seg_idx as usize).cloned().unwrap_or_default();
        log::debug!("Extract claim idempotent: segment={seg_idx}");
        return Ok(ClaimResponse {
            status: "ok".to_string(),
            text_content: Some(text),
            segment_index: Some(seg_idx),
        });
    }

    // ---- 检查是否已用完 ----
    let segments = parse_segments(&record.text_content);

    if record.used_count as usize >= segments.len() {
        let _ = tx.rollback().await;
        log::warn!(
            "Extract claim exhausted: used={}, segments={}",
            record.used_count,
            segments.len()
        );
        return Ok(ClaimResponse {
            status: "exhausted".to_string(),
            text_content: None,
            segment_index: None,
        });
    }

    // ---- 分配新槽位 ----
    let segment_index = record.used_count as u32;
    let text = segments[segment_index as usize].clone();

    // 插入浏览器槽位记录
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

    // 更新主表的已使用计数和最后提取信息
    sqlx::query(
        "UPDATE qr_codes SET used_count = used_count + 1, last_extract_ip = ?, last_extract_at = NOW() WHERE id = ?",
    )
    .bind(client_ip)
    .bind(record.id)
    .execute(&mut *tx)
    .await
    .map_err(|e| format!("update qr_codes failed: {e}"))?;

    // 写入提取日志
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

    log::info!("Extract claim success: segment={segment_index}");

    Ok(ClaimResponse {
        status: "ok".to_string(),
        text_content: Some(text),
        segment_index: Some(segment_index),
    })
}
