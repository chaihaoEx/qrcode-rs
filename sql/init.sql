-- QRCode-RS 数据库初始化脚本
-- 使用方法: mysql -u root -p < sql/init.sql

CREATE DATABASE IF NOT EXISTS qrcode_db DEFAULT CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;

USE qrcode_db;

-- 二维码主表
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

-- 提取记录表
CREATE TABLE IF NOT EXISTS qr_extract_logs (
    id BIGINT UNSIGNED AUTO_INCREMENT PRIMARY KEY,
    qrcode_id BIGINT UNSIGNED NOT NULL,
    client_ip VARCHAR(45) NOT NULL,
    segment_index INT UNSIGNED NULL,
    extracted_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    INDEX idx_qrcode_id (qrcode_id),
    FOREIGN KEY (qrcode_id) REFERENCES qr_codes(id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

-- 按IP独立计数表
CREATE TABLE IF NOT EXISTS qr_ip_extracts (
    id BIGINT UNSIGNED AUTO_INCREMENT PRIMARY KEY,
    qrcode_id BIGINT UNSIGNED NOT NULL,
    client_ip VARCHAR(45) NOT NULL,
    used_count INT UNSIGNED NOT NULL DEFAULT 0,
    UNIQUE KEY uk_qrcode_ip (qrcode_id, client_ip),
    FOREIGN KEY (qrcode_id) REFERENCES qr_codes(id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
