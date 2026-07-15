//! 拼图拼接：把多张图合成为一张。
//!
//! 覆盖 FR-02、Requirement 7、55.3。

use crate::error::{CoreError, Result};
use image::{Rgba, RgbaImage};
use std::num::NonZeroU32;

/// 拼接布局（Requirement 7.2–7.4）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollageLayout {
    /// 纵向排列，形成长图。
    Vertical,
    /// 横向排列，形成宽图。
    Horizontal,
    /// 网格排列。
    Grid {
        /// 列数（Requirement 7.5）。行数由图片数与列数推出。
        columns: NonZeroU32,
    },
}

/// 拼接选项（Requirement 7.6、7.7）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CollageOptions {
    /// 图片之间及外边缘的间距，单位像素。
    pub spacing: u32,
    /// 背景色。间距区域与单元格内的留白由它填充。
    pub background: Rgba<u8>,
}

impl Default for CollageOptions {
    fn default() -> Self {
        Self {
            spacing: 0,
            background: Rgba([255, 255, 255, 255]),
        }
    }
}

/// 拼接多张图片（Requirement 7.8）。
///
/// 三种布局都按**统一单元格**排布：单元格尺寸取所有输入图的最大宽与最大高，
/// 每张图在自己的单元格内居中。这样 N 张图必定产生 N 个互不重叠的可见区域
/// （Requirement 55.3），且不同尺寸的图混排时不会互相挤压。
///
/// 间距同时用于图片之间与画布外边缘，因此单张图也会得到对称的边框。
///
/// 输入为空时返回错误——空拼图没有合理的尺寸。
pub fn compose(
    images: &[RgbaImage],
    layout: CollageLayout,
    options: CollageOptions,
) -> Result<RgbaImage> {
    if images.is_empty() {
        return Err(CoreError::InvalidArgument(
            "拼图至少需要一张图片".to_string(),
        ));
    }

    let cell_w = images.iter().map(RgbaImage::width).max().unwrap_or(1).max(1);
    let cell_h = images
        .iter()
        .map(RgbaImage::height)
        .max()
        .unwrap_or(1)
        .max(1);

    let count = images.len() as u32;
    let (cols, rows) = match layout {
        CollageLayout::Vertical => (1, count),
        CollageLayout::Horizontal => (count, 1),
        CollageLayout::Grid { columns } => {
            let c = columns.get();
            (c, count.div_ceil(c))
        }
    };

    let spacing = options.spacing;
    // 画布 = 单元格 + 单元格间距 + 四周外边距
    let canvas_w = cell_w
        .checked_mul(cols)
        .and_then(|w| w.checked_add(spacing.checked_mul(cols + 1)?))
        .ok_or_else(|| CoreError::InvalidArgument("拼图画布宽度溢出".to_string()))?;
    let canvas_h = cell_h
        .checked_mul(rows)
        .and_then(|h| h.checked_add(spacing.checked_mul(rows + 1)?))
        .ok_or_else(|| CoreError::InvalidArgument("拼图画布高度溢出".to_string()))?;

    let mut canvas = RgbaImage::from_pixel(canvas_w, canvas_h, options.background);

    for (i, img) in images.iter().enumerate() {
        let i = i as u32;
        let (col, row) = (i % cols, i / cols);
        let cell_x = spacing + col * (cell_w + spacing);
        let cell_y = spacing + row * (cell_h + spacing);
        // 在单元格内居中
        let x = cell_x + (cell_w - img.width()) / 2;
        let y = cell_y + (cell_h - img.height()) / 2;
        overlay_exact(&mut canvas, img, x, y);
    }

    Ok(canvas)
}

/// 把 `top` 按 alpha 混合到 `base` 的 (x, y) 处。越界部分裁掉。
///
/// 不用 `image::imageops::overlay` 是因为它的签名要求 i64 坐标且行为随版本变动；
/// 这里的逻辑简单且需要精确控制混合方式。
pub(crate) fn overlay_exact(base: &mut RgbaImage, top: &RgbaImage, x: u32, y: u32) {
    for (tx, ty, px) in top.enumerate_pixels() {
        let (bx, by) = (x + tx, y + ty);
        if bx >= base.width() || by >= base.height() {
            continue;
        }
        let dst = base.get_pixel_mut(bx, by);
        *dst = blend(*dst, *px);
    }
}

/// 标准 source-over alpha 混合。
pub(crate) fn blend(dst: Rgba<u8>, src: Rgba<u8>) -> Rgba<u8> {
    let sa = f32::from(src.0[3]) / 255.0;
    if sa >= 1.0 {
        return src;
    }
    if sa <= 0.0 {
        return dst;
    }
    let da = f32::from(dst.0[3]) / 255.0;
    let out_a = sa + da * (1.0 - sa);
    if out_a <= 0.0 {
        return Rgba([0, 0, 0, 0]);
    }
    let ch = |s: u8, d: u8| -> u8 {
        let s = f32::from(s) / 255.0;
        let d = f32::from(d) / 255.0;
        let v = (s * sa + d * da * (1.0 - sa)) / out_a;
        (v * 255.0).round().clamp(0.0, 255.0) as u8
    };
    Rgba([
        ch(src.0[0], dst.0[0]),
        ch(src.0[1], dst.0[1]),
        ch(src.0[2], dst.0[2]),
        (out_a * 255.0).round().clamp(0.0, 255.0) as u8,
    ])
}
