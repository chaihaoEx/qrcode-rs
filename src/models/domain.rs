//! 数据库领域模型
//!
//! 定义与 MySQL 数据库表一一映射的核心数据结构，包括二维码记录、
//! 审计日志、管理员用户和提取日志。所有结构体均实现 `sqlx::FromRow`
//! 用于数据库查询映射，以及 `serde::Serialize` 用于 JSON / 模板序列化。

use chrono::NaiveDateTime;
use serde::Serialize;

/// 日期时间序列化模块（必填字段）
///
/// 将 `NaiveDateTime` 序列化为 `"YYYY-MM-DD HH:MM:SS"` 格式字符串，
/// 用于 JSON 输出和 Tera 模板渲染。
pub(crate) mod datetime_format {
    use chrono::NaiveDateTime;
    use serde::{self, Serializer};

    const FORMAT: &str = "%Y-%m-%d %H:%M:%S";

    /// 将 `NaiveDateTime` 按 `%Y-%m-%d %H:%M:%S` 格式序列化为字符串。
    pub fn serialize<S>(date: &NaiveDateTime, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&date.format(FORMAT).to_string())
    }
}

/// 可选日期时间序列化模块（可空字段）
///
/// 将 `Option<NaiveDateTime>` 序列化为格式化字符串或 `null`，
/// 用于最后提取时间等可能为空的时间戳字段。
pub(crate) mod option_datetime_format {
    use chrono::NaiveDateTime;
    use serde::{self, Serializer};

    const FORMAT: &str = "%Y-%m-%d %H:%M:%S";

    /// 将 `Option<NaiveDateTime>` 序列化：`Some` 时输出格式化字符串，`None` 时输出 `null`。
    pub fn serialize<S>(date: &Option<NaiveDateTime>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match date {
            Some(d) => serializer.serialize_str(&d.format(FORMAT).to_string()),
            None => serializer.serialize_none(),
        }
    }
}

/// 二维码记录，对应 `qr_codes` 数据库表。
///
/// 每条记录代表一个可被多次提取的二维码，包含文本内容（JSON 数组格式的分段列表）、
/// 使用计数和最大提取次数等信息。通过 `uuid` 字段作为外部唯一标识。
#[derive(sqlx::FromRow, Serialize)]
pub struct QrCodeRecord {
    /// 数据库自增主键
    pub id: u64,
    /// 二维码唯一标识（UUID v4）
    pub uuid: String,
    /// 文本内容，存储为 JSON 数组格式（如 `["段落1", "段落2"]`）
    pub text_content: String,
    /// 备注信息，用于管理员标识和搜索（已建索引）
    pub remark: Option<String>,
    /// 最大可提取次数
    pub max_count: u32,
    /// 已使用的提取次数
    pub used_count: u32,
    /// 最后一次提取操作的客户端 IP
    pub last_extract_ip: Option<String>,
    /// 记录创建时间
    #[serde(serialize_with = "datetime_format::serialize")]
    pub created_at: NaiveDateTime,
    /// 最后一次提取操作的时间
    #[sqlx(default)]
    #[serde(serialize_with = "option_datetime_format::serialize")]
    pub last_extract_at: Option<NaiveDateTime>,
}

/// 审计日志记录，对应 `admin_audit_logs` 数据库表。
///
/// 记录管理员的所有操作行为（创建、编辑、删除、重置等），
/// 用于安全审计和操作追踪。
#[derive(sqlx::FromRow, Serialize)]
pub struct AuditLog {
    /// 数据库自增主键
    pub id: u64,
    /// 执行操作的管理员用户名
    pub username: String,
    /// 操作类型（如 `"create"`, `"delete"`, `"reset"`, `"edit"` 等）
    pub action: String,
    /// 操作目标的二维码 UUID（部分操作可能无目标）
    pub target_uuid: Option<String>,
    /// 操作详情描述
    pub detail: Option<String>,
    /// 执行操作时的客户端 IP
    pub client_ip: String,
    /// 操作时间
    #[serde(serialize_with = "datetime_format::serialize")]
    pub created_at: NaiveDateTime,
}

