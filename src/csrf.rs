use actix_session::Session;

/// Ensure a CSRF token exists in the session, creating one if needed.
pub fn ensure_csrf_token(session: &Session) -> String {
    if let Ok(Some(token)) = session.get::<String>("csrf_token") {
        return token;
    }
    let token = uuid::Uuid::new_v4().to_string().replace('-', "");
    let _ = session.insert("csrf_token", &token);
    token
}

/// Validate CSRF token from form against session.
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
