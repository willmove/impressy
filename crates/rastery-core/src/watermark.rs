//! 水印（FR-03，Requirement 8）。
//!
//! **设计取舍：本模块无字体依赖。** 文字水印由调用方（UI 层）栅格化为带 alpha 的
//! [`RgbaImage`] 后传入；core 只负责按位置与透明度合成。这让 core 保持纯函数、可
//! 完全 headless 测试——文字栅格化依赖系统字体，在无头环境无法验证，而合成逻辑
//! （位置、平铺、透明度）与水印内容无关，可被 property test 覆盖。

use crate::error::{CoreError, Result};
use image::{Rgba, RgbaImage};

/// 水印位置（Requirement 8.5）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Position {
    /// 右下角，距边缘 `margin` 像素。
    BottomRight {
        /// 距右、下边缘的留白。
        margin: u32,
    },
    /// 平铺，相邻水印间距 `spacing` 像素。
    Tiled {
        /// 水印之间的间距。
        spacing: u32,
    },
}

/// 应用水印（Requirement 8.9）。
///
/// `overlay` 为水印像素（带 alpha）；`opacity`（0.0–1.0）整体调节水印不透明度。
/// 返回新图像，**不修改输入**。
///
/// 输出尺寸恒等于输入尺寸（Requirement 54.4：水印不改变图幅与数量）。
pub fn apply(
    img: &RgbaImage,
    overlay: &RgbaImage,
    opacity: f32,
    position: Position,
) -> Result<RgbaImage> {
    if !(0.0..=1.0).contains(&opacity) {
        return Err(CoreError::InvalidArgument(format!(
            "opacity 必须在 0.0–1.0，收到 {opacity}"
        )));
    }
    if overlay.width() == 0 || overlay.height() == 0 {
        return Err(CoreError::InvalidArgument("水印不能为空图".into()));
    }

    let mut out = img.clone();
    let (ow, oh) = overlay.dimensions();
    let (iw, ih) = img.dimensions();

    match position {
        Position::BottomRight { margin } => {
            // 距右下角 margin；若图比水印+margin 小则贴到 0。
            let x = iw.saturating_sub(ow.saturating_add(margin));
            let y = ih.saturating_sub(oh.saturating_add(margin));
            stamp(&mut out, overlay, x, y, opacity);
        }
        Position::Tiled { spacing } => {
            let step_x = ow + spacing;
            let step_y = oh + spacing;
            let mut sy = 0u32;
            while sy < ih {
                let mut sx = 0u32;
                while sx < iw {
                    stamp(&mut out, overlay, sx, sy, opacity);
                    sx += step_x;
                }
                sy += step_y;
            }
        }
    }
    Ok(out)
}

/// 把 `overlay` 以 `opacity` 合成到 `out` 的 (ox, oy) 处，越界裁掉。
fn stamp(out: &mut RgbaImage, overlay: &RgbaImage, ox: u32, oy: u32, opacity: f32) {
    let (ow, oh) = overlay.dimensions();
    let (w, h) = out.dimensions();
    for dy in 0..oh {
        let py = oy.saturating_add(dy);
        if py >= h {
            break;
        }
        for dx in 0..ow {
            let px = ox.saturating_add(dx);
            if px >= w {
                break;
            }
            let src = *overlay.get_pixel(dx, dy);
            if src.0[3] == 0 {
                continue;
            }
            let base = *out.get_pixel(px, py);
            out.put_pixel(px, py, blend(base, src, opacity));
        }
    }
}

/// source-over 合成，水印整体乘以 opacity。
fn blend(base: Rgba<u8>, src: Rgba<u8>, opacity: f32) -> Rgba<u8> {
    let sa = f32::from(src.0[3]) / 255.0 * opacity;
    let inv = 1.0 - sa;
    Rgba([
        ch(src.0[0], base.0[0], sa, inv),
        ch(src.0[1], base.0[1], sa, inv),
        ch(src.0[2], base.0[2], sa, inv),
        ((sa + f32::from(base.0[3]) / 255.0 * inv) * 255.0)
            .round()
            .clamp(0.0, 255.0) as u8,
    ])
}

fn ch(src: u8, dst: u8, sa: f32, inv: f32) -> u8 {
    (f32::from(src) * sa + f32::from(dst) * inv)
        .round()
        .clamp(0.0, 255.0) as u8
}
