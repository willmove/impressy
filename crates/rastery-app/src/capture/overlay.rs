//! 截图覆盖层（Requirement 13.2–13.3、13.8–13.9、14、48）。
//!
//! 一个全屏无边框窗口：显示屏幕抓帧，用户**拖选区域**（复用 [`Selection`]），
//! 可切换到**标注模式**（复用 [`AnnotateCanvas`]），确认后把「抓帧 + 标注」按选区
//! 裁剪，**保存为文件 + 复制到剪贴板**。
//!
//! 取色读数（FR-09）：工具条实时显示光标下像素的 RGB 十六进制与色块。由于截图覆盖层
//! 是全屏且抓帧即屏幕分辨率，光标窗口坐标 ≈ 抓帧像素坐标（DPI 精度须真机验收）。
//!
//! **环境约束（ADR-0002）**：本机只 `cargo check`；抓帧、对话框、剪贴板、渲染须真机验收。

use std::sync::Arc;

use gpui::{
    ClickEvent, Context, Entity, IntoElement, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels,
    Point, RenderImage, SharedString, Window, div, img, prelude::*, px, rgb,
};
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::{ActiveTheme, h_flex, v_flex};
use image::RgbaImage;
use rust_i18n::t;

use rastery_capture::clipboard::Clipboard;
use rastery_core::format::{self, EncodeSettings, PngCompression};
use rastery_core::transform::{self, AspectRatio};

use crate::annotate_canvas::AnnotateCanvas;
use crate::crop_frame::{Selection, selection_overlay};
use crate::workspace::to_render_image;

/// 覆盖层模式：选区 / 标注。
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Select,
    Annotate,
}

/// 截图覆盖层视图。
pub struct CaptureOverlay {
    /// 原始抓帧（供取色读取像素）。
    frame: RgbaImage,
    /// 抓帧的 GPUI 可渲染副本。
    frame_view: Arc<RenderImage>,
    /// 区域选区（自由比例）。
    selection: Entity<Selection>,
    /// 标注画布。
    annotate: Entity<AnnotateCanvas>,
    /// 系统剪贴板（确认时复制）。
    clip: Arc<dyn Clipboard>,
    mode: Mode,
    /// 当前光标位置（取色读数用）。
    cursor: Option<Point<Pixels>>,
    /// 底部状态反馈。
    status: SharedString,
}

impl CaptureOverlay {
    /// 新建覆盖层。`frame` 为已抓取的屏幕图像。
    pub fn new(
        frame: RgbaImage,
        clip: Arc<dyn Clipboard>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let frame_view = to_render_image(&frame);
        let selection = cx.new(|_| Selection::new(None));
        let annotate = cx.new(|_| AnnotateCanvas::new(frame.clone()));
        Self {
            frame,
            frame_view,
            selection,
            annotate,
            clip,
            mode: Mode::Select,
            cursor: None,
            status: "".into(),
        }
    }

    /// 切换 选区 ⇄ 标注。
    fn toggle_mode(&mut self, _event: &ClickEvent, _window: &mut Window, cx: &mut Context<Self>) {
        self.mode = match self.mode {
            Mode::Select => Mode::Annotate,
            Mode::Annotate => Mode::Select,
        };
        cx.notify();
    }

    /// 确认：合成标注 → 按选区裁剪 → 保存 + 复制 → 关闭。
    fn confirm(&mut self, _event: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        let composited = self.annotate.read(cx).composite();
        let (w, h) = (composited.width(), composited.height());
        let cropped = match self.selection.read(cx).to_crop_rect(w, h) {
            Ok(rect) => match transform::crop(&composited, rect) {
                Ok(img) => img,
                Err(e) => {
                    self.status = format!("裁剪失败：{e}").into();
                    cx.notify();
                    return;
                }
            },
            Err(e) => {
                self.status = format!("{e}").into();
                cx.notify();
                return;
            }
        };
        self.save_and_copy(cropped, cx);
        window.remove_window();
    }

    /// 取消：直接关闭。
    fn cancel(&mut self, _event: &ClickEvent, window: &mut Window, _cx: &mut Context<Self>) {
        window.remove_window();
    }

    fn save_and_copy(&mut self, img: RgbaImage, _cx: &mut Context<Self>) {
        // 先复制到剪贴板（失败不阻断保存）。
        let copied = self.clip.copy_image(&img).is_ok();

        if let Some(path) = rfd::FileDialog::new()
            .add_filter("PNG", &["png"])
            .set_file_name("screenshot.png")
            .save_file()
        {
            match format::encode(&img, EncodeSettings::Png { compression: PngCompression::Default })
            {
                Ok(bytes) => match std::fs::write(&path, bytes) {
                    Ok(_) => {
                        self.status = if copied {
                            t!("capture.saved_copied").to_string()
                        } else {
                            t!("capture.saved_only").to_string()
                        }
                        .into();
                    }
                    Err(e) => self.status = format!("写入失败：{e}").into(),
                },
                Err(e) => self.status = format!("编码失败：{e}").into(),
            }
        } else if copied {
            self.status = t!("capture.copied").to_string().into();
        } else {
            self.status = t!("capture.nothing").to_string().into();
        }
    }

    // —— 选区模式鼠标处理：驱动 Selection + 更新取色光标 ——

    fn on_select_down(
        &mut self,
        event: &MouseDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.cursor = Some(event.position);
        let pos = event.position;
        self.selection.update(cx, |s, cx| s.on_down(pos, cx));
    }

