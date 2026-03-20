use actix_web::HttpResponse;
use tera::{Context, Tera};

pub const PAGE_SIZE: i64 = 20;
pub const MAX_CONTENT_LENGTH: usize = 5000;
pub const MAX_COUNT_UPPER: u32 = 10000;

/// DB error → HTML error page. Use in admin handlers.
/// Usage: `let val = db_try!(query.await, &tmpl, base);`
#[macro_export]
macro_rules! db_try {
    ($expr:expr, $tmpl:expr, $base:expr) => {
        match $expr {
            Ok(v) => v,
            Err(e) => {
                log::warn!("DB query failed: {e}");
                return $crate::helpers::render_error(
                    $tmpl,
                    $base,
                    "数据库查询失败",
                    actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
                );
            }
        }
    };
}

/// DB fetch_optional → HTML error page, with 404 on None.
/// Usage: `let record = db_try_optional!(query.await, &tmpl, base, "二维码不存在");`
#[macro_export]
macro_rules! db_try_optional {
    ($expr:expr, $tmpl:expr, $base:expr, $not_found_msg:expr) => {
        match $expr {
            Ok(Some(v)) => v,
            Ok(None) => {
                return $crate::helpers::render_error(
                    $tmpl,
                    $base,
                    $not_found_msg,
                    actix_web::http::StatusCode::NOT_FOUND,
                );
            }
            Err(e) => {
                log::warn!("DB query failed: {e}");
                return $crate::helpers::render_error(
                    $tmpl,
                    $base,
                    "数据库查询失败",
                    actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
                );
            }
        }
    };
}

/// Calculate page number and offset from optional page parameter.
pub fn calc_page_offset(page: Option<i64>) -> (i64, i64) {
    let page = page.unwrap_or(1).clamp(1, 100_000);
    let offset = (page - 1) * PAGE_SIZE;
    (page, offset)
}

/// Calculate total pages from total record count.
pub fn calc_total_pages(total: i64) -> i64 {
    (total + PAGE_SIZE - 1) / PAGE_SIZE
}

/// Generate HMAC-SHA256 hash (first 8 bytes = 16 hex chars)
pub fn generate_extract_hash(uuid: &str, salt: &str) -> String {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    use std::fmt::Write;

    let mut mac = Hmac::<Sha256>::new_from_slice(salt.as_bytes()).unwrap();
    mac.update(uuid.as_bytes());
    let result = mac.finalize().into_bytes();
    let mut hex = String::with_capacity(16);
    for b in &result[..8] {
        let _ = write!(hex, "{:02x}", b);
    }
    hex
}

/// Verify extract hash with constant-time comparison and optional legacy (8-char) support
pub fn verify_extract_hash(uuid: &str, hash: &str, salt: &str, legacy_support: bool) -> bool {
    use subtle::ConstantTimeEq;

    let expected = generate_extract_hash(uuid, salt);

    // New 16-char hash
    if hash.len() == 16 {
        return expected.as_bytes().ct_eq(hash.as_bytes()).into();
    }

    // Legacy 8-char hash fallback
    if legacy_support && hash.len() == 8 {
        return expected[..8].as_bytes().ct_eq(hash.as_bytes()).into();
    }

    false
}

pub fn render_template(tmpl: &Tera, template: &str, ctx: &Context) -> HttpResponse {
    match tmpl.render(template, ctx) {
        Ok(rendered) => HttpResponse::Ok().content_type("text/html").body(rendered),
        Err(e) => {
            log::warn!("Template render failed: template={template}, error={e}");
            HttpResponse::InternalServerError()
                .content_type("text/plain")
                .body("Internal Server Error")
        }
    }
}

pub fn render_template_with_status(
    tmpl: &Tera,
    template: &str,
    ctx: &Context,
    status: actix_web::http::StatusCode,
) -> HttpResponse {
    match tmpl.render(template, ctx) {
        Ok(rendered) => HttpResponse::build(status)
            .content_type("text/html")
            .body(rendered),
        Err(e) => {
            log::warn!("Template render failed: template={template}, error={e}");
            HttpResponse::InternalServerError()
                .content_type("text/plain")
                .body("Internal Server Error")
        }
    }
}

pub fn render_error(
    tmpl: &Tera,
    base: &str,
    message: &str,
    status: actix_web::http::StatusCode,
) -> HttpResponse {
    let mut ctx = Context::new();
    ctx.insert("base", base);
    ctx.insert("message", message);
    render_template_with_status(tmpl, "error.html", &ctx, status)
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
    fn test_generate_extract_hash_deterministic() {
        let h1 = generate_extract_hash("test-uuid", "salt");
        let h2 = generate_extract_hash("test-uuid", "salt");
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 16);
    }

    #[test]
    fn test_generate_extract_hash_different_inputs() {
        let h1 = generate_extract_hash("uuid-1", "salt");
        let h2 = generate_extract_hash("uuid-2", "salt");
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_verify_extract_hash_correct() {
        let hash = generate_extract_hash("test-uuid", "salt");
        assert!(verify_extract_hash("test-uuid", &hash, "salt", false));
    }

    #[test]
    fn test_verify_extract_hash_wrong() {
        assert!(!verify_extract_hash(
            "test-uuid",
            "0000000000000000",
            "salt",
            false
        ));
    }

    #[test]
    fn test_verify_extract_hash_legacy() {
        let full_hash = generate_extract_hash("test-uuid", "salt");
        let legacy = &full_hash[..8];
        assert!(verify_extract_hash("test-uuid", legacy, "salt", true));
        assert!(!verify_extract_hash("test-uuid", legacy, "salt", false));
    }

    #[test]
    fn test_verify_extract_hash_wrong_length() {
        assert!(!verify_extract_hash("test-uuid", "abc", "salt", true));
        assert!(!verify_extract_hash("test-uuid", "", "salt", true));
        assert!(!verify_extract_hash(
            "test-uuid",
            "0000000000000000000000",
            "salt",
            true
        ));
    }

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

    #[test]
    fn test_calc_page_offset_defaults() {
        let (page, offset) = calc_page_offset(None);
        assert_eq!(page, 1);
        assert_eq!(offset, 0);
    }

    #[test]
    fn test_calc_page_offset_page_2() {
        let (page, offset) = calc_page_offset(Some(2));
        assert_eq!(page, 2);
        assert_eq!(offset, 20);
    }

    #[test]
    fn test_calc_page_offset_clamps() {
        let (page, _) = calc_page_offset(Some(0));
        assert_eq!(page, 1);
        let (page, _) = calc_page_offset(Some(-5));
        assert_eq!(page, 1);
    }

    #[test]
    fn test_calc_total_pages() {
        assert_eq!(calc_total_pages(0), 0);
        assert_eq!(calc_total_pages(1), 1);
        assert_eq!(calc_total_pages(20), 1);
        assert_eq!(calc_total_pages(21), 2);
        assert_eq!(calc_total_pages(40), 2);
        assert_eq!(calc_total_pages(41), 3);
    }
}
