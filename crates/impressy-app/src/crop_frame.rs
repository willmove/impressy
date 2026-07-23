//! 交互式裁剪 / 选区（Requirement 47.1–47.3、Requirement 6.2–6.5）。
//!
//! 用 gpui 的 [`canvas()`] 渲染叠加层——`canvas()` 返回的 `Canvas<T>` 自身就是一份
//! 完整的三阶段 Element 实现（request_layout / prepaint / paint），满足 Requirement
//! 47.11。**选区状态与命中测试不在画布里**：它们放在 [`Selection`] 实体里，由宿主
//! 视图的鼠标处理器（`on_mouse_down` / `on_mouse_move` / `on_mouse_up`）驱动；`canvas()`
//! 只负责绘制（暗罩 + 选框 + 8 个把手），并在 prepaint 阶段把元素边界写回 [`Selection`]，
//! 供下一轮命中测试换算坐标。这正是 `vendor-docs` 里 `input.rs` 的状态回写模式。
//!
//! [`Selection`] 被设计为独立实体，图片编辑页可直接复用同一套交互模型，
//! 不把手势逻辑耦合到具体页面。

use gpui::{
    App, Bounds, Context, Entity, IntoElement, Pixels, Point, Styled, canvas, fill, point, px,
    rgba, size,
};
use impressy_core::transform::{AspectRatio, CropRect};

/// 把手半径（命中与绘制的把手半边长，像素）。
const HANDLE_HALF: f32 = 6.0;
/// 选框描边宽度（像素）。
const BORDER: f32 = 1.5;
/// 选区最小归一化边长，防止缩成 0。
const MIN_EDGE: f32 = 0.02;

/// 归一化矩形，各分量落在 `[0,1]`，`w`/`h` 为正。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NormRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl NormRect {
    /// 覆盖整个画布。
    pub const FULL: Self = Self {
        x: 0.0,
        y: 0.0,
        w: 1.0,
        h: 1.0,
    };

    /// 居中、给定比例下面积最大的归一化矩形（比例精确，非近似）。
    pub fn centered(ratio: AspectRatio) -> Self {
        let rv = ratio.value() as f32;
        // 令宽 = 1：若 1/rv <= 1 则高取 1/rv；否则缩到高=1、宽=rv。
        let (w, h) = if rv >= 1.0 {
            (1.0, 1.0 / rv)
        } else {
            (rv, 1.0)
        };
        Self {
            x: (1.0 - w) / 2.0,
            y: (1.0 - h) / 2.0,
            w,
            h,
        }
    }

    /// 钳制到 `[0,1]`，并保证最小边长。
    fn clamp(self) -> Self {
        let w = self.w.clamp(MIN_EDGE, 1.0);
        let h = self.h.clamp(MIN_EDGE, 1.0);
        Self {
            x: self.x.clamp(0.0, 1.0 - w),
            y: self.y.clamp(0.0, 1.0 - h),
            w,
            h,
        }
    }
}

/// 把手种类：整体移动 + 4 角 + 4 边。角把手在锁定比例时强制保持比例（Requirement 6.5）。
#[derive(Clone, Copy, Debug, PartialEq)]
enum Grip {
    Move,
    N,
    S,
    E,
    W,
    Ne,
    Nw,
    Se,
    Sw,
}

/// 一次拖拽的快照：抓的是哪个把手、按下时的选区、按下时的归一化光标。
#[derive(Clone, Copy)]
struct Drag {
    grip: Grip,
    origin: NormRect,
    anchor: (f32, f32),
}

/// 选区交互状态（作为独立 `Entity`，便于图片编辑页复用）。
pub struct Selection {
    /// 当前选区（归一化）。
    pub rect: NormRect,
    /// 锁定的宽高比；`None` 为自由比例。
    pub ratio: Option<AspectRatio>,
    drag: Option<Drag>,
    /// 上一次绘制得到的元素边界，命中测试据此把光标换算到归一化空间。
    bounds: Option<Bounds<Pixels>>,
}

