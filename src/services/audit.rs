//! 审计日志服务
//!
//! 记录管理员的所有操作行为（创建、编辑、删除、重置等），
//! 并提供按操作类型和关键词筛选的日志查询功能。

use sqlx::MySqlPool;

use crate::models::AuditLog;
use crate::utils::pagination::calc_page_offset;
use crate::utils::PAGE_SIZE;

/// 写入一条审计日志记录。
///
/// 写入失败时仅记录警告日志，不影响主业务流程。
/// 应在所有管理操作完成后调用。
///
/// # 参数
/// - `username` - 执行操作的管理员用户名
/// - `action` - 操作类型（如 `"create"`, `"delete"`, `"reset"`, `"edit"` 等）
/// - `target_uuid` - 操作目标的二维码 UUID（可选）
/// - `detail` - 操作详情描述（可选）
/// - `client_ip` - 客户端 IP 地址
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

/// 查询审计日志列表，支持按操作类型和关键词筛选，返回 `(总数, 日志列表)`。
///
/// 筛选逻辑根据 `action_filter` 和 `keyword` 的组合分为四种情况：
/// 1. 同时指定操作类型和关键词
/// 2. 仅指定操作类型
/// 3. 仅指定关键词（匹配用户名、目标 UUID 或 IP）
/// 4. 无筛选条件
///
/// 结果按创建时间倒序排列，支持分页。
pub async fn list_logs(
    pool: &MySqlPool,
    action_filter: &str,
    keyword: &str,
    page: Option<i64>,
) -> Result<(i64, Vec<AuditLog>), sqlx::Error> {
    let (_page, offset) = calc_page_offset(page);

    // 根据筛选条件组合构建不同的 SQL 查询
    let (count_sql, query_sql) = if !action_filter.is_empty() && !keyword.is_empty() {
        // 操作类型 + 关键词
        (
            "SELECT COUNT(*) FROM admin_audit_logs WHERE action = ? AND (username LIKE ? OR target_uuid LIKE ? OR client_ip LIKE ?)".to_string(),
            "SELECT * FROM admin_audit_logs WHERE action = ? AND (username LIKE ? OR target_uuid LIKE ? OR client_ip LIKE ?) ORDER BY created_at DESC LIMIT ? OFFSET ?".to_string(),
        )
    } else if !action_filter.is_empty() {
        // 仅操作类型
        (
            "SELECT COUNT(*) FROM admin_audit_logs WHERE action = ?".to_string(),
            "SELECT * FROM admin_audit_logs WHERE action = ? ORDER BY created_at DESC LIMIT ? OFFSET ?".to_string(),
        )
    } else if !keyword.is_empty() {
        // 仅关键词
        (
            "SELECT COUNT(*) FROM admin_audit_logs WHERE username LIKE ? OR target_uuid LIKE ? OR client_ip LIKE ?".to_string(),
            "SELECT * FROM admin_audit_logs WHERE (username LIKE ? OR target_uuid LIKE ? OR client_ip LIKE ?) ORDER BY created_at DESC LIMIT ? OFFSET ?".to_string(),
        )
    } else {
        // 无筛选
        (
            "SELECT COUNT(*) FROM admin_audit_logs".to_string(),
            "SELECT * FROM admin_audit_logs ORDER BY created_at DESC LIMIT ? OFFSET ?".to_string(),
        )
    };

    let like_keyword = format!("%{keyword}%");

    // 执行 COUNT 查询（根据筛选条件绑定不同参数）
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

    // 执行数据查询（根据筛选条件绑定不同参数）
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
