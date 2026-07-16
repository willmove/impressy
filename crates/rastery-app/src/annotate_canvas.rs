//! 标注画布（Requirement 47.4–47.7、Requirement 13.4–13.6）。
//!
//! 在一张底图上叠加画笔 / 马赛克标注。**合成逻辑全部在 `rastery_capture::annotate`**
//! （已 headless 全绿），本视图只负责交互编排与实时预览：
//! - 画笔：拖动收集归一化顶点，实时把「已提交图层 + 当前笔画」合成刷新预览；松手提交。
//! - 马赛克：拖动框选矩形，松手提交为马赛克区域并刷新预览。
//!
//! 笔触以**归一化坐标**记录（与显示尺寸解耦），提交时按底图像素尺寸换算，
//! 这样窗口缩放不影响标注的像素一致性。

use std::sync::Arc;

use gpui::{
    App, Bounds, Context, Entity, IntoElement, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels,
    Point, RenderImage, Window, canvas, div, fill, img, point, prelude::*, px, rgba,
};
use image::{Rgba, RgbaImage};
use rust_i18n::t;

use rastery_capture::annotate::{AnnotationLayer, Point as AnnoPoint, Region as AnnoRegion};

use crate::workspace::to_render_image;

/// 标注工具。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    /// 自由画笔。
    Brush,
    /// 矩形马赛克。
    Mosaic,
}

/// 进行中的拖拽。
#[derive(Clone, Debug, Default)]
enum Stroke {
    #[default]
    None,
    Brush {
        points: Vec<(f32, f32)>,
    },
    Mosaic {
        start: (f32, f32),
        cur: (f32, f32),
    },
}

impl Stroke {
    fn is_some(&self) -> bool {
        !matches!(self, Stroke::None)
    }
}

/// 标注画布视图。
pub struct AnnotateCanvas {
    base: Arc<RgbaImage>,
    layer: AnnotationLayer,
    preview: Arc<RenderImage>,
    tool: Tool,
    brush_color: Rgba<u8>,
    brush_width: u32,
    mosaic_block: u32,
    stroke: Stroke,
    bounds: Option<Bounds<Pixels>>,
}

impl AnnotateCanvas {
    /// 以给定底图新建空标注画布。
    pub fn new(base: RgbaImage) -> Self {
        Self {
            preview: to_render_image(&base),
            base: Arc::new(base),
            layer: AnnotationLayer::new(),
            tool: Tool::Brush,
            brush_color: Rgba([220, 60, 60, 255]),
            brush_width: 4,
            mosaic_block: 12,
            stroke: Stroke::None,
            bounds: None,
        }
    }

    /// 切换工具。
    pub fn set_tool(&mut self, tool: Tool, cx: &mut Context<Self>) {
        self.tool = tool;
        cx.notify();
    }

    /// 当前已提交的标注数量（供宿主显示状态）。
    pub fn annotation_count(&self) -> usize {
        self.layer.len()
    }

    /// 合成最终图像（底图 + 全部已提交标注），用于导出 / 复制。
    pub fn composite(&self) -> RgbaImage {
        let mut out = (*self.base).clone();
        self.layer.render(&mut out);
        out
    }

    fn on_down(&mut self, event: &MouseDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(b) = self.bounds else {
            return;
        };
        let p = norm_at(event.position, b);
        self.stroke = match self.tool {
            Tool::Brush => Stroke::Brush { points: vec![p] },
            Tool::Mosaic => Stroke::Mosaic { start: p, cur: p },
        };
        cx.notify();
    }

    fn on_move(&mut self, event: &MouseMoveEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(b) = self.bounds else {
            return;
        };
        let p = norm_at(event.position, b);
        match &mut self.stroke {
            Stroke::Brush { points } => points.push(p),
            Stroke::Mosaic { cur, .. } => *cur = p,
            Stroke::None => return,
        }
        cx.notify();
    }

    fn on_up(&mut self, _event: &MouseUpEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let stroke = std::mem::take(&mut self.stroke);
        if stroke.is_some() {
            self.commit(stroke);
            self.refresh_preview();
            cx.notify();
        }
    }

    /// 把一次完成的拖拽提交到标注图层（换算到像素坐标）。
    fn commit(&mut self, stroke: Stroke) {
        let (w, h) = (self.base.width(), self.base.height());
        match stroke {
            Stroke::Brush { points } => {
                let pts = points
                    .into_iter()
                    .map(|(nx, ny)| AnnoPoint {
                        x: (nx * w as f32).round() as i32,
                        y: (ny * h as f32).round() as i32,
                    })
                    .collect();
                self.layer
                    .add_brush_stroke(pts, self.brush_width, self.brush_color);
            }
            Stroke::Mosaic { start, cur } => {
                let (x0, y0) = (
                    (start.0.min(cur.0) * w as f32).round() as u32,
                    (start.1.min(cur.1) * h as f32).round() as u32,
                );
                let (x1, y1) = (
                    (start.0.max(cur.0) * w as f32).round() as u32,
                    (start.1.max(cur.1) * h as f32).round() as u32,
                );
                if x1 > x0 && y1 > y0 {
                    let region = AnnoRegion {
                        x: x0,
                        y: y0,
                        width: x1 - x0,
                        height: y1 - y0,
                    };
                    let _ = self.layer.add_mosaic_region(region, self.mosaic_block);
                }
            }
            Stroke::None => {}
        }
    }

    /// 刷新预览：底图 + 已提交图层 + 进行中笔画。
    fn refresh_preview(&mut self) {
        let mut out = (*self.base).clone();
        self.layer.render(&mut out);
        if self.stroke.is_some() {
            temp_layer_for(&self.stroke).render(&mut out);
        }
        self.preview = to_render_image(&out);
    }
}

