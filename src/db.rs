//! 数据库连接池初始化模块
//!
//! 基于 SQLx 创建 MySQL 连接池，支持配置最大连接数和会话时区。
//! 每个新连接建立后会自动执行 `SET time_zone` 以统一时区。

use sqlx::mysql::MySqlPoolOptions;
use sqlx::MySqlPool;

/// 初始化 MySQL 连接池。
///
/// 创建连接池并为每个新建连接设置指定时区。连接失败时返回错误，
/// 调用方应在启动阶段处理该错误并终止程序。
///
/// # 参数
/// - `database_url` - MySQL 连接字符串（包含用户名、密码、主机和数据库名）
/// - `max_connections` - 连接池最大连接数
/// - `timezone` - MySQL 会话时区（如 `+08:00`）
pub async fn init_pool(
    database_url: &str,
    max_connections: u32,
    timezone: &str,
) -> Result<MySqlPool, sqlx::Error> {
    // 脱敏日志：隐藏连接字符串中 @ 之前的用户名和密码
    let masked_url = if let Some(at_pos) = database_url.find('@') {
        format!("***@{}", &database_url[at_pos + 1..])
    } else {
        "***".to_string()
    };
    log::debug!("Initializing database pool: {masked_url}, max={max_connections}, tz={timezone}");
    let tz = timezone.to_string();
    MySqlPoolOptions::new()
        .max_connections(max_connections)
        // 每个新连接建立后自动设置会话时区
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
