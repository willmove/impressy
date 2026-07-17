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

// 裁剪交互由图片编辑页直接接入，使用自定义 Element 完成三阶段绘制。
mod config_store;
mod crop_frame;
mod feature_params;
mod logging;
mod section;
mod shell;
mod text_watermark;
mod ui_message;
mod workspace;

use gpui::{
    App, AppContext, Application, Bounds, TitlebarOptions, WindowBounds, WindowOptions, px, size,
};
use gpui_component::Root;

use config_store::{ConfigLoad, ConfigStore};
use rastery_core::config::AppConfig;
use shell::AppShell;

/// Native desktop identity shared by the Linux desktop file and macOS bundle.
const APP_ID: &str = "app.rastery.Rastery";

fn main() {
    logging::init();

    let (config_store, loaded) = match ConfigStore::for_current_user() {
        Ok(store) => {
            let loaded = store.load();
            (Some(store), loaded)
        }
        Err(error) => {
            log::error!("configuration directory unavailable: {error}");
            (
                None,
                ConfigLoad {
                    config: AppConfig::default(),
                    warning: Some(error.to_string()),
                },
            )
        }
    };

    let initial_paths = std::env::args_os()
        .skip(1)
        .map(std::path::PathBuf::from)
        .filter(|path| path.is_file())
        .collect::<Vec<_>>();
    let app = Application::new();
    app.run(move |cx: &mut App| {
        // 必须在使用任何 gpui-component 功能之前调用。
        gpui_component::init(cx);
        // set_locale 同时切换应用与组件库文案。
        gpui_component::set_locale(loaded.config.language.locale());

        let bounds = Bounds::centered(None, size(px(1024.), px(700.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                // 窗口标题（任务栏 / 标题栏显示的应用名）。
                titlebar: Some(TitlebarOptions {
                    title: Some("Rastery".into()),
                    ..Default::default()
                }),
                // Linux（Wayland）下的应用标识：影响 dock / 任务栏归类与 .desktop 匹配。
                app_id: Some(APP_ID.into()),
                ..Default::default()
            },
            move |window, cx| {
                let view = cx.new(|cx| {
                    AppShell::new(loaded.config, config_store, loaded.warning, window, cx)
                });
                if !initial_paths.is_empty() {
                    view.update(cx, |shell, cx| shell.open_initial_paths(initial_paths, cx));
                }
                // 窗口第一层必须是 Root（gpui-component 的弹层 / 通知等依赖它）。
                cx.new(|cx| Root::new(view, window, cx))
            },
        )
        .unwrap();
        cx.activate(true);
    });
}

#[cfg(test)]
mod tests {
    use super::APP_ID;

    #[test]
    fn desktop_identity_matches_packaging_assets() {
        assert_eq!(APP_ID, "app.rastery.Rastery");
    }
}
