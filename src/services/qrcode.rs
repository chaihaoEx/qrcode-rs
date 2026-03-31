//! 二维码 CRUD 与图片生成服务
//!
//! 提供二维码记录的增删改查、提取日志查询和二维码图片生成功能。
//! 图片生成支持蓝紫渐变色彩、模块间隙和备注文字绘制。

use sqlx::MySqlPool;

use crate::models::{ExtractLog, QrCodeRecord};
use crate::utils::PAGE_SIZE;

/// 查询二维码列表，支持按关键词搜索，返回 `(总数, 记录列表)`。
///
/// 关键词同时匹配 `text_content` 和 `remark` 字段（LIKE 模糊搜索）。
/// 结果按创建时间倒序排列，支持分页。
pub async fn list_qrcodes(
    pool: &MySqlPool,
    keyword: &str,
    offset: i64,
) -> Result<(i64, Vec<QrCodeRecord>), sqlx::Error> {
    if keyword.is_empty() {
        let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM qr_codes")
            .fetch_one(pool)
            .await?;

        let records = sqlx::query_as::<_, QrCodeRecord>(
            "SELECT * FROM qr_codes ORDER BY created_at DESC LIMIT ? OFFSET ?",
        )
        .bind(PAGE_SIZE)
        .bind(offset)
        .fetch_all(pool)
        .await?;

        Ok((total, records))
    } else {
        // LIKE 模糊搜索：在当前数据规模下可接受，数据量大时考虑全文索引
        let like_pattern = format!("%{keyword}%");

        let total: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM qr_codes WHERE text_content LIKE ? OR remark LIKE ?",
        )
        .bind(&like_pattern)
        .bind(&like_pattern)
        .fetch_one(pool)
        .await?;

        let records = sqlx::query_as::<_, QrCodeRecord>(
            "SELECT * FROM qr_codes WHERE text_content LIKE ? OR remark LIKE ? ORDER BY created_at DESC LIMIT ? OFFSET ?",
        )
        .bind(&like_pattern)
        .bind(&like_pattern)
        .bind(PAGE_SIZE)
        .bind(offset)
        .fetch_all(pool)
        .await?;

        Ok((total, records))
    }
}

/// 根据 UUID 查询单条二维码记录。
pub async fn get_by_uuid(
    pool: &MySqlPool,
    uuid: &str,
) -> Result<Option<QrCodeRecord>, sqlx::Error> {
    sqlx::query_as::<_, QrCodeRecord>("SELECT * FROM qr_codes WHERE uuid = ?")
        .bind(uuid)
        .fetch_optional(pool)
        .await
}

/// 创建新的二维码记录，返回生成的 UUID。
///
/// # 参数
/// - `text_content_json` - JSON 数组格式的文本分段（如 `["段落1", "段落2"]`）
/// - `remark` - 备注信息（可选）
/// - `max_count` - 最大提取次数
pub async fn create(
    pool: &MySqlPool,
    text_content_json: &str,
    remark: Option<&str>,
    max_count: u32,
) -> Result<String, sqlx::Error> {
    let uuid = uuid::Uuid::new_v4().to_string();

    sqlx::query(
        "INSERT INTO qr_codes (uuid, text_content, remark, max_count, used_count, created_at) VALUES (?, ?, ?, ?, 0, NOW())",
    )
    .bind(&uuid)
    .bind(text_content_json)
    .bind(remark)
    .bind(max_count)
    .execute(pool)
    .await?;

    Ok(uuid)
}

/// 更新已有的二维码记录。
pub async fn update(
    pool: &MySqlPool,
    uuid: &str,
    text_content_json: &str,
    remark: Option<&str>,
    max_count: u32,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE qr_codes SET text_content = ?, remark = ?, max_count = ? WHERE uuid = ?")
        .bind(text_content_json)
        .bind(remark)
        .bind(max_count)
        .bind(uuid)
        .execute(pool)
        .await?;

    Ok(())
}

/// 根据 UUID 删除二维码记录。
///
/// 关联的 `qr_browser_slots` 和 `qr_extract_logs` 通过外键级联删除。
pub async fn delete(pool: &MySqlPool, uuid: &str) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM qr_codes WHERE uuid = ?")
        .bind(uuid)
        .execute(pool)
        .await?;

    Ok(())
}

/// 重置二维码的提取状态：删除所有浏览器槽位并将 `used_count` 重置为 0。
///
/// 在事务中执行，确保槽位删除和计数重置的原子性。
pub async fn reset_slots(pool: &MySqlPool, uuid: &str) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;

    // 删除关联的浏览器槽位记录
    sqlx::query(
        "DELETE FROM qr_browser_slots WHERE qrcode_id = (SELECT id FROM qr_codes WHERE uuid = ?)",
    )
    .bind(uuid)
    .execute(&mut *tx)
    .await?;

    // 重置已使用计数
    sqlx::query("UPDATE qr_codes SET used_count = 0 WHERE uuid = ?")
        .bind(uuid)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;
    Ok(())
}

