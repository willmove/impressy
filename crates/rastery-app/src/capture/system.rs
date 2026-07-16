//! 真机截图后端（仅 `system` feature 编译）。
//!
//! 用 [`SystemCaptureDevice`]（`xcap`）抓取主屏全幅 → 在主线程打开覆盖层。
//!
//! **本模块默认不编译**；须在真机以 `cargo run --features system` 构建并验收（ADR-0002）。
//! 无头 VM 上 `xcap` 虽能编译，但无显示服务器时 `detect_displays()` 返回空，故会回退到占位图。
//!
//! **未完成项（须真机）**：全局热键（Requirement 13.1、49）的事件泵与 GPUI 主循环桥接。
//! `SystemCaptureDevice::register_hotkey` 已能注册热键，但 `global-hotkey` 的事件回调如何
//! 安全地把 `start` 投递回 GPUI 主线程，涉及平台事件循环集成，只能在真机调试。

use std::sync::Arc;

use gpui::App;

use rastery_capture::clipboard::{Clipboard, NullClipboard, SystemClipboard};
use rastery_capture::device::{CaptureDevice, PhysicalRegion, SystemCaptureDevice};

use super::{open_overlay, placeholder_frame};

/// 真机入口：抓主屏 → 打开覆盖层。抓取失败或无显示器则回退到占位图。
pub fn start(cx: &mut App) {
    let device = match SystemCaptureDevice::new() {
        Ok(d) => Arc::new(d),
        Err(_) => {
            open_overlay(placeholder_frame(), null_clip(), cx);
            return;
        }
    };
    let clip = match SystemClipboard::new() {
        Ok(c) => Arc::new(c) as Arc<dyn Clipboard>,
        Err(_) => null_clip(),
    };

    let dev = device;
    cx.spawn(async move |cx| {
        let primary = dev.detect_displays().into_iter().next();
        let frame = match primary {
            Some(disp) => {
                let region = PhysicalRegion {
                    x: 0,
                    y: 0,
                    width: disp.physical_resolution.0,
                    height: disp.physical_resolution.1,
                };
                dev.capture_region(disp.id, region).await.ok()
            }
            None => None,
        }
        .unwrap_or_else(placeholder_frame);
        let _ = cx.update(|cx| open_overlay(frame, clip, cx));
    })
    .detach();
}

fn null_clip() -> Arc<dyn Clipboard> {
    Arc::new(NullClipboard)
}
