//! 输入校验与文本处理模块
//!
//! 提供客户端 IP 提取、文本内容分段解析、显示截断和分段校验等工具函数。
//! 所有校验规则均在此模块集中定义，供路由层和服务层调用。

use actix_web::HttpRequest;

use super::MAX_CONTENT_LENGTH;

/// 从 HTTP 请求中提取客户端 IP 地址。
///
/// 优先使用 `X-Forwarded-For` 等代理头（由 actix 的 `ConnectionInfo` 处理），
/// 截断到最大 45 个字符（IPv6 最大长度），避免非法超长输入。
pub fn get_client_ip(req: &HttpRequest) -> String {
    let info = req.connection_info();
    let raw = info.realip_remote_addr().unwrap_or("unknown");
    let end = raw.len().min(45);
    raw[..end].to_string()
}

/// 将文本内容解析为分段列表。
///
/// 尝试按 JSON 数组格式解析；如果失败或结果为空数组，
/// 则将整个文本作为单个分段返回。
pub fn parse_segments(text_content: &str) -> Vec<String> {
    match serde_json::from_str::<Vec<String>>(text_content) {
        Ok(segments) if !segments.is_empty() => segments,
        _ => vec![text_content.to_string()],
    }
}

/// 截断文本用于列表页显示：取第一个分段的前 12 个字符，超出部分用 `"..."` 表示。
///
/// 换行符会被替换为空格，确保在单行内正常显示。
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

/// 校验并解析表单提交的文本内容为分段列表。
///
/// 处理流程：
/// 1. 尝试按 JSON 数组解析，对每个分段去除首尾空白并过滤空字符串
/// 2. 如果不是 JSON 数组，将整段文本作为单个分段
/// 3. 校验分段不为空且总长度不超过 `MAX_CONTENT_LENGTH`
///
/// 返回 `(segments, json_string)` 元组，或错误信息。
pub fn validate_segments(raw: &str) -> Result<(Vec<String>, String), &'static str> {
    let text_content = raw.trim();
    // 尝试 JSON 数组解析，失败则回退到纯文本单分段
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

    // 校验：内容不能为空
    if segments.is_empty() {
        return Err("文字内容不能为空");
    }

    // 校验：总长度不能超过限制
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
