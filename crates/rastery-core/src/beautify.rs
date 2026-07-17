//! 截图美化：圆角、内边距、背景、描边、阴影。
//!
//! 覆盖 FR-07、Requirement 12、Property 18、Property 28。
//!
//! 输出画布尺寸 = 原图 + 2×内边距；阴影绘制在画布内（超出部分裁剪）。无阴影时
//! 尺寸增量恰为 2×padding，满足 Property 28「增量 ≤ 2(R+P)」。
//! 透明背景下圆角以外区域为真透明（alpha=0）（Requirement 12.10、Property 18）。

use crate::error::Result;
use image::{GrayImage, Luma, Rgba, RgbaImage};

/// 美化参数（Requirement 12.2–12.8）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BeautifyParams {
    /// 圆角半径（Requirement 12.2）。0 表示直角；超出半边长时自动钳制。
    pub corner_radius: u32,
    /// 内边距（Requirement 12.3）。
    pub inner_padding: u32,
    /// 背景（Requirement 12.4–12.6）。
    pub background: Background,
    /// 描边（Requirement 12.7）。
    pub border: Option<Border>,
    /// 阴影（Requirement 12.8）。
    pub shadow: Option<Shadow>,
}

/// 背景样式（Requirement 12.4–12.6）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Background {
    /// 透明（Requirement 12.6）。圆角外区域为真透明。
    Transparent,
    /// 纯色（Requirement 12.5）。
    Solid(Rgba<u8>),
    /// 渐变（Requirement 12.4）。
    Gradient {
        /// 起始色。
        start: Rgba<u8>,
        /// 结束色。
        end: Rgba<u8>,
        /// 方向。
        direction: GradientDirection,
    },
}

/// 渐变方向。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GradientDirection {
    /// 水平（左→右）。
    Horizontal,
    /// 垂直（上→下）。
    Vertical,
    /// 对角（左上→右下）。
    Diagonal,
}

/// 描边（Requirement 12.7）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Border {
    /// 描边宽度（像素）。
    pub width: u32,
    /// 描边颜色。
    pub color: Rgba<u8>,
}

/// 阴影（Requirement 12.8）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shadow {
    /// 偏移 (dx, dy)，可负。
    pub offset: (i32, i32),
    /// 模糊半径 σ。
    pub blur_sigma: f32,
    /// 阴影颜色（通常半透明黑）。
    pub color: Rgba<u8>,
}

/// 美化截图（Requirement 12）。
///
/// 参数校验：圆角半径钳制到 `min(半宽, 半高)`，避免半径超过图幅时整张图被切掉。
pub fn beautify(img: &RgbaImage, params: &BeautifyParams) -> Result<RgbaImage> {
    let (iw, ih) = img.dimensions();
    let pad = params.inner_padding;
    let (cw, ch) = (iw + 2 * pad, ih + 2 * pad);

    // 圆角半径钳制到半边长。
    let radius = params
        .corner_radius
        .min(iw / 2)
        .min(ih / 2)
        .min(cw / 2)
        .min(ch / 2);

    // 1. 背景
    let mut canvas = match params.background {
        Background::Transparent => RgbaImage::new(cw, ch),
        Background::Solid(c) => RgbaImage::from_pixel(cw, ch, c),
        Background::Gradient {
            start,
            end,
            direction,
        } => gradient(cw, ch, start, end, direction),
    };

    // 2. 内容带圆角
    let mut content = img.clone();
    if radius > 0 {
        round_corners(&mut content, radius);
    }

    // 3. 阴影（绘制在内容下方）
    if let Some(sh) = params.shadow {
        draw_shadow(&mut canvas, &content, pad, pad, radius, sh);
    }

    // 4. 合成内容
    crate::collage::overlay_exact(&mut canvas, &content, pad, pad);

    // 5. 描边
    if let Some(b) = params.border
        && b.width > 0
    {
        draw_border(&mut canvas, pad, pad, iw, ih, radius, b);
    }

    Ok(canvas)
}

/// 判断像素 (px,py) 是否在圆角矩形 (rx,ry,rw,rh) 内（含边界）。radius=0 即直角矩形。
///
/// 用 i64 运算避免 u32 下溢。
fn inside_rounded(px: i64, py: i64, rx: i64, ry: i64, rw: i64, rh: i64, radius: i64) -> bool {
    if px < rx || py < ry || px >= rx + rw || py >= ry + rh {
        return false;
    }
    if radius <= 0 {
        return true;
    }
    let lx = px - rx;
    let ly = py - ry;
    let left = lx < radius;
    let right = lx >= rw - radius;
    let top = ly < radius;
    let bottom = ly >= rh - radius;
    if (left || right) && (top || bottom) {
        let cx = if left { rx + radius } else { rx + rw - radius };
        let cy = if top { ry + radius } else { ry + rh - radius };
        let dx = px - cx;
        let dy = py - cy;
        dx * dx + dy * dy <= radius * radius
    } else {
        true
    }
}

/// 把图像四角切成透明圆角。
fn round_corners(img: &mut RgbaImage, radius: u32) {
    let (w, h) = img.dimensions();
    let r = i64::from(radius);
    for y in 0..h {
        for x in 0..w {
            if !inside_rounded(
                i64::from(x),
                i64::from(y),
                0,
                0,
                i64::from(w),
                i64::from(h),
                r,
            ) {
                img.get_pixel_mut(x, y).0[3] = 0;
            }
        }
    }
}

