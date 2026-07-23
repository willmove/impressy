//! 用系统字体把文字水印栅格化成透明 RGBA 图。
//!
//! `cosmic-text` 提供跨平台字体发现、Unicode fallback 与 Swash 栅格化；输出随后交给
//! `impressy-core::watermark` 做位置、平铺与透明度合成。字体只属于 UI 层，core 仍是
//! 无 UI、无系统字体依赖的纯函数库。

use cosmic_text::{Attrs, Buffer, Color, FontSystem, Metrics, Shaping, SwashCache, Wrap};
use image::{Rgba, RgbaImage};

const FONT_SIZE: f32 = 40.0;
const LINE_HEIGHT: f32 = 56.0;
const MARGIN: i32 = 12;
const MAX_CHARS: usize = 256;

pub(crate) fn rasterize(text: &str) -> Result<RgbaImage, String> {
    rasterize_with_size(text, FONT_SIZE)
}

/// Rasterizes a poster text layer at an explicit output-pixel size.
pub(crate) fn rasterize_with_size(text: &str, font_size: f32) -> Result<RgbaImage, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("text watermark is empty".to_string());
    }
    if text.chars().count() > MAX_CHARS {
        return Err(format!("text watermark exceeds {MAX_CHARS} characters"));
    }

    let font_size = font_size.clamp(8.0, 512.0);
    let line_height = font_size * (LINE_HEIGHT / FONT_SIZE);
    let margin = ((font_size / FONT_SIZE) * MARGIN as f32).round().max(2.0) as i32;
    let estimated_width = ((text.chars().count() as f32 * font_size) as u32).clamp(64, 16_384);
    let canvas_width = estimated_width.saturating_add((margin * 2) as u32);
    let canvas_height = (line_height as u32).saturating_add((margin * 2) as u32);

    let mut font_system = FontSystem::new();
    let mut cache = SwashCache::new();
    let mut buffer = Buffer::new(&mut font_system, Metrics::new(font_size, line_height));
    {
        let mut borrowed = buffer.borrow_with(&mut font_system);
        borrowed.set_size(Some(estimated_width as f32), Some(canvas_height as f32));
        borrowed.set_wrap(Wrap::None);
        borrowed.set_text(text, &Attrs::new(), Shaping::Advanced);
        borrowed.shape_until_scroll(true);
    }

    let mut canvas = RgbaImage::new(canvas_width, canvas_height);
    let mut bounds: Option<(u32, u32, u32, u32)> = None;
    buffer.draw(
        &mut font_system,
        &mut cache,
        Color::rgb(255, 255, 255),
        |x, y, width, height, color| {
            let x = x + margin;
            let y = y + margin;
            for py in 0..height {
                for px in 0..width {
                    let (dx, dy) = (x + px as i32, y + py as i32);
                    if dx < 0 || dy < 0 {
                        continue;
                    }
                    let (dx, dy) = (dx as u32, dy as u32);
                    if dx >= canvas_width || dy >= canvas_height || color.a() == 0 {
                        continue;
                    }
                    canvas.put_pixel(dx, dy, Rgba(color.as_rgba()));
                    bounds = Some(match bounds {
                        Some((min_x, min_y, max_x, max_y)) => {
                            (min_x.min(dx), min_y.min(dy), max_x.max(dx), max_y.max(dy))
                        }
                        None => (dx, dy, dx, dy),
                    });
                }
            }
        },
    );

    let Some((min_x, min_y, max_x, max_y)) = bounds else {
        return Err("no system font could render the watermark text".to_string());
    };
    Ok(
        image::imageops::crop_imm(&canvas, min_x, min_y, max_x - min_x + 1, max_y - min_y + 1)
            .to_image(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_text_without_loading_fonts() {
        assert!(rasterize("  \n ").is_err());
    }

    #[test]
    fn rejects_unbounded_watermark_text() {
        let text = "x".repeat(MAX_CHARS + 1);
        assert!(rasterize(&text).is_err());
    }
}
