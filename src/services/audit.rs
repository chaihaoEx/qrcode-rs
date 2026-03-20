use sqlx::MySqlPool;

use crate::models::AuditLog;
use crate::utils::pagination::calc_page_offset;
use crate::utils::PAGE_SIZE;

pub async fn log_action(
    pool: &MySqlPool,
    username: &str,
    action: &str,
    target_uuid: Option<&str>,
    detail: Option<&str>,
    client_ip: &str,
) {
    let result = sqlx::query(
        "INSERT INTO admin_audit_logs (username, action, target_uuid, detail, client_ip) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(username)
    .bind(action)
    .bind(target_uuid)
    .bind(detail)
    .bind(client_ip)
    .execute(pool)
    .await;

    if let Err(e) = result {
        log::warn!("Audit log write failed: {e}");
    }
}

pub async fn list_logs(
    pool: &MySqlPool,
    action_filter: &str,
    keyword: &str,
    page: Option<i64>,
) -> Result<(i64, Vec<AuditLog>), sqlx::Error> {
    let (_page, offset) = calc_page_offset(page);

    let (count_sql, query_sql) = if !action_filter.is_empty() && !keyword.is_empty() {
        (
            "SELECT COUNT(*) FROM admin_audit_logs WHERE action = ? AND (username LIKE ? OR target_uuid LIKE ? OR client_ip LIKE ?)".to_string(),
            "SELECT * FROM admin_audit_logs WHERE action = ? AND (username LIKE ? OR target_uuid LIKE ? OR client_ip LIKE ?) ORDER BY created_at DESC LIMIT ? OFFSET ?".to_string(),
        )
    } else if !action_filter.is_empty() {
        (
            "SELECT COUNT(*) FROM admin_audit_logs WHERE action = ?".to_string(),
            "SELECT * FROM admin_audit_logs WHERE action = ? ORDER BY created_at DESC LIMIT ? OFFSET ?".to_string(),
        )
    } else if !keyword.is_empty() {
        (
            "SELECT COUNT(*) FROM admin_audit_logs WHERE username LIKE ? OR target_uuid LIKE ? OR client_ip LIKE ?".to_string(),
            "SELECT * FROM admin_audit_logs WHERE (username LIKE ? OR target_uuid LIKE ? OR client_ip LIKE ?) ORDER BY created_at DESC LIMIT ? OFFSET ?".to_string(),
        )
    } else {
        (
            "SELECT COUNT(*) FROM admin_audit_logs".to_string(),
            "SELECT * FROM admin_audit_logs ORDER BY created_at DESC LIMIT ? OFFSET ?".to_string(),
        )
    };

    let like_keyword = format!("%{keyword}%");

    let total: i64 = if !action_filter.is_empty() && !keyword.is_empty() {
        sqlx::query_scalar(&count_sql)
            .bind(action_filter)
            .bind(&like_keyword)
            .bind(&like_keyword)
            .bind(&like_keyword)
            .fetch_one(pool)
            .await?
    } else if !action_filter.is_empty() {
        sqlx::query_scalar(&count_sql)
            .bind(action_filter)
            .fetch_one(pool)
            .await?
    } else if !keyword.is_empty() {
        sqlx::query_scalar(&count_sql)
            .bind(&like_keyword)
            .bind(&like_keyword)
            .bind(&like_keyword)
            .fetch_one(pool)
            .await?
    } else {
        sqlx::query_scalar(&count_sql).fetch_one(pool).await?
    };

    let logs: Vec<AuditLog> = if !action_filter.is_empty() && !keyword.is_empty() {
        sqlx::query_as(&query_sql)
            .bind(action_filter)
            .bind(&like_keyword)
            .bind(&like_keyword)
            .bind(&like_keyword)
            .bind(PAGE_SIZE)
            .bind(offset)
            .fetch_all(pool)
            .await?
    } else if !action_filter.is_empty() {
        sqlx::query_as(&query_sql)
            .bind(action_filter)
            .bind(PAGE_SIZE)
            .bind(offset)
            .fetch_all(pool)
            .await?
    } else if !keyword.is_empty() {
        sqlx::query_as(&query_sql)
            .bind(&like_keyword)
            .bind(&like_keyword)
            .bind(&like_keyword)
            .bind(PAGE_SIZE)
            .bind(offset)
            .fetch_all(pool)
            .await?
    } else {
        sqlx::query_as(&query_sql)
            .bind(PAGE_SIZE)
            .bind(offset)
            .fetch_all(pool)
            .await?
    };

    Ok((total, logs))
}
