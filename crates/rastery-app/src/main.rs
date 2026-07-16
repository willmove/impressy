//! rastery-app：GPUI 桌面应用入口。
//!
//! 四大板块导航（Requirement 34）+ 多语言（Requirement 35）。v1 板块「基础图片处理」
//! 与「创作输出」列出可用的本地功能；两个 AI 板块与三个视频功能按 ADR-0001 / Req 34
//! 做 v2「开发中」占位。图像处理逻辑全部在 `rastery-core`，本 crate 只负责 UI。
//!
//! **环境约束（ADR-0002）**：主开发机为无头云 VM，本 crate 只能 `cargo check`、不能
//! 运行。渲染正确性、交互手感、冷启动耗时须在真机（Win/macOS/Linux 桌面）验收。

// `i18n!` 生成 `crate::_rust_i18n_t!` 等条目，必须在任何模块（`shell` 等在其中用 `t!`）
// 之前声明，否则子模块里的 `t!` 找不到 `crate::_rust_i18n_t`。
rust_i18n::i18n!("locales");

mod annotate_canvas;
mod capture;
mod crop_frame;
 mod section;
 mod shell;
 mod workspace;

use gpui::{App, AppContext, Application, Bounds, WindowBounds, WindowOptions, px, size};
use gpui_component::Root;

use shell::AppShell;

fn main() {
    let app = Application::new();
    app.run(|cx: &mut App| {
        // 必须在使用任何 gpui-component 功能之前调用。
        gpui_component::init(cx);
        // 默认简体中文（Requirement 35）；set_locale 同时切换组件库文案。
        gpui_component::set_locale("zh-CN");

        let bounds = Bounds::centered(None, size(px(1024.), px(700.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |window, cx| {
                let view = cx.new(|_| AppShell::new());
                // 窗口第一层必须是 Root（gpui-component 的弹层 / 通知等依赖它）。
                cx.new(|cx| Root::new(view, window, cx))
            },
        )
        .unwrap();
        cx.activate(true);
    });
}
