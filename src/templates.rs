//! 模板引擎初始化模块
//!
//! 使用 Tera 模板引擎加载 `templates/` 目录下的所有 HTML 模板文件。

use tera::Tera;

/// 初始化 Tera 模板引擎，加载 `templates/**/*.html` 下的所有模板。
///
/// 如果模板文件加载或解析失败，程序将 panic 退出。
pub fn init_templates() -> Tera {
    Tera::new("templates/**/*.html").expect("Failed to initialize Tera templates")
}
