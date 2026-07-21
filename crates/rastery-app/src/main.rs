//! rastery-app：GPUI 桌面应用入口。
//!
//! 四大板块导航（Requirement 34）+ 多语言（Requirement 35）。本地功能由
//! `rastery-core` 提供，AI 图片功能由 `rastery-ai` 与 `rastery-presets` 提供；三个视频
//! 入口仍为规划占位。本 crate 只负责 GPUI UI 与后台任务编排。
//!
//! 渲染正确性、交互手感、冷启动耗时以及 AI **保持不变项**须在对应真机和真实 Provider
//! 上验收；类型检查与假传输契约测试不能替代这些证据。

// `i18n!` 生成 `crate::_rust_i18n_t!` 等条目，必须在任何模块（`shell` 等在其中用 `t!`）
// 之前声明，否则子模块里的 `t!` 找不到 `crate::_rust_i18n_t`。
rust_i18n::i18n!("locales");

// 裁剪交互由图片编辑页直接接入，使用自定义 Element 完成三阶段绘制。
mod ai_state;
mod config_store;
mod crop_frame;
mod feature_params;
mod logging;
mod poster_canvas;
mod section;
mod shell;
mod text_watermark;
mod ui_message;
mod workspace;

use gpui::{
    App, AppContext, Application, Bounds, TitlebarOptions, WindowBounds, WindowOptions, px, rgb,
    size,
};
use gpui_component::{Root, Theme, ThemeMode};

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
    let app = Application::new().with_assets(gpui_component_assets::Assets);
    app.run(move |cx: &mut App| {
        // 必须在使用任何 gpui-component 功能之前调用。
        gpui_component::init(cx);
        Theme::change(ThemeMode::Light, None, cx);
        {
            let theme = Theme::global_mut(cx);
            theme.font_size = px(14.0);
            theme.radius = px(8.0);
            theme.radius_lg = px(12.0);
            theme.background = rgb(0xffffff).into();
            theme.foreground = rgb(0x182235).into();
            theme.border = rgb(0xdde5ef).into();
            theme.input = rgb(0xd5deea).into();
            theme.muted = rgb(0xedf2f7).into();
            theme.muted_foreground = rgb(0x7d8da7).into();
            theme.secondary = rgb(0xf4f7fb).into();
            theme.secondary_foreground = rgb(0x34435b).into();
            theme.secondary_hover = rgb(0xeef4fc).into();
            theme.primary = rgb(0x2867e8).into();
            theme.primary_hover = rgb(0x1f5edb).into();
            theme.primary_active = rgb(0x194fc0).into();
            theme.primary_foreground = rgb(0xffffff).into();
            theme.success = rgb(0x10a875).into();
            theme.success_foreground = rgb(0xffffff).into();
            theme.sidebar = rgb(0xf7f9fc).into();
            theme.sidebar_foreground = rgb(0x46566f).into();
            theme.sidebar_accent = rgb(0xeaf2ff).into();
            theme.sidebar_accent_foreground = rgb(0x2867e8).into();
            theme.sidebar_border = rgb(0xdde5ef).into();
            theme.ring = rgb(0x77a4ff).into();
        }
        // set_locale 同时切换应用与组件库文案。
        gpui_component::set_locale(loaded.config.language.locale());

        let bounds = Bounds::centered(None, size(px(1640.), px(920.)), cx);
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
