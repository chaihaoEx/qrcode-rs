//! 工具模块
//!
//! 提供加密、分页、模板渲染、输入校验等通用工具函数，
//! 以及全局共享的业务常量定义。

pub mod crypto;
pub mod pagination;
pub mod render;
pub mod validation;

/// 列表页每页显示的记录数
pub const PAGE_SIZE: i64 = 20;
/// 二维码文本内容的最大字符长度
pub const MAX_CONTENT_LENGTH: usize = 5000;
/// 二维码最大提取次数的上限值
pub const MAX_COUNT_UPPER: u32 = 10000;
