//! 应用根视图：侧边导航 + 内容区（Requirement 34、35、36）。
//!
//! **环境约束（ADR-0002）**：本机为无头 VM，只能 `cargo check`、不能运行。因此本模块
//! 仅保证**类型正确**；渲染、交互手感、语言切换的即时性须在真机验证。
//!
//! 内容区是一个二级路由：
//! - 未打开具体功能时，展示当前板块的**功能目录**（卡片网格）。
//! - 打开某功能后，展示该功能页面：v1 功能给出接入 `rastery-core` 的脚手架，
//!   v2 功能给出「开发中」占位（Requirement 34.8–34.10）。

use gpui::{
    Context, InteractiveElement, IntoElement, ParentElement, Render, SharedString,
    StatefulInteractiveElement, Styled, Window, div, img, prelude::FluentBuilder, px,
};
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::{ActiveTheme, h_flex, v_flex};
use rust_i18n::t;

use crate::section::{Feature, Section};
use crate::workspace::Workspace;

/// 一个功能页动作按钮：元素 id、标签 i18n 键、对 [`Workspace`] 的操作、是否主按钮。
type Action = (&'static str, &'static str, fn(&mut Workspace), bool);

/// 取 i18n 文案并转为 GPUI 的 [`SharedString`]。
fn tr(key: &str) -> SharedString {
    t!(key).to_string().into()
}

/// 应用根视图。
pub struct AppShell {
    /// 当前激活板块。
    active: Section,
    /// 当前打开的功能；`None` 表示停留在板块的功能目录。
    open: Option<Feature>,
    /// 当前界面语言（`"zh-CN"` 或 `"en"`），用于语言切换按钮文案与状态。
    locale: SharedString,
    /// 图像工作区：功能页的打开 / 处理 / 保存共享此状态。
    workspace: Workspace,
}

impl AppShell {
    /// 以默认板块（基础图片处理）、默认语言（简体中文）新建。
    pub fn new() -> Self {
        Self {
            active: Section::BasicImage,
            open: None,
            locale: "zh-CN".into(),
            workspace: Workspace::default(),
        }
    }

    /// 切换中英文（Requirement 35.3）。
    ///
    /// 调用 `gpui_component::set_locale` 同时切换组件库自身文案（CLAUDE.md i18n 方案），
    /// 再 `notify` 触发本视图重渲染，使所有 `t!` 文案立即更新——无需重启。
    fn toggle_language(&mut self, cx: &mut Context<Self>) {
        let next = if self.locale.as_ref() == "zh-CN" {
            "en"
        } else {
            "zh-CN"
        };
        self.locale = next.into();
        gpui_component::set_locale(next);
        cx.notify();
    }