    fn on_select_move(
        &mut self,
        event: &MouseMoveEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.cursor = Some(event.position);
        let pos = event.position;
        self.selection.update(cx, |s, cx| s.on_move(pos, cx));
    }

    fn on_select_up(
        &mut self,
        _event: &MouseUpEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.selection.update(cx, |s, cx| s.on_up(cx));
    }

    /// 光标下抓帧像素的 RGB（用于取色读数）。
    fn pixel_under_cursor(&self) -> Option<[u8; 3]> {
        let pos = self.cursor?;
        let (x, y) = pixel_of(pos, self.frame.width(), self.frame.height());
        let px = self.frame.get_pixel(x, y);
        Some([px.0[0], px.0[1], px.0[2]])
    }
    /// 选区比例预设按钮（Requirement 6.5 的比例锁定，复用于区域捕获）。
    fn ratio_button(
        &self,
        id: &'static str,
        label: SharedString,
        ratio: Option<AspectRatio>,
        cx: &mut Context<Self>,
    ) -> Button {
        Button::new(id)
            .label(label)
            .outline()
            .on_click(cx.listener(move |this, _, _, cx| {
                this.selection.update(cx, |s, cx| s.set_ratio(ratio, cx));
            }))
    }
}

/// 把光标窗口坐标换算为抓帧像素索引（全屏 1:1 假设；DPI 精度须真机验收）。
fn pixel_of(pos: Point<Pixels>, w: u32, h: u32) -> (u32, u32) {
    let xf = (pos.x / px(1.0)).max(0.0) as u32;
    let yf = (pos.y / px(1.0)).max(0.0) as u32;
    (xf.min(w.saturating_sub(1)), yf.min(h.saturating_sub(1)))
}

impl gpui::Render for CaptureOverlay {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let (bg, fg, border) = (theme.background, theme.foreground, theme.border);

        let mode = self.mode;
        let status = self.status.clone();
        let swatch = self.pixel_under_cursor();
        let hex: SharedString = swatch
            .map(|[r, g, b]| format!("#{:02X}{:02X}{:02X}", r, g, b))
            .unwrap_or_else(|| "—".into())
            .into();
        let swatch_color = swatch.map(|[r, g, b]| {
            rgb((u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b))
        });
        let annotate_label: SharedString = if mode == Mode::Select {
            t!("capture.annotate").to_string().into()
        } else {
            t!("capture.back_select").to_string().into()
        };
        let anno_count = (mode == Mode::Annotate)
            .then(|| self.annotate.read(cx).annotation_count());

        // 工具条
        let toolbar = h_flex()
            .w_full()
            .h(px(44.0))
            .px_3()
            .gap_2()
            .items_center()
            .bg(bg)
            .border_b_1()
            .border_color(border)
            .child(
                Button::new("overlay-mode")
                    .label(annotate_label)
                    .outline()
                    .on_click(cx.listener(Self::toggle_mode)),
            )
            .child(
                h_flex()
                    .gap_1()
                    .items_center()
                    .when_some(swatch_color, |this, c| {
                        this.child(
                            div()
                                .size(px(14.0))
                                .rounded_sm()
                                .bg(c)
                                .border_1()
                                .border_color(border),
                        )
                    })
                    .child(div().text_xs().text_color(fg).child(hex)),
            )
            .child(
                h_flex()
                    .gap_1()
                    .child(self.ratio_button("ratio-free", "Free".into(), None, cx))
                    .child(
                        self.ratio_button(
                            "ratio-1-1",
                            "1:1".into(),
                            Some(AspectRatio::SQUARE),
                            cx,
                        ),
                    )
                    .child(
                        self.ratio_button(
                            "ratio-16-9",
                            "16:9".into(),
                            Some(AspectRatio::WIDESCREEN_16_9),
                            cx,
                        ),
                    ),
            )
            .when_some(anno_count, |this, n| {
                this.child(
                    div()
                        .text_xs()
                        .text_color(fg)
                        .child(t!("annotate.count", count = n).to_string()),
                )
            })
            .child(div().flex_1())
            .child(
                Button::new("overlay-cancel")
                    .label(t!("capture.cancel").to_string())
                    .outline()
                    .on_click(cx.listener(Self::cancel)),
            )
            .child(
                Button::new("overlay-confirm")
                    .label(t!("capture.confirm").to_string())
                    .primary()
                    .on_click(cx.listener(Self::confirm)),
            );

        // 主体
        let body = match mode {
            Mode::Select => div()
                .id("overlay-select")
                .relative()
                .flex_1()
                .size_full()
                .child(img(self.frame_view.clone()))
                .child(selection_overlay(self.selection.clone()))
                .on_mouse_down(gpui::MouseButton::Left, cx.listener(Self::on_select_down))
                .on_mouse_move(cx.listener(Self::on_select_move))
                .on_mouse_up(gpui::MouseButton::Left, cx.listener(Self::on_select_up))
                .into_any_element(),
            Mode::Annotate => div()
                .flex_1()
                .size_full()
                .child(self.annotate.clone())
                .into_any_element(),
        };

        v_flex()
            .size_full()
            .bg(bg)
            .child(toolbar)
            .child(body)
            .when(!status.is_empty(), |this| {
                this.child(
                    div()
                        .absolute()
                        .bottom_0()
                        .w_full()
                        .px_3()
                        .py_1()
                        .text_xs()
                        .text_color(fg)
                        .bg(bg)
                        .child(status),
                )
            })
    }
}
