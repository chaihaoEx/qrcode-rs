use chrono::NaiveDateTime;
use sqlx::MySqlPool;

use crate::models::AdminUser;

/// List all admin users.
pub async fn list_users(pool: &MySqlPool) -> Result<Vec<AdminUser>, sqlx::Error> {
    sqlx::query_as::<_, AdminUser>("SELECT * FROM admin_users ORDER BY created_at ASC")
        .fetch_all(pool)
        .await
}

/// Create a new admin user. Returns error message on failure.
pub async fn create_user(pool: &MySqlPool, username: &str, password: &str) -> Result<(), String> {
    let username = username.trim();
    if username.is_empty() || username.len() > 100 {
        return Err("用户名长度需在 1-100 字符之间".to_string());
    }

    if password.len() < 8 || password.len() > 200 {
        return Err("密码长度需在 8-200 字符之间".to_string());
    }

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

/// Toggle user active status.
pub async fn toggle_user(pool: &MySqlPool, id: u32, is_active: bool) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE admin_users SET is_active = ?, failed_attempts = 0, locked_until = NULL WHERE id = ?")
        .bind(is_active)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Change password for a DB user. Verifies old password first.
pub async fn change_password(
    pool: &MySqlPool,
    username: &str,
    old_password: &str,
    new_password: &str,
) -> Result<(), String> {
    if new_password.len() < 8 || new_password.len() > 200 {
        return Err("新密码长度需在 8-200 字符之间".to_string());
    }

    let user = sqlx::query_as::<_, AdminUser>("SELECT * FROM admin_users WHERE username = ?")
        .bind(username)
        .fetch_optional(pool)
        .await
        .map_err(|e| format!("查询失败: {e}"))?
        .ok_or_else(|| "用户不存在".to_string())?;

    if !bcrypt::verify(old_password, &user.password_hash).unwrap_or(false) {
        return Err("旧密码错误".to_string());
    }

    let new_hash = bcrypt::hash(new_password, 12).map_err(|e| format!("密码加密失败: {e}"))?;

    sqlx::query("UPDATE admin_users SET password_hash = ? WHERE username = ?")
        .bind(&new_hash)
        .bind(username)
        .execute(pool)
        .await
        .map_err(|e| format!("更新密码失败: {e}"))?;

    Ok(())
}

/// Verify a database user's credentials for login.
/// Returns Ok(Some(username)) on success, Ok(None) if user not found,
/// or Err with a message explaining why login failed (locked, inactive, wrong password).
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

    if !user.is_active {
        return Err("账户已被禁用".to_string());
    }

    // Check if locked
    if let Some(locked_until) = user.locked_until {
        let now = chrono::Utc::now().naive_utc();
        if now < locked_until {
            return Err("账户已被锁定，请稍后再试".to_string());
        }
    }

    if bcrypt::verify(password, &user.password_hash).unwrap_or(false) {
        // Reset failed attempts on success
        let _ = sqlx::query(
            "UPDATE admin_users SET failed_attempts = 0, locked_until = NULL WHERE id = ?",
        )
        .bind(user.id)
        .execute(pool)
        .await;
        Ok(Some(user.username))
    } else {
        // Increment failed attempts
        let new_attempts = user.failed_attempts + 1;
        if new_attempts >= 5 {
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
