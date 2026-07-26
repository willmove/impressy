//! 应用菜单栏：菜单结构、快捷键与全局动作分发。
//!
//! 菜单通过 `App::set_menus` 注册为平台菜单：macOS 由系统原生菜单栏直接呈现；
//! Windows / Linux 没有平台菜单栏，由 gpui-component 的 `AppMenuBar` 读取同一份
//! `set_menus` 数据在窗口标题行内渲染，两端共享同一套结构与动作。
//!
//! 动作经 `App::on_action` 注册为**全局**监听器（bubble 阶段末尾触发）：菜单点击、
//! 快捷键、无焦点起始分发三条路径都能到达同一个处理器。快捷键用 `secondary-`
//! 修饰键（macOS 为 Cmd，其余平台为 Ctrl）；gpui-component 输入框自身的
//! 复制 / 粘贴绑定挂在更深的 `Input` 上下文上，输入框聚焦时优先于这里的全局绑定。
//!
//! 语言切换后由 `AppShell::set_language` 重新调用 [`build_menus`] 刷新菜单文案。

use gpui::{App, Context, Entity, KeyBinding, Menu, MenuItem, SharedString, Window, actions};
use rust_i18n::t;

use crate::shell::AppShell;

actions!(
    app_menus,
    [
        /// 文件 · 打开图片…（secondary-O）
        OpenImages,
        /// 文件 · 打开多张…（secondary-Shift-O）
        OpenMultipleImages,
        /// 文件 · 保存结果…（secondary-S）
        SaveResult,
        /// 文件 · 打开输出目录
        RevealOutputDirectory,
        /// 文件 · 设置…（secondary-,）
        OpenSettings,
        /// 文件 · 退出（secondary-Q）
        QuitApp,
        /// 编辑 · 粘贴图片（secondary-V）
        PasteImage,
        /// 编辑 · 复制结果（secondary-Shift-C）
        CopyResult,
        /// 视图 · 首页
        GoHome,
        /// 视图 · 隐藏/显示侧栏（secondary-B）
        ToggleSidebar,
        /// 视图 · 切换语言
        SwitchLanguage,
        /// 帮助 · 关于 Impressy
        ShowAbout,
    ]
);

fn tr(key: &str) -> SharedString {
    t!(key).to_string().into()
}

/// 以当前语言构建四大菜单。语言切换或侧栏显隐变化后必须再次调用并 `set_menus`。
///
/// `sidebar_open` 决定视图菜单中侧栏项的文案：打开时为「隐藏侧栏」，关闭时为「显示侧栏」。
pub fn build_menus(sidebar_open: bool) -> Vec<Menu> {
    let sidebar_label = if sidebar_open {
        tr("menu.hide_sidebar")
    } else {
        tr("menu.show_sidebar")
    };
    vec![
        Menu {
            name: tr("menu.file"),
            items: vec![
                MenuItem::action(tr("menu.open"), OpenImages),
                MenuItem::action(tr("menu.open_multi"), OpenMultipleImages),
                MenuItem::separator(),
                MenuItem::action(tr("menu.save"), SaveResult),
                MenuItem::action(tr("menu.reveal_output"), RevealOutputDirectory),
                MenuItem::separator(),
                MenuItem::action(tr("menu.settings"), OpenSettings),
                MenuItem::separator(),
                MenuItem::action(tr("menu.quit"), QuitApp),
            ],
        },
        Menu {
            name: tr("menu.edit"),
            items: vec![
                MenuItem::action(tr("menu.paste"), PasteImage),
                MenuItem::action(tr("menu.copy"), CopyResult),
            ],
        },
        Menu {
            name: tr("menu.view"),
            items: vec![
                MenuItem::action(tr("menu.home"), GoHome),
                MenuItem::action(sidebar_label, ToggleSidebar),
                MenuItem::action(tr("menu.switch_language"), SwitchLanguage),
            ],
        },
        Menu {
            name: tr("menu.help"),
            items: vec![MenuItem::action(tr("menu.about"), ShowAbout)],
        },
    ]
}

/// 注册平台菜单与菜单项对应的全局快捷键。必须在创建窗口（及 `AppMenuBar`）之前调用。
pub fn install(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("secondary-o", OpenImages, None),
        KeyBinding::new("secondary-shift-o", OpenMultipleImages, None),
        KeyBinding::new("secondary-s", SaveResult, None),
        KeyBinding::new("secondary-v", PasteImage, None),
        KeyBinding::new("secondary-shift-c", CopyResult, None),
        KeyBinding::new("secondary-,", OpenSettings, None),
        KeyBinding::new("secondary-q", QuitApp, None),
        KeyBinding::new("secondary-b", ToggleSidebar, None),
    ]);
    // 启动时侧栏默认展开。
    cx.set_menus(build_menus(true));
}

