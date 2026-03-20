use sqlx::mysql::MySqlPoolOptions;
use sqlx::MySqlPool;

pub async fn init_pool(
    database_url: &str,
    max_connections: u32,
    timezone: &str,
) -> Result<MySqlPool, sqlx::Error> {
    let masked_url = if let Some(at_pos) = database_url.find('@') {
        format!("***@{}", &database_url[at_pos + 1..])
    } else {
        "***".to_string()
    };
    log::debug!("Initializing database pool: {masked_url}, max={max_connections}, tz={timezone}");
    let tz = timezone.to_string();
    MySqlPoolOptions::new()
        .max_connections(max_connections)
        .after_connect(move |conn, _meta| {
            let tz = tz.clone();
            Box::pin(async move {
                sqlx::query(&format!("SET time_zone = '{}'", tz))
                    .execute(&mut *conn)
                    .await?;
                Ok(())
            })
        })
        .connect(database_url)
        .await
}
