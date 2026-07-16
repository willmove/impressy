//! 截图标注合成：画笔、马赛克（FR-08 标注工具）。
//!
//! 对应 design.md `AnnotationLayer`。**本模块是纯函数、无系统依赖、可 headless 测试**——
//! 与 `rastery-core` 同性质。这是 `rastery-capture` 里唯一能在无头 VM 上自动验证的部分，
//! 因此优先做实、做全（截图/热键/取色只能真机验收，见 [`crate::device`]）。
//!
//! **文字标注不在此实现**：文字栅格化依赖系统字体，无头环境无法验证——与
//! `rastery_core::watermark` 的取舍一致（文字由 UI 层栅格化后作为图像叠加）。

use crate::error::{CaptureError, Result};
use image::{Rgba, RgbaImage};

/// 画布坐标系下的点（像素，可为负以便线段裁剪）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Point {
    /// x。
    pub x: i32,
    /// y。
    pub y: i32,
}

impl Point {
    /// 便捷构造。
    pub fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

/// 矩形区域（像素）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Region {
    /// 左上角 x。
    pub x: u32,
    /// 左上角 y。
    pub y: u32,
    /// 宽。
    pub width: u32,
    /// 高。
    pub height: u32,
}

/// 单条标注。
#[derive(Debug, Clone)]
pub enum Annotation {
    /// 画笔笔画：一串点，以给定粗细与颜色连成折线。
    Brush {
        /// 折线顶点，按顺序。
        points: Vec<Point>,
        /// 线宽（像素），至少按 1 处理。
        width: u32,
        /// 颜色（含 alpha）。
        color: Rgba<u8>,
    },
    /// 马赛克：把矩形区域按块打码。
    Mosaic {
        /// 目标区域。
        region: Region,
        /// 块边长（像素），必须 ≥ 1。
        block_size: u32,
    },
}

/// 标注图层：一组按添加顺序叠加的标注（design.md `AnnotationLayer`）。
#[derive(Debug, Clone, Default)]
pub struct AnnotationLayer {
    annotations: Vec<Annotation>,
}

impl AnnotationLayer {
    /// 新建空图层。
    pub fn new() -> Self {
        Self::default()
    }

    /// 添加画笔笔画（design.md `add_brush_stroke`）。
    pub fn add_brush_stroke(&mut self, points: Vec<Point>, width: u32, color: Rgba<u8>) {
        self.annotations.push(Annotation::Brush {
            points,
            width,
            color,
        });
    }

    /// 添加马赛克区域（design.md `add_mosaic_region`）。
    ///
    /// `block_size` 为 0 时返回错误——0 会导致除零。
    pub fn add_mosaic_region(&mut self, region: Region, block_size: u32) -> Result<()> {
        if block_size == 0 {
            return Err(CaptureError::InvalidArgument(
                "马赛克块大小必须为正".to_string(),
            ));
        }
        self.annotations
            .push(Annotation::Mosaic { region, block_size });
        Ok(())
    }

    /// 图层内标注数量。
    pub fn len(&self) -> usize {
        self.annotations.len()
    }

    /// 图层是否为空。
    pub fn is_empty(&self) -> bool {
        self.annotations.is_empty()
    }

    /// 把全部标注按顺序渲染到底图（design.md `render`）。
    ///
    /// **输出尺寸恒等于输入尺寸**：标注只改像素、不改图幅。空图层是恒等操作。
    pub fn render(&self, base: &mut RgbaImage) {
        for ann in &self.annotations {
            match ann {
                Annotation::Brush {
                    points,
                    width,
                    color,
                } => draw_brush(base, points, (*width).max(1), *color),
                Annotation::Mosaic { region, block_size } => {
                    apply_mosaic(base, *region, (*block_size).max(1));
                }
            }
        }
    }
}

/// 沿折线以圆形笔刷描绘。相邻顶点间线性插值取样，逐点盖圆盘，保证线条连续无断点。
fn draw_brush(img: &mut RgbaImage, points: &[Point], width: u32, color: Rgba<u8>) {
    let radius = (width as f32 / 2.0).max(0.5);
    if points.is_empty() {
        return;
    }
    if points.len() == 1 {
        stamp_disc(img, points[0], radius, color);
        return;
    }
    for pair in points.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let dx = (b.x - a.x) as f32;
        let dy = (b.y - a.y) as f32;
        let dist = (dx * dx + dy * dy).sqrt();
        // 每 0.5px 取一个采样点，避免快速移动时留下断点。
        let steps = (dist / 0.5).ceil().max(1.0) as i32;
        for s in 0..=steps {
            let t = s as f32 / steps as f32;
            let px = a.x as f32 + dx * t;
            let py = a.y as f32 + dy * t;
            stamp_disc(
                img,
                Point::new(px.round() as i32, py.round() as i32),
                radius,
                color,
            );
        }
    }
}

/// 在 (center) 处盖一个半径 `radius` 的实心圆盘，越界像素裁掉，按 source-over 混合。
fn stamp_disc(img: &mut RgbaImage, center: Point, radius: f32, color: Rgba<u8>) {
    let (w, h) = (img.width() as i32, img.height() as i32);
    let r = radius.ceil() as i32;
    let r2 = radius * radius;
    for oy in -r..=r {
        for ox in -r..=r {
            if (ox * ox + oy * oy) as f32 > r2 {
                continue;
            }
            let (x, y) = (center.x + ox, center.y + oy);
            if x < 0 || y < 0 || x >= w || y >= h {
                continue;
            }
            let dst = img.get_pixel_mut(x as u32, y as u32);
            *dst = blend(*dst, color);
        }
    }
}

/// 对矩形区域打马赛克：每个 `block_size × block_size` 块取平均色后整块填充。
///
/// 区域超出图像的部分自动裁到图内；边缘不足一整块的块按实际覆盖像素取平均。
fn apply_mosaic(img: &mut RgbaImage, region: Region, block_size: u32) {
    let (iw, ih) = img.dimensions();
    let x0 = region.x.min(iw);
    let y0 = region.y.min(ih);
    let x1 = region.x.saturating_add(region.width).min(iw);
    let y1 = region.y.saturating_add(region.height).min(ih);

    let mut by = y0;
    while by < y1 {
        let block_y1 = (by + block_size).min(y1);
        let mut bx = x0;
        while bx < x1 {
            let block_x1 = (bx + block_size).min(x1);
            let avg = block_average(img, bx, by, block_x1, block_y1);
            for y in by..block_y1 {
                for x in bx..block_x1 {
                    img.put_pixel(x, y, avg);
                }
            }
            bx = block_x1;
        }
        by = block_y1;
    }
}

/// 求 [x0,x1)×[y0,y1) 块的平均色（各通道独立平均，含 alpha）。
fn block_average(img: &RgbaImage, x0: u32, y0: u32, x1: u32, y1: u32) -> Rgba<u8> {
    let mut sum = [0u64; 4];
    let mut count = 0u64;
    for y in y0..y1 {
        for x in x0..x1 {
            let p = img.get_pixel(x, y).0;
            for c in 0..4 {
                sum[c] += u64::from(p[c]);
            }
            count += 1;
        }
    }
    if count == 0 {
        return Rgba([0, 0, 0, 0]);
    }
    Rgba([
        (sum[0] / count) as u8,
        (sum[1] / count) as u8,
        (sum[2] / count) as u8,
        (sum[3] / count) as u8,
    ])
}

/// 标准 source-over alpha 混合。
fn blend(dst: Rgba<u8>, src: Rgba<u8>) -> Rgba<u8> {
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