/// 查询指定二维码的提取日志列表，返回 `(总数, 日志列表)`。
///
/// 结果按提取时间倒序排列，支持分页。
pub async fn list_extract_logs(
    pool: &MySqlPool,
    qrcode_id: u64,
    offset: i64,
) -> Result<(i64, Vec<ExtractLog>), sqlx::Error> {
    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM qr_extract_logs WHERE qrcode_id = ?")
        .bind(qrcode_id)
        .fetch_one(pool)
        .await?;

    let logs = sqlx::query_as::<_, ExtractLog>(
        "SELECT * FROM qr_extract_logs WHERE qrcode_id = ? ORDER BY extracted_at DESC LIMIT ? OFFSET ?",
    )
    .bind(qrcode_id)
    .bind(PAGE_SIZE)
    .bind(offset)
    .fetch_all(pool)
    .await?;

    Ok((total, logs))
}

/// 判断字符是否为 emoji 或 emoji 相关的特殊符号。
///
/// 用于在绘制二维码备注文字前过滤掉 emoji 字符，
/// 因为系统 CJK 字体通常不包含 emoji 字形，会导致渲染异常。
fn is_emoji(c: char) -> bool {
    let cp = c as u32;
    matches!(cp,
        0x200D |                    // 零宽连接符
        0xFE0F |                    // 变体选择符-16
        0x20E3 |                    // 组合包围键帽
        0x2600..=0x27BF |           // 杂项符号、装饰符号
        0x2B50..=0x2B55 |           // 星星、圆圈
        0xFE00..=0xFE0F |          // 变体选择符
        0x1F000..=0x1FAFF |         // 麻将、骰子、emoji 区块
        0xE0020..=0xE007F |         // 标签字符
        0x200B..=0x200F |           // 零宽空格
        0x2028..=0x202F |           // 行/段落分隔符
        0x2060..=0x206F |           // 不可见格式化字符
        0xE0001..=0xE007F           // 语言标签
    )
}

/// 尝试从系统常见路径加载支持 CJK 的字体文件。
///
/// 按优先级依次尝试 macOS 和 Linux 的常见字体路径，
/// 最后回退到项目本地的字体文件。不嵌入字体文件以保持二进制体积。
fn load_system_font() -> Result<Vec<u8>, String> {
    let candidates = [
        // macOS 系统字体
        "/System/Library/Fonts/PingFang.ttc",
        "/System/Library/Fonts/STHeiti Medium.ttc",
        "/System/Library/Fonts/Hiragino Sans GB.ttc",
        // Linux 常见字体路径
        "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/google-noto-cjk/NotoSansCJKsc-Regular.otf",
        "/usr/share/fonts/truetype/droid/DroidSansFallbackFull.ttf",
        // 回退：项目本地字体
        "static/fonts/LXGWWenKaiScreen-Regular.ttf",
    ];
    for path in &candidates {
        if let Ok(data) = std::fs::read(path) {
            if data.len() > 1000 {
                log::debug!("Loaded system font: {path}");
                return Ok(data);
            }
        }
    }
    Err(
        "No CJK font found on system. Install Noto Sans CJK or place a .ttf in static/fonts/"
            .to_string(),
    )
}