/// 把菜单动作转发到唯一的 `AppShell` 窗口。菜单 / 快捷键都从全局 bubble 阶段进来，
/// 这里统一补上 `Window` 句柄后调用 `AppShell` 上的同名处理器。
pub fn register_actions(cx: &mut App, shell: &Entity<AppShell>) {
    macro_rules! on_menu_action {
        ($action:ty, $handler:ident) => {{
            let shell = shell.downgrade();
            cx.on_action(move |_: &$action, cx: &mut App| {
                // 全局监听器在 window.update 的 bubble 阶段内运行，此时窗口仍被
                // 占用，不能嵌套 update；推迟到当前分发结束后再处理。
                let shell = shell.clone();
                cx.defer(move |cx| {
                    dispatch_to_shell(&shell, cx, |shell, window, cx| {
                        shell.$handler(window, cx);
                    });
                });
            });
        }};
    }

    on_menu_action!(OpenImages, menu_open_images);
    on_menu_action!(OpenMultipleImages, menu_open_multiple_images);
    on_menu_action!(SaveResult, menu_save_result);
    on_menu_action!(RevealOutputDirectory, menu_reveal_output_directory);
    on_menu_action!(OpenSettings, menu_open_settings);
    on_menu_action!(QuitApp, menu_quit);
    on_menu_action!(PasteImage, menu_paste_image);
    on_menu_action!(CopyResult, menu_copy_result);
    on_menu_action!(GoHome, menu_go_home);
    on_menu_action!(ToggleSidebar, menu_toggle_sidebar);
    on_menu_action!(SwitchLanguage, menu_switch_language);
    on_menu_action!(ShowAbout, menu_show_about);
}

fn dispatch_to_shell(
    shell: &gpui::WeakEntity<AppShell>,
    cx: &mut App,
    handler: impl FnOnce(&mut AppShell, &mut Window, &mut Context<AppShell>),
) {
    // 单窗口应用：全局动作不携带窗口句柄，统一取第一个窗口补上。
    let Some(window) = cx.windows().first().copied() else {
        return;
    };
    _ = window.update(cx, |_, window, cx| {
        _ = shell.update(cx, |shell, cx| handler(shell, window, cx));
    });
}

// —— Windows / Linux 的窗口内菜单栏 ——

use gpui::{
    Corner, DismissEvent, Focusable, InteractiveElement, IntoElement, MouseButton, OwnedMenuItem,
    ParentElement, Render, StatefulInteractiveElement, Styled, Subscription, anchored, deferred,
    div, prelude::FluentBuilder, px,
};
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::menu::PopupMenu;
use gpui_component::{Selectable, Sizable, h_flex};

/// 窗口内菜单栏（Windows / Linux）。
///
/// 读取 `set_menus` 的同一份数据渲染触发按钮 + [`PopupMenu`] 下拉；菜单项动作经
/// `PopupMenu` 的 `action_context` 分发到全局监听器。每次渲染都重新读取 `get_menus()`，
/// 因此语言切换调用 `set_menus` 后文案自动更新，无需重建本视图。
///
/// 不直接用 gpui-component 的 `AppMenuBar`：在本开发机上其触发按钮始终无法接收
/// 点击（最小复现中同样失效，而相同样式的普通 `Button` 正常），根因未明；自有实现
/// 只依赖已实测验证的普通 `Button` + `PopupMenu` 路径。
pub struct MenuBar {
    open_ix: Option<usize>,
    /// 当前展开菜单的下拉实体。**只在打开时构建一次并缓存**，渲染时复用；每帧重建
    /// 会不断创建新实体并抢焦点，导致菜单项收不到点击（与上游 `AppMenuBar` 一致）。
    popup: Option<Entity<PopupMenu>>,
    _subscription: Option<Subscription>,
}

impl MenuBar {
    pub fn new(_cx: &mut Context<Self>) -> Self {
        Self {
            open_ix: None,
            popup: None,
            _subscription: None,
        }
    }

    fn toggle_menu(&mut self, ix: usize, cx: &mut Context<Self>) {
        if self.open_ix == Some(ix) {
            self.close_menu(cx);
        } else {
            self.open_menu(ix, cx);
        }
    }

    /// 展开第 `ix` 个菜单。切换菜单时丢弃旧下拉，让 [`build_popup`] 重建对应内容。
    fn open_menu(&mut self, ix: usize, cx: &mut Context<Self>) {
        if self.open_ix != Some(ix) {
            self.open_ix = Some(ix);
            self.popup = None;
            self._subscription = None;
            cx.notify();
        }
    }

    fn close_menu(&mut self, cx: &mut Context<Self>) {
        self.open_ix = None;
        self.popup = None;
        self._subscription = None;
        cx.notify();
    }