/// 管理员用户，对应 `admin_users` 数据库表。
///
/// 存储通过数据库管理的普通管理员账户信息。超级管理员通过配置文件定义，
/// 不在此表中。支持账户锁定机制：连续 5 次登录失败后锁定 30 分钟。
#[derive(sqlx::FromRow, Serialize)]
pub struct AdminUser {
    /// 数据库自增主键
    pub id: u32,
    /// 用户名（唯一）
    pub username: String,
    /// bcrypt 密码哈希值（序列化时跳过，不暴露给前端）
    #[serde(skip_serializing)]
    pub password_hash: String,
    /// 账户是否启用
    pub is_active: bool,
    /// 账户锁定截止时间（`None` 表示未锁定）
    #[serde(serialize_with = "option_datetime_format::serialize")]
    pub locked_until: Option<NaiveDateTime>,
    /// 连续登录失败次数（成功登录后重置为 0）
    pub failed_attempts: u32,
    /// 账户创建时间
    #[serde(serialize_with = "datetime_format::serialize")]
    pub created_at: NaiveDateTime,
    /// 账户最后更新时间
    #[serde(serialize_with = "datetime_format::serialize")]
    pub updated_at: NaiveDateTime,
}

/// 提取日志记录，对应 `qr_extract_logs` 数据库表。
///
/// 记录每次二维码内容提取操作的详细信息，包括客户端 IP、
/// 浏览器标识和分配的分段索引。
#[derive(sqlx::FromRow, Serialize)]
pub struct ExtractLog {
    /// 数据库自增主键
    pub id: u64,
    /// 关联的二维码记录 ID（外键，级联删除）
    pub qrcode_id: u64,
    /// 客户端 IP 地址
    pub client_ip: String,
    /// 浏览器唯一标识（UUID v4，由客户端生成并存储在 localStorage）
    pub browser_id: String,
    /// 分配的文本分段索引（从 0 开始）
    pub segment_index: Option<u32>,
    /// 提取操作时间
    #[serde(serialize_with = "datetime_format::serialize")]
    pub extracted_at: NaiveDateTime,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn test_datetime_format() {
        #[derive(Serialize)]
        struct T {
            #[serde(serialize_with = "datetime_format::serialize")]
            dt: NaiveDateTime,
        }
        let t = T {
            dt: NaiveDate::from_ymd_opt(2024, 3, 30)
                .unwrap()
                .and_hms_opt(15, 45, 30)
                .unwrap(),
        };
        let json = serde_json::to_value(&t).unwrap();
        assert_eq!(json["dt"], "2024-03-30 15:45:30");
    }

    #[test]
    fn test_option_datetime_some() {
        #[derive(Serialize)]
        struct T {
            #[serde(serialize_with = "option_datetime_format::serialize")]
            dt: Option<NaiveDateTime>,
        }
        let t = T {
            dt: Some(
                NaiveDate::from_ymd_opt(2024, 1, 1)
                    .unwrap()
                    .and_hms_opt(0, 0, 0)
                    .unwrap(),
            ),
        };
        let json = serde_json::to_value(&t).unwrap();
        assert_eq!(json["dt"], "2024-01-01 00:00:00");
    }

    #[test]
    fn test_option_datetime_none() {
        #[derive(Serialize)]
        struct T {
            #[serde(serialize_with = "option_datetime_format::serialize")]
            dt: Option<NaiveDateTime>,
        }
        let t = T { dt: None };
        let json = serde_json::to_value(&t).unwrap();
        assert!(json["dt"].is_null());
    }

    #[test]
    fn test_admin_user_skips_password() {
        let now = NaiveDate::from_ymd_opt(2024, 1, 1)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();
        let user = AdminUser {
            id: 1,
            username: "admin".to_string(),
            password_hash: "secret_hash".to_string(),
            is_active: true,
            locked_until: None,
            failed_attempts: 0,
            created_at: now,
            updated_at: now,
        };
        let json = serde_json::to_value(&user).unwrap();
        assert!(json.get("password_hash").is_none());
        assert_eq!(json["username"], "admin");
    }
}
