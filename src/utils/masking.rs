/// Mask an IP address: "192.168.1.100" -> "192.168.1.*"
pub fn mask_ip(ip: &str) -> String {
    if let Some(pos) = ip.rfind('.') {
        format!("{}.*", &ip[..pos])
    } else if let Some(pos) = ip.find(':') {
        format!("{}:*", &ip[..pos])
    } else {
        "***".to_string()
    }
}

/// Mask a browser_id: "550e8400-e29b-41d4-..." -> "550e8400..."
pub fn mask_browser_id(bid: &str) -> String {
    if bid.len() > 8 {
        format!("{}...", &bid[..8])
    } else {
        "***".to_string()
    }
}

/// Mask a username: "admin" -> "a***"
pub fn mask_username(name: &str) -> String {
    if let Some(first) = name.chars().next() {
        format!("{first}***")
    } else {
        "***".to_string()
    }
}

/// Mask a UUID: "550e8400-e29b-41d4-..." -> "550e8400..."
pub fn mask_uuid(uuid: &str) -> String {
    if uuid.len() > 8 {
        format!("{}...", &uuid[..8])
    } else {
        "***".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mask_ip_v4() {
        assert_eq!(mask_ip("192.168.1.100"), "192.168.1.*");
    }

    #[test]
    fn test_mask_ip_v6() {
        assert_eq!(mask_ip("2001:db8::1"), "2001:*");
    }

    #[test]
    fn test_mask_ip_empty() {
        assert_eq!(mask_ip(""), "***");
    }

    #[test]
    fn test_mask_browser_id() {
        assert_eq!(
            mask_browser_id("550e8400-e29b-41d4-a716-446655440000"),
            "550e8400..."
        );
    }

    #[test]
    fn test_mask_browser_id_short() {
        assert_eq!(mask_browser_id("short"), "***");
    }

    #[test]
    fn test_mask_username() {
        assert_eq!(mask_username("admin"), "a***");
    }

    #[test]
    fn test_mask_username_empty() {
        assert_eq!(mask_username(""), "***");
    }

    #[test]
    fn test_mask_uuid() {
        assert_eq!(mask_uuid("550e8400-e29b-41d4"), "550e8400...");
    }

    #[test]
    fn test_mask_uuid_short() {
        assert_eq!(mask_uuid("short"), "***");
    }
}
