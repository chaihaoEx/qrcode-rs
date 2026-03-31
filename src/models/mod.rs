//! 数据模型模块
//!
//! 定义与数据库表映射的领域模型（`domain`）以及 HTTP 请求/响应
//! 的数据传输对象（`request`）。所有类型统一从本模块重导出。

mod domain;
mod request;

pub use domain::*;
pub use request::*;
