-- QRCode-RS 数据库初始化脚本
-- 使用方法: mysql -u root -p < sql/init.sql

CREATE DATABASE IF NOT EXISTS qrcode_db DEFAULT CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;

USE qrcode_db;

-- 二维码主表
-- used_count 语义：已分配槽位数（每个新 browser_id 分配时 +1）
-- max_count  语义：最大可分配槽位数（即段落总数）
CREATE TABLE IF NOT EXISTS qr_codes (
    id BIGINT UNSIGNED AUTO_INCREMENT PRIMARY KEY,
    uuid VARCHAR(36) NOT NULL,
    text_content TEXT NOT NULL,
    remark VARCHAR(255) NULL,
    max_count INT UNSIGNED NOT NULL DEFAULT 5,
    used_count INT UNSIGNED NOT NULL DEFAULT 0,
    last_extract_ip VARCHAR(45) NULL,
    last_extract_at DATETIME NULL,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE KEY uk_uuid (uuid)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

-- 提取记录表（追加 browser_id 和 segment_index）
CREATE TABLE IF NOT EXISTS qr_extract_logs (
    id BIGINT UNSIGNED AUTO_INCREMENT PRIMARY KEY,
    qrcode_id BIGINT UNSIGNED NOT NULL,
    client_ip VARCHAR(45) NOT NULL,
    browser_id VARCHAR(36) NOT NULL DEFAULT '',
    segment_index INT UNSIGNED NULL,
    extracted_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    INDEX idx_qrcode_id (qrcode_id),
    FOREIGN KEY (qrcode_id) REFERENCES qr_codes(id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

-- 浏览器槽位表（每个 browser_id 独占一段，顺序分配）
CREATE TABLE IF NOT EXISTS qr_browser_slots (
    id BIGINT UNSIGNED AUTO_INCREMENT PRIMARY KEY,
    qrcode_id BIGINT UNSIGNED NOT NULL,
    browser_id VARCHAR(36) NOT NULL,
    segment_index INT UNSIGNED NOT NULL,
    client_ip VARCHAR(45) NOT NULL,
    assigned_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE KEY uk_qrcode_browser (qrcode_id, browser_id),
    INDEX idx_qrcode_id (qrcode_id),
    FOREIGN KEY (qrcode_id) REFERENCES qr_codes(id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
