//! CSRF（跨站请求伪造）防护模块
//!
//! 为每个会话生成唯一的 CSRF 令牌，并在所有管理后台的 POST 请求中验证。
//! 令牌存储在服务端 session 中，表单通过隐藏字段 `csrf_token` 提交。
//! 配合 `SameSite=Strict` Cookie 策略提供双层防护。

use actix_session::Session;

/// 确保会话中存在 CSRF 令牌，不存在则创建新令牌。
///
/// 返回当前会话的 CSRF 令牌字符串（32 个十六进制字符）。
/// 在渲染包含表单的页面时调用，将令牌注入模板上下文。
pub fn ensure_csrf_token(session: &Session) -> String {
    if let Ok(Some(token)) = session.get::<String>("csrf_token") {
        return token;
    }
    let token = uuid::Uuid::new_v4().to_string().replace('-', "");
    let _ = session.insert("csrf_token", &token);
    token
}

/// 验证表单提交的 CSRF 令牌是否与会话中存储的令牌匹配。
///
/// 匹配成功返回 `true`，令牌不匹配或会话中无令牌返回 `false`。
pub fn validate_csrf_token(session: &Session, token: &str) -> bool {
    match session.get::<String>("csrf_token") {
        Ok(Some(expected)) => expected == token,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    // Note: CSRF functions require actix Session which needs a running service.
    // These tests verify the token format logic only.

    #[test]
    fn test_uuid_format_no_hyphens() {
        let token = uuid::Uuid::new_v4().to_string().replace('-', "");
        assert_eq!(token.len(), 32);
        assert!(token.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_uuid_uniqueness() {
        let t1 = uuid::Uuid::new_v4().to_string().replace('-', "");
        let t2 = uuid::Uuid::new_v4().to_string().replace('-', "");
        assert_ne!(t1, t2);
    }
}
