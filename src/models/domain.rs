use chrono::NaiveDateTime;
use serde::Serialize;

pub(crate) mod datetime_format {
    use chrono::NaiveDateTime;
    use serde::{self, Serializer};

    const FORMAT: &str = "%Y-%m-%d %H:%M:%S";

    pub fn serialize<S>(date: &NaiveDateTime, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&date.format(FORMAT).to_string())
    }
}

pub(crate) mod option_datetime_format {
    use chrono::NaiveDateTime;
    use serde::{self, Serializer};

    const FORMAT: &str = "%Y-%m-%d %H:%M:%S";

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

#[derive(sqlx::FromRow, Serialize)]
pub struct QrCodeRecord {
    pub id: u64,
    pub uuid: String,
    pub text_content: String,
    pub remark: Option<String>,
    pub max_count: u32,
    pub used_count: u32,
    pub last_extract_ip: Option<String>,
    #[serde(serialize_with = "datetime_format::serialize")]
    pub created_at: NaiveDateTime,
    #[sqlx(default)]
    #[serde(serialize_with = "option_datetime_format::serialize")]
    pub last_extract_at: Option<NaiveDateTime>,
}

#[derive(sqlx::FromRow, Serialize)]
pub struct AuditLog {
    pub id: u64,
    pub username: String,
    pub action: String,
    pub target_uuid: Option<String>,
    pub detail: Option<String>,
    pub client_ip: String,
    #[serde(serialize_with = "datetime_format::serialize")]
    pub created_at: NaiveDateTime,
}

#[derive(sqlx::FromRow, Serialize)]
pub struct AdminUser {
    pub id: u32,
    pub username: String,
    #[serde(skip_serializing)]
    pub password_hash: String,
    pub is_active: bool,
    #[serde(serialize_with = "option_datetime_format::serialize")]
    pub locked_until: Option<NaiveDateTime>,
    pub failed_attempts: u32,
    #[serde(serialize_with = "datetime_format::serialize")]
    pub created_at: NaiveDateTime,
    #[serde(serialize_with = "datetime_format::serialize")]
    pub updated_at: NaiveDateTime,
}

#[derive(sqlx::FromRow, Serialize)]
pub struct ExtractLog {
    pub id: u64,
    pub qrcode_id: u64,
    pub client_ip: String,
    pub browser_id: String,
    pub segment_index: Option<u32>,
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