/// 在内容下方绘制模糊阴影。
///
/// 取内容 alpha 作为剪影掩码，高斯模糊后按 offset 平移，以阴影色合成到画布。
fn draw_shadow(
    canvas: &mut RgbaImage,
    content: &RgbaImage,
    ox: u32,
    oy: u32,
    radius: u32,
    sh: Shadow,
) {
    let (cw, ch) = canvas.dimensions();
    let (w, h) = content.dimensions();

    // 剪影掩码（内容 alpha）。
    let mut mask = GrayImage::new(w, h);
    for y in 0..h {
        for x in 0..w {
            mask.put_pixel(x, y, Luma([content.get_pixel(x, y).0[3]]));
        }
    }
    // 圆角处掩码已是剪影（round_corners 已置 alpha=0），无需再裁。

    if sh.blur_sigma > 0.0 {
        mask = imageproc::filter::gaussian_blur_f32(&mask, sh.blur_sigma);
    }

    let sa_base = f32::from(sh.color.0[3]) / 255.0;
    for y in 0..h {
        for x in 0..w {
            let cov = f32::from(mask.get_pixel(x, y).0[0]) / 255.0;
            if cov == 0.0 {
                continue;
            }
            let dx = i64::from(ox) + i64::from(x) + i64::from(sh.offset.0);
            let dy = i64::from(oy) + i64::from(y) + i64::from(sh.offset.1);
            if dx < 0 || dy < 0 || dx >= i64::from(cw) || dy >= i64::from(ch) {
                continue;
            }
            let (dxu, dyu) = (dx as u32, dy as u32);
            let base = *canvas.get_pixel(dxu, dyu);
            let sa = sa_base * cov;
            let inv = 1.0 - sa;
            let out = Rgba([
                blend_ch(sh.color.0[0], base.0[0], sa, inv),
                blend_ch(sh.color.0[1], base.0[1], sa, inv),
                blend_ch(sh.color.0[2], base.0[2], sa, inv),
                ((sa + f32::from(base.0[3]) / 255.0 * inv) * 255.0)
                    .round()
                    .clamp(0.0, 255.0) as u8,
            ]);
            canvas.put_pixel(dxu, dyu, out);
        }
    }
    // 抑制未使用的 radius（圆角已并入内容剪影）。
    let _ = radius;
}

/// 沿圆角矩形边缘绘制描边环。
fn draw_border(canvas: &mut RgbaImage, ox: u32, oy: u32, iw: u32, ih: u32, radius: u32, b: Border) {
    let r = i64::from(radius);
    let bw = i64::from(b.width);
    for y in 0..ih {
        for x in 0..iw {
            let (gx, gy) = (i64::from(ox) + i64::from(x), i64::from(oy) + i64::from(y));
            // 外圆角矩形内、内圆角矩形（内缩 width）外 → 描边带。
            let in_outer = inside_rounded(
                gx,
                gy,
                i64::from(ox),
                i64::from(oy),
                i64::from(iw),
                i64::from(ih),
                r,
            );
            let in_inner = inside_rounded(
                gx,
                gy,
                i64::from(ox) + bw,
                i64::from(oy) + bw,
                i64::from(iw) - 2 * bw,
                i64::from(ih) - 2 * bw,
                (r - bw).max(0),
            );
            if in_outer && !in_inner {
                canvas.put_pixel(x + ox, y + oy, b.color);
            }
        }
    }
}

/// 生成渐变背景。
fn gradient(w: u32, h: u32, start: Rgba<u8>, end: Rgba<u8>, dir: GradientDirection) -> RgbaImage {
    let mut img = RgbaImage::new(w, h);
    let span_x = (w.saturating_sub(1)).max(1) as f32;
    let span_y = (h.saturating_sub(1)).max(1) as f32;
    for y in 0..h {
        for x in 0..w {
            let t = match dir {
                GradientDirection::Horizontal => x as f32 / span_x,
                GradientDirection::Vertical => y as f32 / span_y,
                GradientDirection::Diagonal => (x as f32 + y as f32) / (span_x + span_y),
            }
            .clamp(0.0, 1.0);
            img.put_pixel(x, y, lerp(start, end, t));
        }
    }
    img
}

/// 线性插值两个颜色。
fn lerp(a: Rgba<u8>, b: Rgba<u8>, t: f32) -> Rgba<u8> {
    Rgba([
        lerp_ch(a.0[0], b.0[0], t),
        lerp_ch(a.0[1], b.0[1], t),
        lerp_ch(a.0[2], b.0[2], t),
        lerp_ch(a.0[3], b.0[3], t),
    ])
}

fn lerp_ch(a: u8, b: u8, t: f32) -> u8 {
    (f32::from(a) * (1.0 - t) + f32::from(b) * t)
        .round()
        .clamp(0.0, 255.0) as u8
}

fn blend_ch(src: u8, dst: u8, sa: f32, inv: f32) -> u8 {
    (f32::from(src) * sa + f32::from(dst) * inv)
        .round()
        .clamp(0.0, 255.0) as u8
}
