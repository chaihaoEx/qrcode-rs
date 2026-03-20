use actix_web::HttpRequest;

use super::MAX_CONTENT_LENGTH;

/// Extract client IP from request, truncated to max 45 chars (IPv6 max length).
/// Binds `ConnectionInfo` to a local variable so the `&str` borrow remains valid,
/// then slices before allocating to avoid CodeQL "uncontrolled allocation" warnings.
pub fn get_client_ip(req: &HttpRequest) -> String {
    let info = req.connection_info();
    let raw = info.realip_remote_addr().unwrap_or("unknown");
    let end = raw.len().min(45);
    raw[..end].to_string()
}

/// Parse text_content into segments. Falls back to single segment if not JSON array.
pub fn parse_segments(text_content: &str) -> Vec<String> {
    match serde_json::from_str::<Vec<String>>(text_content) {
        Ok(segments) if !segments.is_empty() => segments,
        _ => vec![text_content.to_string()],
    }
}

/// Truncate display: first 12 chars of first segment + "..."
pub fn truncate_display(text_content: &str) -> String {
    let segments = parse_segments(text_content);
    let first = segments.first().map(|s| s.as_str()).unwrap_or("");
    let first = first.replace('\n', " ").replace('\r', "");
    let chars: Vec<char> = first.chars().collect();
    if chars.len() > 12 {
        format!("{}...", chars[..12].iter().collect::<String>())
    } else {
        first.to_string()
    }
}

/// Validate and parse segments from form input.
/// Returns (segments, json_string) or an error message.
pub fn validate_segments(raw: &str) -> Result<(Vec<String>, String), &'static str> {
    let text_content = raw.trim();
    let segments: Vec<String> = match serde_json::from_str::<Vec<String>>(text_content) {
        Ok(segs) => segs
            .into_iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect(),
        Err(_) => {
            let trimmed = text_content.to_string();
            if trimmed.is_empty() {
                vec![]
            } else {
                vec![trimmed]
            }
        }
    };

    if segments.is_empty() {
        return Err("文字内容不能为空");
    }

    let total_len: usize = segments.iter().map(|s| s.len()).sum();
    if total_len > MAX_CONTENT_LENGTH {
        return Err("文字内容总长度不能超过 5000 字符");
    }

    let json = serde_json::to_string(&segments).unwrap_or_default();
    Ok((segments, json))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_segments_json_array() {
        let result = parse_segments(r#"["a","b","c"]"#);
        assert_eq!(result, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_parse_segments_plain_string() {
        let result = parse_segments("hello world");
        assert_eq!(result, vec!["hello world"]);
    }

    #[test]
    fn test_parse_segments_empty_array() {
        let result = parse_segments("[]");
        assert_eq!(result, vec!["[]"]);
    }

    #[test]
    fn test_truncate_display_short() {
        assert_eq!(truncate_display("短文字"), "短文字");
    }

    #[test]
    fn test_truncate_display_long() {
        let long = "这是一段非常非常非常长的文字内容";
        let result = truncate_display(long);
        assert!(result.ends_with("..."));
        let chars: Vec<char> = result.chars().collect();
        assert_eq!(chars.len(), 15); // 12 + 3 for "..."
    }

    #[test]
    fn test_truncate_display_newlines() {
        let text = "第一行\n第二行";
        let result = truncate_display(text);
        assert!(!result.contains('\n'));
    }

    #[test]
    fn test_validate_segments_valid() {
        let (segs, json) = validate_segments(r#"["hello","world"]"#).unwrap();
        assert_eq!(segs, vec!["hello", "world"]);
        assert_eq!(json, r#"["hello","world"]"#);
    }

    #[test]
    fn test_validate_segments_empty() {
        assert!(validate_segments("").is_err());
        assert!(validate_segments("  ").is_err());
    }

    #[test]
    fn test_validate_segments_too_long() {
        let long = "x".repeat(5001);
        assert!(validate_segments(&long).is_err());
    }

    #[test]
    fn test_validate_segments_exactly_at_limit() {
        let exact = "x".repeat(5000);
        assert!(validate_segments(&exact).is_ok());
    }

    #[test]
    fn test_validate_segments_plain_string_fallback() {
        let (segs, _) = validate_segments("plain text").unwrap();
        assert_eq!(segs, vec!["plain text"]);
    }

    #[test]
    fn test_validate_segments_trims_whitespace() {
        let (segs, _) = validate_segments(r#"[" hello ", " world "]"#).unwrap();
        assert_eq!(segs, vec!["hello", "world"]);
    }

    #[test]
    fn test_validate_segments_filters_empty() {
        let (segs, _) = validate_segments(r#"["hello", "", " ", "world"]"#).unwrap();
        assert_eq!(segs, vec!["hello", "world"]);
    }
}