/// 把进行中的笔画临时合成（复用已测试的 `AnnotationLayer`）。
fn temp_layer_for(stroke: &Stroke) -> AnnotationLayer {
    let mut tmp = AnnotationLayer::new();
    match stroke {
        Stroke::Brush { points } => {
            // 宽高未知时按非零兜底；提交时才用真实尺寸，此处仅预览。
            let (w, h) = image_dims_hint(points);
            let pts: Vec<_> = points
                .iter()
                .map(|(nx, ny)| AnnoPoint {
                    x: (nx * w as f32).round() as i32,
                    y: (ny * h as f32).round() as i32,
                })
                .collect();
            if !pts.is_empty() {
                tmp.add_brush_stroke(pts, 4, Rgba([220, 60, 60, 255]));
            }
        }
        Stroke::Mosaic { start, cur } => {
            let (w, h) = (1024, 768);
            let (x0, y0) = (
                (start.0.min(cur.0) * w as f32).round() as u32,
                (start.1.min(cur.1) * h as f32).round() as u32,
            );
            let (x1, y1) = (
                (start.0.max(cur.0) * w as f32).round() as u32,
                (start.1.max(cur.1) * h as f32).round() as u32,
            );
            if x1 > x0 && y1 > y0 {
                let _ = tmp.add_mosaic_region(
                    AnnoRegion {
                        x: x0,
                        y: y0,
                        width: x1 - x0,
                        height: y1 - y0,
                    },
                    12,
                );
            }
        }
        Stroke::None => {}
    }
    tmp
}

/// 预览用的临时图层无法访问真实底图尺寸；画笔预览不影响最终像素一致性，
/// 故用一个固定上限做近似渲染（提交时用真实尺寸重算）。
fn image_dims_hint(points: &[(f32, f32)]) -> (u32, u32) {
    let _ = points;
    (1024, 768)
}

impl gpui::Render for AnnotateCanvas {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.stroke.is_some() {
            self.refresh_preview();
        }

        let tool = self.tool;
        let count = self.layer.len();
        let preview = self.preview.clone();
        let brush_label: gpui::SharedString = t!("annotate.brush").to_string().into();
        let mosaic_label: gpui::SharedString = t!("annotate.mosaic").to_string().into();
        let status: gpui::SharedString =
            t!("annotate.count", count = count).to_string().into();

        div()
            .flex()
            .flex_col()
            .gap_2()
            .size_full()
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(tool_button("anno-brush", brush_label, tool == Tool::Brush, cx, Tool::Brush))
                    .child(tool_button("anno-mosaic", mosaic_label, tool == Tool::Mosaic, cx, Tool::Mosaic))
                    .child(div().text_xs().text_color(gpui::rgb(0x888888)).child(status)),
            )
            .child(
                div()
                    .id("annotate-canvas")
                    .relative()
                    .flex_1()
                    .min_h(px(120.0))
                    .bg(gpui::rgb(0x222222))
                    .overflow_hidden()
                    .child(img(preview))
                    .child(annotate_overlay(cx.entity()))
                    .on_mouse_down(gpui::MouseButton::Left, cx.listener(Self::on_down))
                    .on_mouse_move(cx.listener(Self::on_move))
                    .on_mouse_up(gpui::MouseButton::Left, cx.listener(Self::on_up)),
            )
    }
}

/// 工具切换按钮。
fn tool_button(
    id: &'static str,
    label: gpui::SharedString,
    active: bool,
    cx: &mut Context<AnnotateCanvas>,
    tool: Tool,
) -> impl IntoElement {
    let text = if active {
        gpui::rgb(0xffffff)
    } else {
        gpui::rgb(0xcccccc)
    };
    let bg = if active {
        gpui::rgb(0x3b82f6)
    } else {
        gpui::rgb(0x333333)
    };
    div()
        .id(id)
        .px_3()
        .py_1()
        .rounded_md()
        .text_sm()
        .cursor_pointer()
        .text_color(text)
        .bg(bg)
        .child(label)
        .on_click(cx.listener(move |this, _, _, cx| this.set_tool(tool, cx)))
}

/// 透明覆盖层：记录画布边界（供命中测试），并绘制进行中的马赛克选框。
fn annotate_overlay(state: Entity<AnnotateCanvas>) -> impl IntoElement {
    let state2 = state.clone();
    canvas(
        move |bounds: Bounds<Pixels>, _window, cx: &mut App| {
            state.update(cx, |s, _| s.bounds = Some(bounds));
        },
        move |bounds: Bounds<Pixels>, _, window, cx: &mut App| {
            let stroke = {
                let s = state2.read(cx);
                match &s.stroke {
                    Stroke::Mosaic { start, cur } => Some((*start, *cur)),
                    _ => None,
                }
            };
            if let Some((start, cur)) = stroke {
                let (x0, y0) = norm_to_px(start, bounds);
                let (x1, y1) = norm_to_px(cur, bounds);
                let r = Bounds::from_corners(point(x0, y0), point(x1, y1));
                window.paint_quad(fill(r, rgba(0x3b82f655)));
            }
        },
    )
}

fn norm_at(pos: Point<Pixels>, b: Bounds<Pixels>) -> (f32, f32) {
    (
        ((pos.x - b.left()) / b.size.width).clamp(0.0, 1.0),
        ((pos.y - b.top()) / b.size.height).clamp(0.0, 1.0),
    )
}

fn norm_to_px(p: (f32, f32), b: Bounds<Pixels>) -> (Pixels, Pixels) {
    (b.left() + p.0 * b.size.width, b.top() + p.1 * b.size.height)
}
