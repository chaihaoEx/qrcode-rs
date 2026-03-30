use crate::config::AiConfig;
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<Message>,
    temperature: f32,
}

#[derive(Serialize)]
struct Message {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: ResponseMessage,
}

#[derive(Deserialize)]
struct ResponseMessage {
    content: String,
}

const SYSTEM_PROMPT: &str = r#"你是一个评论生成助手。根据用户提供的主题信息，生成指定数量的独立评论。
要求：
1. 每条评论内容不同，表达方式各异，长度也要有变化
2. 像真实用户的评论，自然、口语化，不要套话
3. 严格按 JSON 数组格式返回，如 ["评论1", "评论2"]
4. 不要添加任何额外文字、解释或 markdown 格式"#;

pub async fn generate_comments(
    config: &AiConfig,
    topic: &str,
    count: u32,
    style: &str,
    examples: &str,
) -> Result<Vec<String>, String> {
    let mut user_content = format!("主题/原始信息：\n{topic}\n\n请生成 {count} 条不同的评论。");

    if !style.is_empty() {
        user_content.push_str(&format!("\n\n风格要求：{style}"));
    }

    if !examples.is_empty() {
        user_content.push_str(&format!("\n\n参考示例：\n{examples}"));
    }

    let request = ChatRequest {
        model: config.model.clone(),
        messages: vec![
            Message {
                role: "system".to_string(),
                content: SYSTEM_PROMPT.to_string(),
            },
            Message {
                role: "user".to_string(),
                content: user_content,
            },
        ],
        temperature: 0.9,
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| format!("HTTP 客户端创建失败: {e}"))?;

    let url = format!("{}/chat/completions", config.base_url.trim_end_matches('/'));

    let request_body = serde_json::to_string(&request).unwrap_or_default();
    log::info!(
        "AI API request: url={url}, model={}, body_len={}",
        config.model,
        request_body.len()
    );
    log::debug!("AI API request body: {request_body}");

    let response = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", config.api_key))
        .header("Content-Type", "application/json")
        .json(&request)
        .send()
        .await
        .map_err(|e| format!("AI API 请求失败: {e}"))?;

    let status = response.status();
    log::info!("AI API response: status={status}");

    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        log::warn!("AI API error: status={status}, body={body}");
        return Err(format!("AI API 返回错误: {status}, {body}"));
    }

    let response_text = response
        .text()
        .await
        .map_err(|e| format!("AI 响应读取失败: {e}"))?;
    log::debug!("AI API response body: {response_text}");

    let chat_response: ChatResponse = serde_json::from_str(&response_text)
        .map_err(|e| format!("AI 响应解析失败: {e}, body={response_text}"))?;

    let content = chat_response
        .choices
        .first()
        .map(|c| c.message.content.clone())
        .ok_or_else(|| format!("AI 未返回任何内容, body={response_text}"))?;

    // Try to extract JSON array from response (may contain markdown code blocks)
    let json_str = extract_json_array(&content).unwrap_or(&content);

    let comments: Vec<String> = serde_json::from_str(json_str)
        .map_err(|e| format!("AI 返回内容格式错误，无法解析为评论列表: {e}"))?;

    if comments.is_empty() {
        return Err("AI 生成了空的评论列表".to_string());
    }

    log::info!("AI generated {} comments", comments.len());
    Ok(comments)
}

/// Extract JSON array from content that may be wrapped in markdown code blocks
fn extract_json_array(content: &str) -> Option<&str> {
    let trimmed = content.trim();

    // Already a JSON array
    if trimmed.starts_with('[') && trimmed.ends_with(']') {
        return Some(trimmed);
    }

    // Wrapped in ```json ... ``` or ``` ... ```
    if let Some(start) = trimmed.find('[') {
        if let Some(end) = trimmed.rfind(']') {
            if start < end {
                return Some(&trimmed[start..=end]);
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_json_array_plain() {
        let input = r#"["hello", "world"]"#;
        assert_eq!(extract_json_array(input), Some(r#"["hello", "world"]"#));
    }

    #[test]
    fn test_extract_json_array_with_markdown() {
        let input = "```json\n[\"hello\", \"world\"]\n```";
        assert_eq!(extract_json_array(input), Some("[\"hello\", \"world\"]"));
    }

    #[test]
    fn test_extract_json_array_with_extra_text() {
        let input = "Here are the comments:\n[\"a\", \"b\"]\nDone!";
        assert_eq!(extract_json_array(input), Some("[\"a\", \"b\"]"));
    }

    #[test]
    fn test_extract_json_array_no_array() {
        assert_eq!(extract_json_array("no json here"), None);
    }
}
