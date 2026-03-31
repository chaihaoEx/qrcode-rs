//! 业务服务层
//!
//! 封装所有核心业务逻辑，包括二维码 CRUD、内容提取、审计日志、
//! AI 评论生成和用户管理。路由层通过调用服务层函数完成业务操作。

pub mod ai;
pub mod audit;
pub mod extract;
pub mod qrcode;
pub mod user;
