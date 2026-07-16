//! 截图启动器（Requirement 13.1、48、49）。
//!
//! 打开一个全屏无边框覆盖层（[`overlay::CaptureOverlay`]），用于区域选择 + 标注 + 保存/复制。
//!
//! **两套后端，按 feature 切换**（与 `rastery-capture` 一致，见 ADR-0002）：
//! - 默认（无头 VM / CI 基础 check）：用占位渐变图打开覆盖层。保证 UI **可编译、可在真机冒烟**，
//!   但不抓真实屏幕。
//! - `system` feature（真机 `--features system` 构建）：用真实 [`CaptureDevice`] 抓屏后打开覆盖层；
//!   全局热键的注册见 [`system`] 模块（其事件→UI 桥接须真机验收）。

pub mod overlay;

#[cfg(feature = "system")]
pub mod system;

use std::sync::Arc;

use gpui::{
    App, AppContext, Bounds, TitlebarOptions, WindowBounds, WindowKind, WindowOptions, point, px,
    size,
};
use image::{ImageBuffer, Rgba};
#[cfg(not(feature = "system"))]
use rastery_capture::clipboard::NullClipboard;
use rastery_capture::clipboard::Clipboard;

/// 打开截图覆盖层。
///
/// - 默认构建：占位渐变图 + `NullClipboard`（无头回退，仅冒烟 UI）。
/// - `system` feature：转交 [`system::start`]，抓真实屏幕。
#[cfg(not(feature = "system"))]
pub fn start_capture(cx: &mut App) {
    let frame = placeholder_frame();
    let clip: Arc<dyn Clipboard> = Arc::new(NullClipboard);
    open_overlay(frame, clip, cx);
}

#[cfg(feature = "system")]
pub fn start_capture(cx: &mut App) {
    system::start(cx);
}

/// 用给定抓帧与剪贴板打开一个全屏覆盖层窗口。
pub(crate) fn open_overlay(frame: image::RgbaImage, clip: Arc<dyn Clipboard>, cx: &mut App) {
    let restore = Bounds::new(point(px(0.), px(0.)), size(px(1280.), px(800.)));
    let _ = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Fullscreen(restore)),
            titlebar: Some(TitlebarOptions::default()),
            kind: WindowKind::PopUp,
            is_movable: false,
            is_resizable: false,
            focus: true,
            show: true,
            ..Default::default()
        },
        |window, cx| {
            let view = cx.new(|cx| overlay::CaptureOverlay::new(frame.clone(), clip.clone(), window, cx));
            cx.new(|cx| gpui_component::Root::new(view, window, cx))
        },
    );
}

/// 无头回退用的占位渐变图（1280×800），让覆盖层在无真实抓帧时也能渲染。
pub(crate) fn placeholder_frame() -> image::RgbaImage {
    let (w, h) = (1280u32, 800u32);
    let mut img: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let t = x as f32 / w as f32;
            let r = (40.0 + 60.0 * t) as u8;
            let g = (50.0 + 80.0 * (y as f32 / h as f32)) as u8;
            let b = (90.0 + 70.0 * (1.0 - t)) as u8;
            img.put_pixel(x, y, Rgba([r, g, b, 255]));
        }
    }
    img
}