impl Selection {
    /// 新建：比例已知时取居中最大矩形，否则默认全幅。
    pub fn new(ratio: Option<AspectRatio>) -> Self {
        let rect = match ratio {
            Some(r) => NormRect::centered(r),
            None => NormRect::FULL,
        };
        Self {
            rect,
            ratio,
            drag: None,
            bounds: None,
        }
    }

    /// 切换比例并重置为该比例下的居中矩形。
    pub fn set_ratio(&mut self, ratio: Option<AspectRatio>, cx: &mut Context<Self>) {
        self.ratio = ratio;
        self.rect = match ratio {
            Some(r) => NormRect::centered(r),
            None => NormRect::FULL,
        };
        cx.notify();
    }

    /// 鼠标按下：命中把手或选区内部，记录拖拽起点。
    pub fn on_down(&mut self, pos: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(b) = self.bounds else {
            return;
        };
        let anchor = norm_at(pos, b);
        let Some(grip) = self.grip_at(pos, b) else {
            return;
        };
        self.drag = Some(Drag {
            grip,
            origin: self.rect,
            anchor,
        });
        cx.notify();
    }

    /// 鼠标移动：按下时拖动把手，否则仅刷新光标下的把手预览（不改选区）。
    pub fn on_move(&mut self, pos: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(b) = self.bounds else {
            return;
        };
        let Some(drag) = self.drag else {
            return;
        };
        let cur = norm_at(pos, b);
        self.rect = match drag.grip {
            Grip::Move => {
                let (dx, dy) = (cur.0 - drag.anchor.0, cur.1 - drag.anchor.1);
                let x = (drag.origin.x + dx).clamp(0.0, 1.0 - drag.origin.w);
                let y = (drag.origin.y + dy).clamp(0.0, 1.0 - drag.origin.h);
                NormRect {
                    x,
                    y,
                    ..drag.origin
                }
            }
            grip => self.resize(grip, drag, cur),
        }
        .clamp();
        cx.notify();
    }

    /// 鼠标抬起：结束拖拽。
    pub fn on_up(&mut self, cx: &mut Context<Self>) {
        if self.drag.take().is_some() {
            cx.notify();
        }
    }

    /// 按当前选区与图像尺寸，换算成像素裁剪矩形。
    pub fn to_crop_rect(&self, iw: u32, ih: u32) -> impressy_core::error::Result<CropRect> {
        let r = self.rect.clamp();
        let x = ((r.x * iw as f32).round() as u32).min(iw.saturating_sub(1));
        let y = ((r.y * ih as f32).round() as u32).min(ih.saturating_sub(1));
        let x2 = (((r.x + r.w) * iw as f32).round() as u32).clamp(x + 1, iw);
        let y2 = (((r.y + r.h) * ih as f32).round() as u32).clamp(y + 1, ih);
        CropRect::new(x, y, x2 - x, y2 - y)
    }

    /// 缩放把手：角把手在锁定比例时保持比例（Requirement 6.5），其余自由。
    fn resize(&self, grip: Grip, drag: Drag, cur: (f32, f32)) -> NormRect {
        let o = drag.origin;
        // 对边 / 对角的固定锚点（归一化）。
        let (fix_x, fix_y) = match grip {
            Grip::N => (o.x, o.y + o.h),
            Grip::S => (o.x, o.y),
            Grip::E => (o.x, o.y),
            Grip::W => (o.x + o.w, o.y),
            Grip::Ne => (o.x, o.y + o.h),
            Grip::Nw => (o.x + o.w, o.y + o.h),
            Grip::Se => (o.x, o.y),
            Grip::Sw => (o.x + o.w, o.y),
            Grip::Move => unreachable!(),
        };
        let raw_w = (cur.0 - fix_x).abs();
        let raw_h = (cur.1 - fix_y).abs();

        let (mut w, mut h) = match (grip.is_corner(), self.ratio) {
            (true, Some(ratio)) => {
                // 角把手锁定比例：以拖动距离更约束的一轴为准。
                let rv = ratio.value() as f32; // w/h
                let by_w = raw_w;
                let by_h = raw_h * rv;
                let w = by_w.max(by_h);
                (w, w / rv)
            }
            _ => (raw_w, raw_h),
        };

        let sx = if cur.0 >= fix_x { 1.0 } else { -1.0 };
        let sy = if cur.1 >= fix_y { 1.0 } else { -1.0 };
        if grip.is_corner() && self.ratio.is_some() {
            let max_w = if sx > 0.0 { 1.0 - fix_x } else { fix_x };
            let max_h = if sy > 0.0 { 1.0 - fix_y } else { fix_y };
            let scale = (max_w / w.max(MIN_EDGE))
                .min(max_h / h.max(MIN_EDGE))
                .min(1.0);
            w *= scale;
            h *= scale;
        }
        NormRect {
            x: fix_x.min(fix_x + sx * w),
            y: fix_y.min(fix_y + sy * h),
            w,
            h,
        }
    }

