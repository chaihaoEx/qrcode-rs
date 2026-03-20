pub mod crypto;
pub mod masking;
pub mod pagination;
pub mod render;
pub mod validation;

pub const PAGE_SIZE: i64 = 20;
pub const MAX_CONTENT_LENGTH: usize = 5000;
pub const MAX_COUNT_UPPER: u32 = 10000;
