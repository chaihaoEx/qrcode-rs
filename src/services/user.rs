//! 用户管理服务
//!
//! 提供管理员用户的 CRUD 操作、密码管理和登录凭证验证。
//! 包含用户名和密码的格式校验、bcrypt 加密、账户锁定机制。

use chrono::NaiveDateTime;
use sqlx::MySqlPool;

use crate::models::AdminUser;

/// 校验用户名格式：去除首尾空白后长度需在 1-100 字符之间。
///
/// 返回去除空白后的用户名，或格式错误的提示信息。
pub(crate) fn validate_username(username: &str) -> Result<String, String> {
    let trimmed = username.trim();
    if trimmed.is_empty() || trimmed.len() > 100 {
        return Err("用户名长度需在 1-100 字符之间".to_string());
    }
    Ok(trimmed.to_string())
}

/// 校验密码长度：需在 8-200 字符之间。
///
/// `label` 参数用于生成友好的错误提示（如 "密码" 或 "新密码"）。
pub(crate) fn validate_password(password: &str, label: &str) -> Result<(), String> {
    if password.len() < 8 || password.len() > 200 {
        return Err(format!("{label}长度需在 8-200 字符之间"));
    }
    Ok(())
}

/// 查询所有管理员用户列表，按创建时间升序排列。
pub async fn list_users(pool: &MySqlPool) -> Result<Vec<AdminUser>, sqlx::Error> {
    sqlx::query_as::<_, AdminUser>("SELECT * FROM admin_users ORDER BY created_at ASC")
        .fetch_all(pool)
        .await
}

/// 创建新的管理员用户。
///
/// 校验用户名和密码格式后，使用 bcrypt（cost=12）加密密码并写入数据库。
/// 如果用户名已存在，返回友好的错误提示。
pub async fn create_user(pool: &MySqlPool, username: &str, password: &str) -> Result<(), String> {
    let username = validate_username(username)?;
    validate_password(password, "密码")?;

    let password_hash = bcrypt::hash(password, 12).map_err(|e| format!("密码加密失败: {e}"))?;

    sqlx::query("INSERT INTO admin_users (username, password_hash) VALUES (?, ?)")
        .bind(username)
        .bind(&password_hash)
        .execute(pool)
        .await
        .map_err(|e| {
            if e.to_string().contains("Duplicate entry") {
                "用户名已存在".to_string()
            } else {
                format!("创建用户失败: {e}")
            }
        })?;

    Ok(())
}

/// 切换用户的启用/禁用状态。
///
/// 同时重置失败登录次数和锁定时间，确保重新启用的用户可以立即登录。
pub async fn toggle_user(pool: &MySqlPool, id: u32, is_active: bool) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE admin_users SET is_active = ?, failed_attempts = 0, locked_until = NULL WHERE id = ?")
        .bind(is_active)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// 修改数据库用户的密码。
///
/// 先验证旧密码正确性，然后用 bcrypt 加密新密码并更新。
/// 仅限数据库用户使用（超级管理员通过配置文件管理密码）。
pub async fn change_password(
    pool: &MySqlPool,
    username: &str,
    old_password: &str,
    new_password: &str,
) -> Result<(), String> {
    validate_password(new_password, "新密码")?;

    // 查询用户并验证旧密码
    let user = sqlx::query_as::<_, AdminUser>("SELECT * FROM admin_users WHERE username = ?")
        .bind(username)
        .fetch_optional(pool)
        .await
        .map_err(|e| format!("查询失败: {e}"))?
        .ok_or_else(|| "用户不存在".to_string())?;

    if !bcrypt::verify(old_password, &user.password_hash).unwrap_or(false) {
        return Err("旧密码错误".to_string());
    }

    // 加密新密码并更新
    let new_hash = bcrypt::hash(new_password, 12).map_err(|e| format!("密码加密失败: {e}"))?;

    sqlx::query("UPDATE admin_users SET password_hash = ? WHERE username = ?")
        .bind(&new_hash)
        .bind(username)
        .execute(pool)
        .await
        .map_err(|e| format!("更新密码失败: {e}"))?;

    Ok(())
}

/// 验证数据库用户的登录凭证。
///
/// 返回值说明：
/// - `Ok(Some(username))` — 登录成功
/// - `Ok(None)` — 用户不存在（由调用方回退到其他认证方式）
/// - `Err(message)` — 登录失败（账户禁用、锁定或密码错误）
///
/// 登录失败处理：
/// - 累计失败次数，达到 5 次后锁定账户 30 分钟
/// - 登录成功时重置失败次数和锁定状态
pub async fn verify_db_user(
    pool: &MySqlPool,
    username: &str,
    password: &str,
) -> Result<Option<String>, String> {
    let user = match sqlx::query_as::<_, AdminUser>("SELECT * FROM admin_users WHERE username = ?")
        .bind(username)
        .fetch_optional(pool)
        .await
    {
        Ok(Some(u)) => u,
        Ok(None) => return Ok(None),
        Err(e) => {
            log::warn!("DB user query failed: {e}");
            return Ok(None);
        }
    };

    // ---- 账户状态检查 ----
    if !user.is_active {
        return Err("账户已被禁用".to_string());
    }

    // 检查是否处于锁定期
    if let Some(locked_until) = user.locked_until {
        let now = chrono::Utc::now().naive_utc();
        if now < locked_until {
            return Err("账户已被锁定，请稍后再试".to_string());
        }
    }

    // ---- 密码验证 ----
    if bcrypt::verify(password, &user.password_hash).unwrap_or(false) {
        // 验证成功：重置失败计数
        let _ = sqlx::query(
            "UPDATE admin_users SET failed_attempts = 0, locked_until = NULL WHERE id = ?",
        )
        .bind(user.id)
        .execute(pool)
        .await;
        Ok(Some(user.username))
    } else {
        // 验证失败：递增失败次数，达到阈值则锁定
        let new_attempts = user.failed_attempts + 1;
        if new_attempts >= 5 {
            // 锁定 30 分钟
            let lock_until: NaiveDateTime =
                chrono::Utc::now().naive_utc() + chrono::Duration::minutes(30);
            let _ = sqlx::query(
                "UPDATE admin_users SET failed_attempts = ?, locked_until = ? WHERE id = ?",
            )
            .bind(new_attempts)
            .bind(lock_until)
            .execute(pool)
            .await;
        } else {
            let _ = sqlx::query("UPDATE admin_users SET failed_attempts = ? WHERE id = ?")
                .bind(new_attempts)
                .bind(user.id)
                .execute(pool)
                .await;
        }
        Err("密码错误".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_username_valid() {
        assert_eq!(validate_username("alice").unwrap(), "alice");
    }

    #[test]
    fn test_validate_username_trims() {
        assert_eq!(validate_username("  bob  ").unwrap(), "bob");
    }

    #[test]
    fn test_validate_username_empty() {
        assert!(validate_username("").is_err());
    }

    #[test]
    fn test_validate_username_only_spaces() {
        assert!(validate_username("   ").is_err());
    }

    #[test]
    fn test_validate_username_too_long() {
        let name = "a".repeat(101);
        assert!(validate_username(&name).is_err());
    }

    #[test]
    fn test_validate_username_at_limit() {
        let name = "a".repeat(100);
        assert!(validate_username(&name).is_ok());
    }

    #[test]
    fn test_validate_password_valid() {
        assert!(validate_password("12345678", "密码").is_ok());
    }

    #[test]
    fn test_validate_password_too_short() {
        assert!(validate_password("1234567", "密码").is_err());
    }
}