/// 生成带样式的二维码 PNG 图片。
///
/// 图片特性：
/// - 蓝色→紫色对角线渐变色彩
/// - 模块间留有间隙，呈现圆润风格
/// - 白色内边距
/// - 可选在底部绘制备注文字（自动过滤 emoji、超长截断、水平居中）
///
/// # 参数
/// - `url` - 编码到二维码中的 URL 内容
/// - `remark` - 可选的备注文字，绘制在二维码下方
pub fn generate_qr_image(url: &str, remark: Option<&str>) -> Result<Vec<u8>, String> {
    use ab_glyph::{Font as AbFont, FontRef, PxScale, ScaleFont};
    use image::{ImageEncoder, Rgba, RgbaImage};
    use imageproc::drawing::{draw_filled_rect_mut, draw_text_mut};
    use imageproc::rect::Rect;

    // ---- 颜色和尺寸常量 ----
    let color_start: [u8; 3] = [59, 130, 246]; // 渐变起始色 blue-500 #3b82f6
    let color_end: [u8; 3] = [168, 85, 247]; // 渐变结束色 purple-500 #a855f7
    let bg_color = Rgba([255u8, 255, 255, 255]); // 白色背景
    let text_color = Rgba([71u8, 85, 105, 255]); // 备注文字色 gray-600

    let padding = 24u32; // 二维码周围的白色内边距
    let module_size = 8u32; // 每个 QR 模块的像素大小
    let module_gap = 1u32; // 模块间隙（营造圆润效果）

    // ---- 生成 QR 矩阵 ----
    let qr = qrcode::QrCode::new(url.as_bytes())
        .map_err(|e| format!("Failed to generate QR code: {e}"))?;

    let modules = qr.width() as u32; // 每边的模块数量
    let qr_area = modules * module_size;
    let content_w = qr_area + padding * 2;

    // ---- 处理备注文字 ----
    // 过滤 emoji 字符（系统 CJK 字体不含 emoji 字形）
    let remark = remark
        .map(|s| s.chars().filter(|c| !is_emoji(*c)).collect::<String>())
        .filter(|s| !s.trim().is_empty());
    let text_area_height = if remark.is_some() { 40u32 } else { 0 };
    let canvas_h = qr_area + padding * 2 + text_area_height;

    let mut canvas = RgbaImage::from_pixel(content_w, canvas_h, bg_color);

    // ---- 绘制 QR 模块（对角线渐变） ----
    let colors = qr.to_colors();
    for row in 0..modules {
        for col in 0..modules {
            let idx = (row * modules + col) as usize;
            if colors[idx] == qrcode::Color::Dark {
                // 根据对角线位置计算渐变插值因子
                let t = ((row as f32 + col as f32) / (modules as f32 * 2.0 - 2.0)).min(1.0);
                let r = (color_start[0] as f32 * (1.0 - t) + color_end[0] as f32 * t) as u8;
                let g = (color_start[1] as f32 * (1.0 - t) + color_end[1] as f32 * t) as u8;
                let b = (color_start[2] as f32 * (1.0 - t) + color_end[2] as f32 * t) as u8;

                let x = padding + col * module_size + module_gap;
                let y = padding + row * module_size + module_gap;
                let size = module_size - module_gap * 2;
                draw_filled_rect_mut(
                    &mut canvas,
                    Rect::at(x as i32, y as i32).of_size(size, size),
                    Rgba([r, g, b, 255]),
                );
            }
        }
    }

    // ---- 绘制备注文字 ----
    if let Some(remark_text) = remark {
        let font_data = load_system_font()?;
        let font = FontRef::try_from_slice(&font_data)
            .map_err(|e| format!("Failed to parse font: {e}"))?;

        let scale = PxScale::from(18.0);
        let scaled_font = font.as_scaled(scale);

        // 超长文字截断：保留能完整显示的字符 + "..."
        let max_width = (content_w - padding * 2) as f32;
        let mut display_remark = String::new();
        let mut current_width: f32 = 0.0;
        let mut truncated = false;
        let ellipsis_width: f32 = "..."
            .chars()
            .map(|c| scaled_font.h_advance(scaled_font.glyph_id(c)))
            .sum();

        for c in remark_text.chars() {
            let advance = scaled_font.h_advance(scaled_font.glyph_id(c));
            if current_width + advance + ellipsis_width > max_width {
                display_remark.push_str("...");
                truncated = true;
                break;
            }
            current_width += advance;
            display_remark.push(c);
        }
        if !truncated {
            current_width = display_remark
                .chars()
                .map(|c| scaled_font.h_advance(scaled_font.glyph_id(c)))
                .sum();
        } else {
            current_width = display_remark
                .chars()
                .map(|c| scaled_font.h_advance(scaled_font.glyph_id(c)))
                .sum();
        }

        // 文字水平居中
        let text_x = (((content_w as f32) - current_width) / 2.0).max(padding as f32) as i32;
        let text_y = (padding + qr_area + 10) as i32;

        draw_text_mut(
            &mut canvas,
            text_color,
            text_x,
            text_y,
            scale,
            &font,
            &display_remark,
        );
    }

    // ---- 编码为 PNG ----
    let mut buf = std::io::Cursor::new(Vec::new());
    let encoder = image::codecs::png::PngEncoder::new(&mut buf);
    encoder
        .write_image(
            canvas.as_raw(),
            content_w,
            canvas_h,
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|e| format!("Failed to encode PNG: {e}"))?;

    Ok(buf.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_emoji_zwj() {
        assert!(is_emoji('\u{200D}'));
    }

    #[test]
    fn test_is_emoji_variation_selector() {
        assert!(is_emoji('\u{FE0F}'));
    }

    #[test]
    fn test_is_emoji_keycap() {
        assert!(is_emoji('\u{20E3}'));
    }

    #[test]
    fn test_is_emoji_sun() {
        assert!(is_emoji('☀')); // U+2600
    }

    #[test]
    fn test_is_emoji_heart() {
        assert!(is_emoji('❤')); // U+2764
    }

    #[test]
    fn test_is_emoji_star() {
        assert!(is_emoji('⭐')); // U+2B50
    }

    #[test]
    fn test_is_emoji_face() {
        assert!(is_emoji('😀')); // U+1F600
    }

    #[test]
    fn test_is_emoji_ascii() {
        assert!(!is_emoji('A'));
    }

    #[test]
    fn test_is_emoji_digit() {
        assert!(!is_emoji('5'));
    }

    #[test]
    fn test_is_emoji_cjk() {
        assert!(!is_emoji('中'));
    }
}
