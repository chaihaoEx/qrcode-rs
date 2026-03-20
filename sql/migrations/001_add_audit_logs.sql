-- 管理员操作审计日志表
-- 版本: v1.7.0
-- 日期: 2026-03-20

CREATE TABLE IF NOT EXISTS admin_audit_logs (
    id BIGINT UNSIGNED AUTO_INCREMENT PRIMARY KEY,
    username VARCHAR(100) NOT NULL,
    action VARCHAR(50) NOT NULL COMMENT '操作类型: login_success/login_failed/logout/create/edit/delete/reset/ai_create',
    target_uuid VARCHAR(36) DEFAULT NULL COMMENT '操作对象UUID（登录/退出时为空）',
    detail VARCHAR(500) DEFAULT NULL COMMENT '操作详情',
    client_ip VARCHAR(45) NOT NULL,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    INDEX idx_created_at (created_at),
    INDEX idx_username (username),
    INDEX idx_action (action)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;
