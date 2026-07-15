//! rastery-app：GPUI 主窗口与四板块导航骨架。
//!
//! 覆盖 Requirement 34（主界面导航）。v1 四板块中「基础图片处理」与「创作输出」
//! 有内容占位；「AI 生成与改图」与「行业定制 AI 工具」两块按 ADR-0001 做开发中占位。
//!
//! **环境约束（ADR-0002）**：本机能 `cargo check`、不能运行（无头 VM，无显示）。
//! 故此处只保证类型正确——渲染、交互手感须在真机验证。

use gpui::{
    div, prelude::*, px, rgb, size, App, Application, Bounds, Context, InteractiveElement,
    IntoElement, ParentElement, Render, SharedString, StatefulInteractiveElement, Styled, Window,
    WindowBounds, WindowOptions,
};
use rust_i18n::t;

rust_i18n::i18n!("locales");

/// 主界面板块（Requirement 34）。
#[derive(Clone, Copy, PartialEq, Eq)]
enum Section {
    /// 基础图片处理。
    BasicImage,
    /// AI 生成与改图（v2，占位）。
    AiGeneration,
    /// 行业定制 AI 工具（v2，占位）。
    IndustryTools,
    /// 创作输出。
    CreativeOutput,
}

impl Section {
    /// 四大板块，按导航顺序。
    const ALL: [Self; 4] = [
        Self::BasicImage,
        Self::AiGeneration,
        Self::IndustryTools,
        Self::CreativeOutput,
    ];

    /// 对应的 i18n 键。
    fn nav_key(self) -> &'static str {
        match self {
            Self::BasicImage => "nav.basic_image",
            Self::AiGeneration => "nav.ai_generation",
            Self::IndustryTools => "nav.industry_tools",
            Self::CreativeOutput => "nav.creative_output",
        }
    }

    /// v1 是否有实质内容（非占位）。AI 两块属 v2。
    fn has_content(self) -> bool {
        matches!(self, Self::BasicImage | Self::CreativeOutput)
    }
}

/// 应用根视图：侧边导航 + 内容区。
struct AppShell {
    /// 当前激活板块。
    active: Section,
}

impl AppShell {
    fn new() -> Self {
        Self {
            active: Section::BasicImage,
        }
    }

    /// 渲染单个导航按钮。点击切换激活板块。
    fn nav_button(&self, section: Section, cx: &mut Context<Self>) -> impl IntoElement {
        let active = self.active == section;
        let label: SharedString = t!(section.nav_key()).to_string().into();
        div()
            .id(label.clone())
            .px_4()
            .py_2()
            .rounded_md()
            .text_sm()
            .cursor_pointer()
            .text_color(if active {
                rgb(0xffffff)
            } else {
                rgb(0x9a9a9a)
            })
            .when(active, |b| b.bg(rgb(0x3a3a3a)))
            .hover(|b| b.bg(rgb(0x333333)))
            .child(label)
            .on_click(cx.listener(move |this, _, _, cx| {
                this.active = section;
                cx.notify();
            }))
    }

    /// 内容区：标题 + 占位说明。
    fn content(&self) -> impl IntoElement {
        let title: SharedString = t!(self.active.nav_key()).to_string().into();
        let body: SharedString = if self.active.has_content() {
            t!("placeholder.coming").to_string().into()
        } else {
            t!("placeholder.dev").to_string().into()
        };

        div()
            .flex_1()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_4()
            .text_color(rgb(0xcbcbcb))
            .child(div().text_xl().child(title))
            .child(div().child(body))
    }
}

impl Render for AppShell {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // 逐个构建导航项，避免闭包对 cx 的可变借用冲突。
        let mut nav_items: Vec<gpui::AnyElement> = Vec::with_capacity(Section::ALL.len());
        for section in Section::ALL {
            nav_items.push(self.nav_button(section, cx).into_any_element());
        }

        let app_title: SharedString = t!("app.title").to_string().into();

        div()
            .flex()
            .h_full()
            .w_full()
            .bg(rgb(0x1e1e1e))
            // 侧边导航
            .child(
                div()
                    .flex()
                    .flex_col()
                    .w(px(200.0))
                    .h_full()
                    .bg(rgb(0x2b2b2b))
                    .p_2()
                    .gap_1()
                    .child(
                        div()
                            .px_4()
                            .py_3()
                            .text_color(rgb(0xffffff))
                            .child(app_title),
                    )
                    .children(nav_items),
            )
            // 内容区
            .child(self.content())
    }
}

fn main() {
    // 默认简体中文。待采用 gpui-component 组件后改用 gpui_component::set_locale，
    // 以同步切换组件库自身文案（CLAUDE.md i18n 方案）。
    rust_i18n::set_locale("zh-CN");

    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(960.), px(640.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| cx.new(|_| AppShell::new()),
        )
        .unwrap();
        cx.activate(true);
    });
}