    /// 侧边栏单个板块导航项。点击切换板块并回到该板块的功能目录。
    fn nav_button(&self, section: Section, cx: &mut Context<Self>) -> impl IntoElement {
        let active = self.active == section && self.open.is_none();
        let theme = cx.theme();
        let (fg, bg_active, bg_hover) = (
            theme.sidebar_foreground,
            theme.sidebar_accent,
            theme.sidebar_accent,
        );
        div()
            .id(section.id())
            .px_3()
            .py_2()
            .rounded_md()
            .text_sm()
            .cursor_pointer()
            .text_color(fg)
            .when(active, |b| b.bg(bg_active))
            .hover(|b| b.bg(bg_hover))
            .child(tr(section.nav_key()))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.active = section;
                this.open = None;
                cx.notify();
            }))
    }

    /// 语言切换按钮。
    fn language_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        div()
            .id("lang-toggle")
            .px_3()
            .py_2()
            .rounded_md()
            .text_sm()
            .cursor_pointer()
            .border_1()
            .border_color(theme.sidebar_border)
            .text_color(theme.sidebar_foreground)
            .hover(|b| b.bg(theme.sidebar_accent))
            .child(tr("lang.toggle"))
            .on_click(cx.listener(|this, _, _, cx| this.toggle_language(cx)))
    }

    /// 功能目录中的单张卡片。点击打开该功能页面。
    fn feature_card(&self, feature: Feature, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let v1 = feature.is_v1();
        let badge_text = if v1 {
            tr("placeholder.local_badge")
        } else {
            SharedString::from("v2")
        };
        let badge_color = if v1 { theme.primary } else { theme.muted_foreground };

        v_flex()
            .id(feature.id())
            .w(px(260.0))
            .gap_2()
            .p_4()
            .rounded_lg()
            .border_1()
            .border_color(theme.border)
            .bg(theme.secondary)
            .cursor_pointer()
            .hover(|b| b.border_color(theme.primary))
            .child(
                h_flex()
                    .justify_between()
                    .items_center()
                    .child(div().text_sm().font_weight(gpui::FontWeight::SEMIBOLD).child(tr(feature.name_key())))
                    .child(div().text_xs().text_color(badge_color).child(badge_text)),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(tr(feature.desc_key())),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.open = Some(feature);
                cx.notify();
            }))
    }

    /// 板块功能目录：标题 + 功能卡片网格。
    fn section_home(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut cards: Vec<gpui::AnyElement> = Vec::new();
        for &feature in self.active.features() {
            cards.push(self.feature_card(feature, cx).into_any_element());
        }
        let theme = cx.theme();

        v_flex()
            .size_full()
            .p_6()
            .gap_4()
            .child(
                div()
                    .text_xl()
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_color(theme.foreground)
                    .child(tr(self.active.nav_key())),
            )
            .child(h_flex().flex_wrap().gap_4().children(cards))
    }

    /// 具体功能页面：v1 给脚手架，v2 给占位。
    fn feature_page(&self, feature: Feature, cx: &mut Context<Self>) -> impl IntoElement {
        // 先把要用到的颜色（`Hsla`，Copy）取出，结束对 cx 的不可变借用，
        // 之后才能对 cx 做可变借用（cx.listener / v1_scaffold）。
        let theme = cx.theme();
        let (muted, secondary, foreground) =
            (theme.muted_foreground, theme.secondary, theme.foreground);

        let back = div()
            .id("back")
            .px_3()
            .py_1()
            .rounded_md()
            .text_sm()
            .cursor_pointer()
            .text_color(muted)
            .hover(move |b| b.bg(secondary))
            .child(SharedString::from(format!("← {}", t!("placeholder.back"))))
            .on_click(cx.listener(|this, _, _, cx| {
                this.open = None;
                cx.notify();
            }));

        let body = if feature.is_v1() {
            self.v1_page(feature, cx).into_any_element()
        } else {
            // Requirement 34.8–34.10：v2 功能点开显示「开发中」占位。
            div()
                .text_color(muted)
                .child(tr("placeholder.dev"))
                .into_any_element()
        };

        v_flex()
            .size_full()
            .p_6()
            .gap_4()
            .child(back)
            .child(
                div()
                    .text_xl()
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_color(foreground)
                    .child(tr(feature.name_key())),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(muted)
                    .child(tr(feature.desc_key())),
            )
            .child(body)
    }

    /// 一个操作 [`Workspace`] 的按钮。点击执行 `action` 后 `notify` 重渲染（刷新预览/状态）。
    fn ws_button(
        &self,
        id: &'static str,
        label_key: &str,
        action: fn(&mut Workspace),
        is_primary: bool,
        cx: &mut Context<Self>,
    ) -> Button {
        let btn = Button::new(id).label(tr(label_key));
        let btn = if is_primary { btn.primary() } else { btn.outline() };
        btn.on_click(cx.listener(move |this, _, _, cx| {
            action(&mut this.workspace);
            cx.notify();
        }))
    }

    /// v1 功能页：动作按钮（打开 / 处理 / 保存，全部接 `rastery-core`）+ 预览 + 状态反馈。
    ///
    /// 文件对话框、预览渲染的实际效果须真机验收；核心变换的正确性由 `rastery-core`
    /// 的测试保证（ADR-0002）。
    fn v1_page(&self, feature: Feature, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let (border, secondary, muted, foreground, primary_c) = (
            theme.border,
            theme.secondary,
            theme.muted_foreground,
            theme.foreground,
            theme.primary,
        );

        let mut buttons: Vec<gpui::AnyElement> = Vec::new();
        for (id, label_key, action, is_primary) in feature_actions(feature) {
            buttons.push(
                self.ws_button(id, label_key, action, is_primary, cx)
                    .into_any_element(),
            );
        }

        let detail: SharedString = feature_detail(feature).into();
        let preview = self.workspace.preview();
        let status = self.workspace.status.clone();
        let info = self.workspace.info.clone();

        v_flex()
            .gap_3()
            .child(h_flex().flex_wrap().gap_2().children(buttons))
            .child(
                div()
                    .text_xs()
                    .text_color(primary_c)
                    .child(tr("placeholder.local_badge")),
            )
            .child(div().text_xs().text_color(muted).child(detail))
            .when(!info.is_empty(), |this| {
                this.child(div().text_sm().text_color(foreground).child(info))
            })
            .when(!status.is_empty(), |this| {
                this.child(div().text_sm().text_color(muted).child(status))
            })
            .when_some(preview, |this, arc| {
                this.child(
                    div()
                        .p_2()
                        .rounded_lg()
                        .border_1()
                        .border_color(border)
                        .bg(secondary)
                        .child(img(arc).max_h(px(320.0)).max_w(px(520.0))),
                )
            })
    }
}

impl Default for AppShell {
    fn default() -> Self {
        Self::new()
    }
}

