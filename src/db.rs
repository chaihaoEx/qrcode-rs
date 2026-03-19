use sqlx::mysql::MySqlPoolOptions;
use sqlx::MySqlPool;

pub async fn init_pool(database_url: &str) -> Result<MySqlPool, sqlx::Error> {
    let masked_url = if let Some(at_pos) = database_url.find('@') {
        format!("***@{}", &database_url[at_pos + 1..])
    } else {
        "***".to_string()
    };
    log::debug!("Initializing database pool: {masked_url}, max=5");
    MySqlPoolOptions::new()
        .max_connections(5)
        .connect(database_url)
        .await
}