    /// 命中测试：返回光标所在的把手；落在选区内部（非把手）返回 `Move`，外部返回 `None`。
    fn grip_at(&self, pos: Point<Pixels>, b: Bounds<Pixels>) -> Option<Grip> {
        let sel = abs_bounds(self.rect, b);
        let corners = [
            (Grip::Nw, sel.origin),
            (Grip::Ne, sel.top_right()),
            (Grip::Sw, sel.bottom_left()),
            (Grip::Se, sel.bottom_right()),
            (Grip::N, midpoint(sel.origin, sel.top_right())),
            (Grip::S, midpoint(sel.bottom_left(), sel.bottom_right())),
            (Grip::W, midpoint(sel.origin, sel.bottom_left())),
            (Grip::E, midpoint(sel.top_right(), sel.bottom_right())),
        ];
        let r = px(HANDLE_HALF);
        for (grip, c) in corners {
            if (pos.x - c.x).abs() <= r && (pos.y - c.y).abs() <= r {
                return Some(grip);
            }
        }
        if b.contains(&pos) && sel.contains(&pos) {
            Some(Grip::Move)
        } else {
            None
        }
    }
}

impl Grip {
    fn is_corner(self) -> bool {
        matches!(self, Grip::Ne | Grip::Nw | Grip::Se | Grip::Sw)
    }
}

/// 光标（窗口坐标）→ 归一化坐标。
fn norm_at(pos: Point<Pixels>, b: Bounds<Pixels>) -> (f32, f32) {
    (
        (pos.x - b.left()) / b.size.width,
        (pos.y - b.top()) / b.size.height,
    )
}

/// 归一化矩形 → 元素内像素矩形。
fn abs_bounds(r: NormRect, b: Bounds<Pixels>) -> Bounds<Pixels> {
    Bounds::new(
        point(b.left() + r.x * b.size.width, b.top() + r.y * b.size.height),
        size(r.w * b.size.width, r.h * b.size.height),
    )
}

fn midpoint(a: Point<Pixels>, b: Point<Pixels>) -> Point<Pixels> {
    point((a.x + b.x) * 0.5, (a.y + b.y) * 0.5)
}