/// 各 v1 功能背后 `rastery-core` 能力的具体说明。引用真实 core 数据/常量，
/// 保证 UI 与 core 的集成在编译期即被检查。
fn feature_detail(feature: Feature) -> String {
    use rastery_core::transform::AspectRatio;
    match feature {
        Feature::Edit => {
            let ratios: Vec<String> = AspectRatio::PRESETS.iter().map(|r| r.to_string()).collect();
            format!("rastery_core::transform · 比例预设：{}", ratios.join(" / "))
        }
        Feature::Collage => "rastery_core::collage · 纵向 / 横向 / 网格，统一单元格居中".into(),
        Feature::Batch => "rastery_core::batch · 单项失败不中断整批，失败项单独标记".into(),
        Feature::Slice => "rastery_core::slice · 保证切出恰好 行×列 块，余数分摊不丢像素".into(),
        Feature::QrCode => "rastery_core::qr · 生成带静区，识别用 rxing".into(),
        Feature::Exif => "rastery_core::exif · 容器层清除元数据，像素不变、幂等".into(),
        Feature::Beautify => "rastery_core::beautify · 圆角 / 内边距 / 渐变 / 描边 / 阴影".into(),
        Feature::Gif => "rastery_core::animation · 正序 / 倒序 / 乒乓，延迟 100–800ms".into(),
        _ => "rastery_core".into(),
    }
}

/// 各 v1 功能的动作按钮清单（接 [`Workspace`] 方法）。返回类型固定为 `Vec<Action>`，
/// 使各方法引用自动强转为 `fn(&mut Workspace)` 指针。
fn feature_actions(feature: Feature) -> Vec<Action> {
    match feature {
        Feature::Edit => vec![
            ("act-open", "action.open", Workspace::open_single, true),
            ("act-rot", "action.rotate90", Workspace::rotate90, false),
            ("act-crop", "action.crop_square", Workspace::crop_square, false),
            ("act-save", "action.save", Workspace::save_result, false),
        ],
        Feature::Collage => vec![
            ("act-open", "action.open_multi", Workspace::open_multiple, true),
            ("act-collage", "action.collage_v", Workspace::collage_vertical, false),
            ("act-save", "action.save", Workspace::save_result, false),
        ],
        Feature::Batch => vec![
            ("act-open", "action.open_multi", Workspace::open_multiple, true),
            ("act-batch", "action.batch_png", Workspace::batch_to_png, false),
        ],
        Feature::Slice => vec![
            ("act-open", "action.open", Workspace::open_single, true),
            ("act-slice", "action.slice_3x3", Workspace::slice_3x3, false),
        ],
        Feature::QrCode => vec![
            ("act-open", "action.open", Workspace::open_single, true),
            ("act-qr", "action.decode_qr", Workspace::decode_qr, false),
        ],
        Feature::Exif => vec![
            ("act-open", "action.open", Workspace::open_single, true),
            ("act-strip", "action.strip_exif", Workspace::strip_exif, false),
            ("act-save", "action.save", Workspace::save_result, false),
        ],
        Feature::Beautify => vec![
            ("act-open", "action.open", Workspace::open_single, true),
            ("act-beautify", "action.beautify", Workspace::beautify_default, false),
            ("act-save", "action.save", Workspace::save_result, false),
        ],
        Feature::Gif => vec![
            ("act-open", "action.open_multi", Workspace::open_multiple, true),
            ("act-gif", "action.gif", Workspace::make_gif, false),
            ("act-save", "action.save", Workspace::save_result, false),
        ],
        // 非 v1 功能不进入本页。
        _ => Vec::new(),
    }
}

impl Render for AppShell {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // 先把导航项收集到 Vec，避免闭包对 cx 的可变借用冲突。
        let mut nav_items: Vec<gpui::AnyElement> = Vec::with_capacity(Section::ALL.len());
        for section in Section::ALL {
            nav_items.push(self.nav_button(section, cx).into_any_element());
        }
        let lang = self.language_button(cx).into_any_element();

        let content = match self.open {
            Some(feature) => self.feature_page(feature, cx).into_any_element(),
            None => self.section_home(cx).into_any_element(),
        };

        let theme = cx.theme();
        h_flex()
            .size_full()
            .bg(theme.background)
            // 侧边导航
            .child(
                v_flex()
                    .w(px(220.0))
                    .h_full()
                    .bg(theme.sidebar)
                    .border_r_1()
                    .border_color(theme.sidebar_border)
                    .p_3()
                    .gap_1()
                    .child(
                        v_flex()
                            .px_2()
                            .py_3()
                            .gap_1()
                            .child(
                                div()
                                    .text_lg()
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .text_color(theme.sidebar_foreground)
                                    .child(tr("app.title")),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child(tr("app.subtitle")),
                            ),
                    )
                    .children(nav_items)
                    .child(div().flex_1())
                    .child(lang),
            )
            // 内容区
            .child(v_flex().flex_1().h_full().child(content))
    }
}
