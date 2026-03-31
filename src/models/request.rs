//! HTTP 请求与响应数据传输对象
//!
//! 定义所有 HTTP 接口使用的表单、查询参数和 JSON 请求/响应结构体。
//! 表单类结构体包含 `csrf_token` 字段用于 CSRF 防护验证。

use serde::{Deserialize, Serialize};

/// 浏览器槽位领取请求（JSON POST body）
///
/// 由客户端在提取页面通过 `POST /claim` 发送，
/// `browser_id` 为客户端生成的 UUID v4，存储在 localStorage 中。
#[derive(Deserialize)]
pub struct ClaimRequest {
    /// 浏览器唯一标识（UUID v4）
    pub browser_id: String,
}

/// 浏览器槽位领取响应（JSON 返回）
///
/// 返回领取结果状态和分配的文本内容。
#[derive(Serialize)]
pub struct ClaimResponse {
    /// 状态码：`"ok"` 成功、`"exhausted"` 已用完、`"error"` 出错
    pub status: String,
    /// 分配的文本分段内容（成功时返回）
    pub text_content: Option<String>,
    /// 分配的分段索引（成功时返回，从 0 开始）
    pub segment_index: Option<u32>,
}

/// 二维码列表页查询参数
#[derive(Deserialize)]
pub struct ListQuery {
    /// 当前页码（从 1 开始，默认为 1）
    pub page: Option<i64>,
    /// 搜索关键词（按备注模糊匹配）
    pub keyword: Option<String>,
}

/// 提取日志页查询参数
///
/// 同时保留列表页的分页和搜索状态，以便返回时恢复。
#[derive(Deserialize)]
pub struct LogsQuery {
    /// 日志页当前页码
    pub page: Option<i64>,
    /// 列表页的页码（用于返回时恢复）
    pub list_page: Option<i64>,
    /// 列表页的搜索关键词（用于返回时恢复）
    pub list_keyword: Option<String>,
}

/// 审计日志页查询参数
#[derive(Deserialize)]
pub struct AuditLogsQuery {
    /// 当前页码
    pub page: Option<i64>,
    /// 按操作类型筛选（如 `"create"`, `"delete"` 等）
    pub action: Option<String>,
    /// 按用户名或目标 UUID 搜索
    pub keyword: Option<String>,
}

/// 创建二维码表单（POST 提交）
#[derive(Deserialize)]
pub struct CreateForm {
    /// 文本内容（多个分段以换行分隔）
    pub text_content: String,
    /// 备注信息
    pub remark: Option<String>,
    /// 最大提取次数（不填则默认等于分段数量）
    pub max_count: Option<u32>,
    /// CSRF 防护令牌
    pub csrf_token: String,
}

/// 通用操作表单（仅含 CSRF 令牌，用于删除/重置等操作）
#[derive(Deserialize)]
pub struct ActionForm {
    /// CSRF 防护令牌
    pub csrf_token: String,
}

/// AI 生成评论请求（JSON POST body）
#[derive(Deserialize)]
pub struct AiGenerateRequest {
    /// 生成主题
    pub topic: String,
    /// 生成数量
    pub count: Option<u32>,
    /// 风格描述
    pub style: Option<String>,
    /// 示例内容
    pub examples: Option<String>,
    /// CSRF 防护令牌
    pub csrf_token: String,
}

/// AI 生成内容创建二维码表单
#[derive(Deserialize)]
pub struct AiCreateForm {
    /// AI 生成的评论内容（JSON 数组字符串）
    pub comments: String,
    /// 备注信息
    pub remark: Option<String>,
    /// 最大提取次数
    pub max_count: Option<u32>,
    /// CSRF 防护令牌
    pub csrf_token: String,
}

/// 创建管理员用户表单
#[derive(Deserialize)]
pub struct CreateUserForm {
    /// 用户名（需符合格式校验规则）
    pub username: String,
    /// 密码（需符合长度要求）
    pub password: String,
    /// CSRF 防护令牌
    pub csrf_token: String,
}

/// 切换用户启用/禁用状态表单
#[derive(Deserialize)]
pub struct ToggleUserForm {
    /// 用户 ID
    pub id: u32,
    /// 目标状态：`true` 启用、`false` 禁用
    pub is_active: bool,
    /// CSRF 防护令牌
    pub csrf_token: String,
}

/// 修改密码表单
#[derive(Deserialize)]
pub struct ChangePasswordForm {
    /// 旧密码（用于验证身份）
    pub old_password: String,
    /// 新密码
    pub new_password: String,
    /// CSRF 防护令牌
    pub csrf_token: String,
}
