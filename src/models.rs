use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

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
pub struct ExtractLog {
    pub id: u64,
    pub qrcode_id: u64,
    pub client_ip: String,
    pub browser_id: String,
    pub segment_index: Option<u32>,
    #[serde(serialize_with = "datetime_format::serialize")]
    pub extracted_at: NaiveDateTime,
}

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
