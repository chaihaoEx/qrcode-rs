use sqlx::MySqlPool;

use crate::models::{ExtractLog, QrCodeRecord};
use crate::utils::PAGE_SIZE;

/// List QR codes with optional keyword search, returns (total, records).
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
        // Note: %keyword% LIKE on TEXT column cannot use index.
        // At current scale this is acceptable; consider full-text search if data grows large.
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

/// Fetch a single QR code by UUID.
pub async fn get_by_uuid(
    pool: &MySqlPool,
    uuid: &str,
) -> Result<Option<QrCodeRecord>, sqlx::Error> {
    sqlx::query_as::<_, QrCodeRecord>("SELECT * FROM qr_codes WHERE uuid = ?")
        .bind(uuid)
        .fetch_optional(pool)
        .await
}

/// Create a new QR code, returns the generated UUID.
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

/// Update an existing QR code.
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

/// Delete a QR code by UUID.
pub async fn delete(pool: &MySqlPool, uuid: &str) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM qr_codes WHERE uuid = ?")
        .bind(uuid)
        .execute(pool)
        .await?;

    Ok(())
}

/// Reset slots for a QR code (delete browser slots and reset used_count).
pub async fn reset_slots(pool: &MySqlPool, uuid: &str) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;

    sqlx::query(
        "DELETE FROM qr_browser_slots WHERE qrcode_id = (SELECT id FROM qr_codes WHERE uuid = ?)",
    )
    .bind(uuid)
    .execute(&mut *tx)
    .await?;

    sqlx::query("UPDATE qr_codes SET used_count = 0 WHERE uuid = ?")
        .bind(uuid)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;
    Ok(())
}

/// List extract logs for a QR code, returns (total, logs).
pub async fn list_extract_logs(
    pool: &MySqlPool,
    qrcode_id: u64,
    offset: i64,
) -> Result<(i64, Vec<ExtractLog>), sqlx::Error> {
    let total: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM qr_extract_logs WHERE qrcode_id = ?")
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

/// Check if a character is an emoji or emoji-related symbol.
fn is_emoji(c: char) -> bool {
    let cp = c as u32;
    matches!(cp,
        0x200D |                    // Zero-width joiner
        0xFE0F |                    // Variation selector-16
        0x20E3 |                    // Combining enclosing keycap
        0x2600..=0x27BF |           // Misc symbols, dingbats
        0x2B50..=0x2B55 |           // Stars, circles
        0xFE00..=0xFE0F |          // Variation selectors
        0x1F000..=0x1FAFF |         // Mahjong, dominos, emoji block
        0xE0020..=0xE007F |         // Tags
        0x200B..=0x200F |           // Zero-width spaces
        0x2028..=0x202F |           // Line/paragraph separators
        0x2060..=0x206F |           // Invisible formatters
        0xE0001..=0xE007F           // Language tags
    )
}

/// Try to load a CJK-capable font from common system paths.
fn load_system_font() -> Result<Vec<u8>, String> {
    let candidates = [
        // macOS
        "/System/Library/Fonts/PingFang.ttc",
        "/System/Library/Fonts/STHeiti Medium.ttc",
        "/System/Library/Fonts/Hiragino Sans GB.ttc",
        // Linux common paths
        "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/google-noto-cjk/NotoSansCJKsc-Regular.otf",
        "/usr/share/fonts/truetype/droid/DroidSansFallbackFull.ttf",
        // Fallback: project-local font if present
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
    Err("No CJK font found on system. Install Noto Sans CJK or place a .ttf in static/fonts/".to_string())
}

/// Generate a styled QR code PNG image from a URL.
/// If `remark` is provided, the remark text is drawn below the QR code.
pub fn generate_qr_image(url: &str, remark: Option<&str>) -> Result<Vec<u8>, String> {
    use ab_glyph::{Font as AbFont, FontRef, PxScale, ScaleFont};
    use image::{ImageEncoder, Rgba, RgbaImage};
    use imageproc::drawing::{draw_filled_rect_mut, draw_text_mut};
    use imageproc::rect::Rect;

    // Gradient colors: top-left → bottom-right
    let color_start: [u8; 3] = [59, 130, 246]; // blue-500 #3b82f6
    let color_end: [u8; 3] = [168, 85, 247]; // purple-500 #a855f7
    let bg_color = Rgba([255u8, 255, 255, 255]);
    let text_color = Rgba([71u8, 85, 105, 255]); // --gray-600: #475569

    let padding = 24u32; // white padding around QR
    let module_size = 8u32; // pixels per QR module
    let module_gap = 1u32; // gap between modules for rounded look

    let qr = qrcode::QrCode::new(url.as_bytes())
        .map_err(|e| format!("Failed to generate QR code: {e}"))?;

    let modules = qr.width() as u32; // number of modules per side
    let qr_area = modules * module_size;
    let content_w = qr_area + padding * 2;

    let remark = remark
        .map(|s| s.chars().filter(|c| !is_emoji(*c)).collect::<String>())
        .filter(|s| !s.trim().is_empty());
    let text_area_height = if remark.is_some() { 40u32 } else { 0 };
    let canvas_h = qr_area + padding * 2 + text_area_height;

    let mut canvas = RgbaImage::from_pixel(content_w, canvas_h, bg_color);

    // Draw QR modules with gradient color (diagonal: top-left → bottom-right)
    let colors = qr.to_colors();
    for row in 0..modules {
        for col in 0..modules {
            let idx = (row * modules + col) as usize;
            if colors[idx] == qrcode::Color::Dark {
                // Interpolation factor based on diagonal position
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

    // Draw remark text if present
    if let Some(remark_text) = remark {
        let font_data = load_system_font()?;
        let font = FontRef::try_from_slice(&font_data)
            .map_err(|e| format!("Failed to parse font: {e}"))?;

        let scale = PxScale::from(18.0);
        let scaled_font = font.as_scaled(scale);

        // Truncate if too long
        let max_width = (content_w - padding * 2) as f32;
        let mut display_remark = String::new();
        let mut current_width: f32 = 0.0;
        let mut truncated = false;
        let ellipsis_width: f32 = "...".chars()
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
            // recalculate without ellipsis reserve
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
