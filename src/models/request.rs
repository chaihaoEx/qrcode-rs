use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct ClaimRequest {
    pub browser_id: String,
}

#[derive(Serialize)]
pub struct ClaimResponse {
    pub status: String,
    pub text_content: Option<String>,
    pub segment_index: Option<u32>,
}

#[derive(Deserialize)]
pub struct ListQuery {
    pub page: Option<i64>,
    pub keyword: Option<String>,
}

#[derive(Deserialize)]
pub struct LogsQuery {
    pub page: Option<i64>,
    pub list_page: Option<i64>,
    pub list_keyword: Option<String>,
}

#[derive(Deserialize)]
pub struct AuditLogsQuery {
    pub page: Option<i64>,
    pub action: Option<String>,
    pub keyword: Option<String>,
}

#[derive(Deserialize)]
pub struct CreateForm {
    pub text_content: String,
    pub remark: Option<String>,
    pub max_count: Option<u32>,
    pub csrf_token: String,
}

#[derive(Deserialize)]
pub struct ActionForm {
    pub csrf_token: String,
}

#[derive(Deserialize)]
pub struct AiGenerateRequest {
    pub topic: String,
    pub count: Option<u32>,
    pub style: Option<String>,
    pub examples: Option<String>,
    pub csrf_token: String,
}

#[derive(Deserialize)]
pub struct AiCreateForm {
    pub comments: String,
    pub remark: Option<String>,
    pub max_count: Option<u32>,
    pub csrf_token: String,
}

#[derive(Deserialize)]
pub struct CreateUserForm {
    pub username: String,
    pub password: String,
    pub csrf_token: String,
}

#[derive(Deserialize)]
pub struct ToggleUserForm {
    pub id: u32,
    pub is_active: bool,
    pub csrf_token: String,
}

#[derive(Deserialize)]
pub struct ChangePasswordForm {
    pub old_password: String,
    pub new_password: String,
    pub csrf_token: String,
}