    fn build_popup(
        &mut self,
        ix: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<PopupMenu> {
        // 已有缓存：复用同一实体，仅在失焦时补焦点（键盘导航需要），避免每帧抢焦点。
        if let Some(popup) = self.popup.clone() {
            let focus_handle = popup.read(cx).focus_handle(cx);
            if !focus_handle.contains_focused(window, cx) {
                focus_handle.focus(window);
            }
            return popup;
        }

        let items = cx
            .get_menus()
            .and_then(|menus| menus.into_iter().nth(ix))
            .map(|menu| menu.items)
            .unwrap_or_default();
        let popup = PopupMenu::build(window, cx, |popup, window, cx| {
            let mut popup = popup.min_w(px(180.0));
            if let Some(handle) = window.focused(cx) {
                popup = popup.action_context(handle);
            }
            for item in items {
                popup = match item {
                    OwnedMenuItem::Action { name, action, .. } => {
                        popup.menu(name.clone(), action.boxed_clone())
                    }
                    OwnedMenuItem::Separator => popup.separator(),
                    _ => popup,
                };
            }
            popup
        });
        popup.read(cx).focus_handle(cx).focus(window);
        self._subscription = Some(cx.subscribe_in(
            &popup,
            window,
            |this, _, _: &DismissEvent, _, cx| {
                this.close_menu(cx);
            },
        ));
        self.popup = Some(popup.clone());
        popup
    }
}

impl Render for MenuBar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let names = cx
            .get_menus()
            .unwrap_or_default()
            .into_iter()
            .map(|menu| menu.name)
            .collect::<Vec<_>>();
        h_flex()
            .id("menu-bar")
            .gap_1()
            .children(names.into_iter().enumerate().map(|(ix, name)| {
                let is_open = self.open_ix == Some(ix);
                div()
                    .id(("menu-trigger", ix))
                    .relative()
                    .child(
                        Button::new(("menu-button", ix))
                            .small()
                            .compact()
                            .ghost()
                            .label(name)
                            .selected(is_open)
                            .on_mouse_down(MouseButton::Left, |_, window, cx| {
                                // 阻止事件冒泡到窗口，避免触发拖拽 / 抢焦点吃掉点击。
                                window.prevent_default();
                                cx.stop_propagation();
                            })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.toggle_menu(ix, cx);
                            })),
                    )
                    .on_hover(cx.listener(move |this, hovered, _, cx| {
                        // 已有菜单展开时，悬停切换到相邻菜单（桌面菜单栏惯例）。
                        if *hovered && this.open_ix.is_some() && this.open_ix != Some(ix) {
                            this.open_menu(ix, cx);
                        }
                    }))
                    .when(is_open, |this| {
                        this.child(deferred(
                            anchored()
                                .anchor(Corner::TopLeft)
                                .snap_to_window_with_margin(px(8.0))
                                .child(
                                    div()
                                        .size_full()
                                        .occlude()
                                        .top_1()
                                        .child(self.build_popup(ix, window, cx)),
                                ),
                        ))
                    })
                    .into_any_element()
            }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn action_names(menus: &[Menu]) -> Vec<String> {
        let mut names = menus
            .iter()
            .flat_map(|menu| menu.items.iter())
            .filter_map(|item| match item {
                MenuItem::Action { action, .. } => Some(action.name().to_string()),
                _ => None,
            })
            .collect::<Vec<_>>();
        names.sort();
        names
    }

    #[test]
    fn menus_cover_every_action_exactly_once() {
        let menus = build_menus(true);
        assert_eq!(menus.len(), 4);

        let mut expected = [
            "app_menus::OpenImages",
            "app_menus::OpenMultipleImages",
            "app_menus::SaveResult",
            "app_menus::RevealOutputDirectory",
            "app_menus::OpenSettings",
            "app_menus::QuitApp",
            "app_menus::PasteImage",
            "app_menus::CopyResult",
            "app_menus::GoHome",
            "app_menus::ToggleSidebar",
            "app_menus::SwitchLanguage",
            "app_menus::ShowAbout",
        ]
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<_>>();
        expected.sort();

        assert_eq!(action_names(&menus), expected);
    }

    #[test]
    fn menu_names_come_from_i18n_keys() {
        let menus = build_menus(true);
        let names = menus
            .iter()
            .map(|menu| menu.name.to_string())
            .collect::<Vec<_>>();
        // rust-i18n 缺键时回退为键名本身，这里确保四个菜单名都已被翻译。
        assert!(
            names
                .iter()
                .all(|name| !name.starts_with("menu.") && !name.is_empty())
        );
    }

    #[test]
    fn sidebar_menu_label_follows_visibility() {
        let hide = action_label(&build_menus(true), "app_menus::ToggleSidebar");
        let show = action_label(&build_menus(false), "app_menus::ToggleSidebar");
        assert_ne!(hide, show);
        assert!(!hide.is_empty());
        assert!(!show.is_empty());
        assert!(!hide.starts_with("menu."));
        assert!(!show.starts_with("menu."));
    }

    fn action_label(menus: &[Menu], action_name: &str) -> String {
        menus
            .iter()
            .flat_map(|menu| menu.items.iter())
            .find_map(|item| match item {
                MenuItem::Action { name, action, .. } if action.name() == action_name => {
                    Some(name.to_string())
                }
                _ => None,
            })
            .expect("ToggleSidebar should be present in View menu")
    }
}