/// 选区叠加层：暗罩 + 选框 + 8 把手。作为 `canvas()` 子元素挂到宿主视图。
///
/// `canvas()` 的 prepaint 把元素边界写回实体（供命中测试），paint 读取选区并绘制。
pub fn selection_overlay(state: Entity<Selection>) -> impl IntoElement {
    let state2 = state.clone();
    canvas(
        move |bounds: Bounds<Pixels>, _window, cx: &mut App| {
            state.update(cx, |s, _| s.bounds = Some(bounds));
        },
        move |bounds: Bounds<Pixels>, _, window, cx: &mut App| {
            let rect = state2.read(cx).rect;
            let sel = abs_bounds(rect, bounds);
            let dim = rgba(0x00000099);
            let edge = rgba(0xffffffcc);
            let handle_c = rgba(0xffffffff);
            let border = px(BORDER);
            let half = px(HANDLE_HALF);

            // 暗罩：选区外的四条带。
            let quads = [
                fill(
                    Bounds::new(
                        bounds.origin,
                        size(bounds.size.width, sel.top() - bounds.top()),
                    ),
                    dim,
                ),
                fill(
                    Bounds::new(
                        point(bounds.left(), sel.bottom()),
                        size(bounds.size.width, bounds.bottom() - sel.bottom()),
                    ),
                    dim,
                ),
                fill(
                    Bounds::new(
                        point(bounds.left(), sel.top()),
                        size(sel.left() - bounds.left(), sel.size.height),
                    ),
                    dim,
                ),
                fill(
                    Bounds::new(
                        point(sel.right(), sel.top()),
                        size(bounds.right() - sel.right(), sel.size.height),
                    ),
                    dim,
                ),
            ];
            for q in quads {
                window.paint_quad(q);
            }

            // 选框描边（四条窄条）。
            let outline = [
                fill(Bounds::new(sel.origin, size(sel.size.width, border)), edge),
                fill(
                    Bounds::new(
                        point(sel.left(), sel.bottom() - border),
                        size(sel.size.width, border),
                    ),
                    edge,
                ),
                fill(Bounds::new(sel.origin, size(border, sel.size.height)), edge),
                fill(
                    Bounds::new(
                        point(sel.right() - border, sel.top()),
                        size(border, sel.size.height),
                    ),
                    edge,
                ),
            ];
            for q in outline {
                window.paint_quad(q);
            }

            // 8 个把手。
            let handles = [
                sel.origin,
                sel.top_right(),
                sel.bottom_left(),
                sel.bottom_right(),
                midpoint(sel.origin, sel.top_right()),
                midpoint(sel.bottom_left(), sel.bottom_right()),
                midpoint(sel.origin, sel.bottom_left()),
                midpoint(sel.top_right(), sel.bottom_right()),
            ];
            for c in handles {
                window.paint_quad(fill(
                    Bounds::new(point(c.x - half, c.y - half), size(half * 2.0, half * 2.0)),
                    handle_c,
                ));
            }
        },
    )
    .size_full()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn centered_presets_keep_the_exact_requested_ratio() {
        for ratio in AspectRatio::PRESETS {
            let rect = NormRect::centered(ratio);
            let actual = rect.w / rect.h;
            assert!((actual - ratio.value() as f32).abs() < 0.000_01);
            assert!(rect.x >= 0.0 && rect.y >= 0.0);
            assert!(rect.x + rect.w <= 1.0 && rect.y + rect.h <= 1.0);
        }
    }

    #[test]
    fn locked_corner_resize_stays_in_bounds_and_preserves_ratio() {
        let ratio = AspectRatio::PORTRAIT_4_5;
        let selection = Selection::new(Some(ratio));
        let drag = Drag {
            grip: Grip::Se,
            origin: selection.rect,
            anchor: (selection.rect.x + selection.rect.w, 1.0),
        };
        let resized = selection.resize(Grip::Se, drag, (2.0, 2.0)).clamp();
        assert!(resized.x + resized.w <= 1.0);
        assert!(resized.y + resized.h <= 1.0);
        assert!((resized.w / resized.h - ratio.value() as f32).abs() < 0.000_01);
    }

    #[test]
    fn normalized_selection_maps_to_a_valid_pixel_crop() {
        let mut selection = Selection::new(Some(AspectRatio::WIDESCREEN_16_9));
        selection.rect = NormRect {
            x: 0.1,
            y: 0.2,
            w: 0.8,
            h: 0.45,
        };
        let crop = selection
            .to_crop_rect(1_000, 800)
            .expect("normalized selection should produce a crop");
        assert!(crop.fits_within(1_000, 800));
        assert_eq!(
            (crop.x, crop.y, crop.width, crop.height),
            (100, 160, 800, 360)
        );
    }
}
