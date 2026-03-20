use sqlx::mysql::MySqlPoolOptions;
use sqlx::MySqlPool;

pub async fn init_pool(database_url: &str, max_connections: u32) -> Result<MySqlPool, sqlx::Error> {
    let masked_url = if let Some(at_pos) = database_url.find('@') {
        format!("***@{}", &database_url[at_pos + 1..])
    } else {
        "***".to_string()
    };
    log::debug!("Initializing database pool: {masked_url}, max={max_connections}");
    MySqlPoolOptions::new()
        .max_connections(max_connections)
        .after_connect(|conn, _meta| {
            Box::pin(async move {
                sqlx::query("SET time_zone = '+08:00'")
                    .execute(&mut *conn)
                    .await?;
                Ok(())
            })
        })
        .connect(database_url)
        .await
}
