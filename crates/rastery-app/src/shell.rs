//! 应用根视图：四板块导航、设置与后台工作区。
//!
//! GPUI 主线程只更新状态和渲染；文件读取、编解码及图像变换通过 background
//! executor 执行。渲染和交互仍须按 ADR-0002 在桌面真机验收。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use gpui::{
    AnyElement, AppContext, ClipboardEntry, ClipboardItem, Context, Entity, ExternalPaths, Hsla,
    Image as ClipboardImage, ImageFormat as ClipboardImageFormat, InteractiveElement, IntoElement,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement, PathPromptOptions,
    Render, SharedString, StatefulInteractiveElement, Styled, Subscription, Timer, Window, black,
    div, img, prelude::FluentBuilder, px, white,
};
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::color_picker::{ColorPicker, ColorPickerEvent, ColorPickerState};
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::select::{Select, SelectEvent, SelectState};
use gpui_component::slider::{Slider, SliderEvent, SliderState};
use gpui_component::spinner::Spinner;
use gpui_component::tooltip::Tooltip;
use gpui_component::{
    ActiveTheme, Colorize, Disableable, Icon, IconName, IndexPath, Sizable, WindowExt, h_flex,
    v_flex,
};
use rastery_ai::{
    AiError, ApiKey, AspectRatio as AiAspectRatio, CredentialStore, GeneratedImage,
    GenerationQuality, GenerationRequest, ImageInput, ProviderId, ProviderRegistry,
    SystemCredentialStore,
};
use rastery_core::config::{AppConfig, Language};
use rastery_core::format::{self, EncodeSettings, OutputFormat, PngCompression, Quality};
use rastery_core::transform::Rotation;
use rastery_presets::{AiEditPreset, IndustryTool};
use rust_i18n::t;

use crate::ai_state::{AiErrorKind, AiStatus, AiUiState, RenderedAiImage};
use crate::config_store::ConfigStore;
use crate::crop_frame::{NormRect, Selection, selection_overlay};
use crate::feature_params::{BatchMode, FeatureParams, ParamAction, ParamEffect, WatermarkSource};
#[cfg(not(target_os = "macos"))]
use crate::menus::MenuBar;
use crate::poster_canvas::{
    POSTER_HEIGHT, POSTER_WIDTH, PosterLayout, PosterTextLayer, bounds_capture,
};
use crate::section::{Feature, Section};
use crate::ui_message::{ErrorKind, UiMessage};
use crate::workspace::{
    BatchRequest, ClipboardFormat, ClipboardPayload, OutputDirectoryCommand, TransformOperation,
    Workspace, WorkspaceCommand, WorkspaceCommandRoute, WorkspaceJob, WorkspaceOutcome,
    is_supported_image_path,
};

fn tr(key: &str) -> SharedString {
    t!(key).to_string().into()
}

enum WorkspaceEvent {
    Progress { completed: usize, total: usize },
    Finished(Box<WorkspaceOutcome>),
}

#[derive(Clone)]
struct AiPromptTask {
    tier_id: String,
    prompt: String,
}

enum AiTaskOutcome {
    Finished(Vec<RenderedAiImage>),
    Failed(AiErrorKind),
}

struct AiTaskRequest {
    registry: ProviderRegistry,
    credentials: Arc<dyn CredentialStore>,
    provider: ProviderId,
    feature: Feature,
    prompts: Vec<AiPromptTask>,
    images: Arc<Vec<rastery_core::RgbaImage>>,
    mask_rect: Option<NormRect>,
    aspect_ratio: AiAspectRatio,
    count: u8,
    quality: GenerationQuality,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PathPromptFailure {
    Platform,
    ChannelClosed,
}

enum PathPromptResult<T> {
    Selected(T),
    Cancelled,
    Failed(PathPromptFailure),
}

fn classify_path_prompt_result<T, PlatformError, ChannelError>(
    result: Result<Result<Option<T>, PlatformError>, ChannelError>,
) -> PathPromptResult<T> {
    match result {
        Ok(Ok(Some(selected))) => PathPromptResult::Selected(selected),
        Ok(Ok(None)) => PathPromptResult::Cancelled,
        Ok(Err(_)) => PathPromptResult::Failed(PathPromptFailure::Platform),
        Err(_) => PathPromptResult::Failed(PathPromptFailure::ChannelClosed),
    }
}

fn first_path_prompt_result(result: PathPromptResult<Vec<PathBuf>>) -> PathPromptResult<PathBuf> {
    match result {
        PathPromptResult::Selected(paths) => paths
            .into_iter()
            .next()
            .map(PathPromptResult::Selected)
            .unwrap_or(PathPromptResult::Cancelled),
        PathPromptResult::Cancelled => PathPromptResult::Cancelled,
        PathPromptResult::Failed(failure) => PathPromptResult::Failed(failure),
    }
}

fn resolve_path_prompt<T>(
    workspace: &mut Workspace,
    result: PathPromptResult<T>,
    cancelled: UiMessage,
    operation: &'static str,
) -> Option<T> {
    match result {
        PathPromptResult::Selected(selected) => Some(selected),
        PathPromptResult::Cancelled => {
            workspace.set_status(cancelled);
            None
        }
        PathPromptResult::Failed(failure) => {
            log::error!("{operation} path prompt failed: {failure:?}");
            workspace.set_status(UiMessage::Error(ErrorKind::Io));
            None
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum EstimateState {
    #[default]
    Unavailable,
    Pending,
    Ready(usize),
    Failed,
}

pub struct AppShell {
    active: Section,
    open: Option<Feature>,
    settings_open: bool,
    config: AppConfig,
    config_store: Option<ConfigStore>,
    workspace: Workspace,
    params: FeatureParams,
    crop_selection: Entity<Selection>,
    poster_layout: Entity<PosterLayout>,
    qr_input: Entity<InputState>,
    watermark_input: Entity<InputState>,
    ai_prompt: Entity<InputState>,
    poster_title: Entity<InputState>,
    poster_subtitle: Entity<InputState>,
    poster_corner_label: Entity<InputState>,
    nav_search: Entity<InputState>,
    seedream_key: Entity<InputState>,
    nano_banana_key: Entity<InputState>,
    openai_key: Entity<InputState>,
    ai_state: AiUiState,
    provider_registry: ProviderRegistry,
    credential_store: Arc<dyn CredentialStore>,
    /// Windows / Linux 的窗口内菜单栏；macOS 由系统原生菜单栏呈现同一份 `set_menus` 数据。
    #[cfg(not(target_os = "macos"))]
    menu_bar: Entity<MenuBar>,
    export_format_select: Entity<SelectState<Vec<SharedString>>>,
    quality_slider: Entity<SliderState>,
    batch_quality_slider: Entity<SliderState>,
    qr_foreground: Entity<ColorPickerState>,
    qr_background: Entity<ColorPickerState>,
    ai_custom_color: Entity<ColorPickerState>,
    estimate_state: EstimateState,
    estimate_generation: Arc<AtomicU64>,
    /// 上一帧窗口客户区尺寸（逻辑像素）。渲染时更新，供预览画布随窗口伸缩计算可用空间。
    viewport_width: f32,
    viewport_height: f32,
    /// 侧边栏中已收起的导航分组，用分组的 `label_key` 标识；默认全部展开（空集）。
    collapsed_groups: std::collections::HashSet<&'static str>,
    _subscriptions: Vec<Subscription>,
}

impl AppShell {
    pub fn new(
        config: AppConfig,
        config_store: Option<ConfigStore>,
        config_warning: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut workspace = Workspace::default();
        if let Some(warning) = config_warning {
            log::error!("configuration reset to defaults: {warning}");
            workspace.set_status(UiMessage::ConfigReset);
        }
        let params = FeatureParams::default();
        let crop_selection = cx.new(|_| Selection::new(Some(params.edit.ratio())));
        #[cfg(not(target_os = "macos"))]
        let menu_bar = cx.new(MenuBar::new);
        let poster_layout = cx.new(|_| PosterLayout::default());
        let qr_input =
            cx.new(|cx| InputState::new(window, cx).placeholder(tr("placeholder.qr_text")));
        let watermark_input =
            cx.new(|cx| InputState::new(window, cx).placeholder(tr("placeholder.watermark_text")));
        let ai_prompt =
            cx.new(|cx| InputState::new(window, cx).placeholder(tr("ai.placeholder.prompt")));
        let poster_title =
            cx.new(|cx| InputState::new(window, cx).placeholder(tr("ai.placeholder.poster_title")));
        let poster_subtitle = cx.new(|cx| {
            InputState::new(window, cx).placeholder(tr("ai.placeholder.poster_subtitle"))
        });
        let poster_corner_label = cx
            .new(|cx| InputState::new(window, cx).placeholder(tr("ai.placeholder.poster_corner")));
        let nav_search =
            cx.new(|cx| InputState::new(window, cx).placeholder(tr("nav.search_placeholder")));
        let seedream_key = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(tr("ai.placeholder.api_key"))
                .masked(true)
        });
        let nano_banana_key = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(tr("ai.placeholder.api_key"))
                .masked(true)
        });
        let openai_key = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(tr("ai.placeholder.api_key"))
                .masked(true)
        });
        let configured_provider = provider_from_config(&config.default_provider);
        let export_format_select = cx.new(|cx| {
            SelectState::new(
                export_format_items(),
                Some(IndexPath::default().row(export_format_index(config.default_export_format))),
                window,
                cx,
            )
        });
        let quality_slider = cx.new(|_| {
            SliderState::new()
                .min(1.0)
                .max(100.0)
                .step(1.0)
                .default_value(f32::from(config.default_export_quality.get()))
        });
        let batch_quality_slider = cx.new(|_| {
            SliderState::new()
                .min(1.0)
                .max(100.0)
                .step(1.0)
                .default_value(f32::from(params.batch.quality.get()))
        });
        let qr_foreground = cx.new(|cx| ColorPickerState::new(window, cx).default_value(black()));
        let qr_background = cx.new(|cx| ColorPickerState::new(window, cx).default_value(white()));
        let ai_custom_color =
            cx.new(|cx| ColorPickerState::new(window, cx).default_value(cx.theme().blue));

        let _subscriptions = vec![
            cx.subscribe_in(&nav_search, window, {
                move |_, _, event: &InputEvent, _, cx| {
                    if matches!(event, InputEvent::Change) {
                        cx.notify();
                    }
                }
            }),
            cx.subscribe_in(
                &export_format_select,
                window,
                |this, _, event: &SelectEvent<Vec<SharedString>>, _, cx| {
                    if let SelectEvent::Confirm(Some(label)) = event {
                        this.config.default_export_format = export_format_from_label(label);
                        this.persist_config(true);
                        this.schedule_size_estimate(cx);
                        cx.notify();
                    }
                },
            ),
            cx.subscribe(&quality_slider, |this, _, event: &SliderEvent, cx| {
                let SliderEvent::Change(value) = event;
                let value = value.start().round().clamp(1.0, 100.0) as u8;
                this.config.default_export_quality =
                    Quality::new(value).expect("quality slider is clamped to 1..=100");
                this.persist_config(false);
                this.schedule_size_estimate(cx);
                cx.notify();
            }),
            cx.subscribe(&batch_quality_slider, |this, _, event: &SliderEvent, cx| {
                let SliderEvent::Change(value) = event;
                let value = value.start().round().clamp(1.0, 100.0) as u8;
                this.params.batch.quality =
                    Quality::new(value).expect("quality slider is clamped to 1..=100");
                cx.notify();
            }),
            cx.subscribe(&qr_foreground, |this, _, event: &ColorPickerEvent, cx| {
                let ColorPickerEvent::Change(color) = event;
                if let Some(color) = color {
                    this.params.qr.foreground = hsla_to_image_rgba(*color);
                    cx.notify();
                }
            }),
            cx.subscribe(&qr_background, |this, _, event: &ColorPickerEvent, cx| {
                let ColorPickerEvent::Change(color) = event;
                if let Some(color) = color {
                    this.params.qr.background = hsla_to_image_rgba(*color);
                    cx.notify();
                }
            }),
        ];
        Self {
            active: Section::BasicImage,
            open: None,
            settings_open: false,
            config,
            config_store,
            workspace,
            params,
            crop_selection,
            poster_layout,
            qr_input,
            watermark_input,
            ai_prompt,
            poster_title,
            poster_subtitle,
            poster_corner_label,
            nav_search,
            seedream_key,
            nano_banana_key,
            openai_key,
            ai_state: AiUiState::new(configured_provider),
            provider_registry: ProviderRegistry::default(),
            credential_store: Arc::new(SystemCredentialStore),
            #[cfg(not(target_os = "macos"))]
            menu_bar,
            export_format_select,
            quality_slider,
            batch_quality_slider,
            qr_foreground,
            qr_background,
            ai_custom_color,
            estimate_state: EstimateState::Unavailable,
            estimate_generation: Arc::new(AtomicU64::new(0)),
            // 初始沿用窗口请求尺寸；首帧渲染即被真实客户区尺寸覆盖。
            viewport_width: 1280.0,
            viewport_height: 800.0,
            collapsed_groups: std::collections::HashSet::new(),
            _subscriptions,
        }
    }

    pub fn open_initial_paths(&mut self, paths: Vec<std::path::PathBuf>, cx: &mut Context<Self>) {
        let supported_count = paths
            .iter()
            .filter(|path| is_supported_image_path(path))
            .count();
        let multiple = supported_count > 1;
        self.active = Section::BasicImage;
        self.open = Some(initial_paths_feature(supported_count));
        self.settings_open = false;
        if let Some(job) = self.workspace.prepare_paths(paths, multiple) {
            self.spawn_workspace_job(job, cx);
        } else {
            cx.notify();
        }
    }

    fn persist_config(&mut self, show_success: bool) {
        let Some(store) = &self.config_store else {
            self.workspace
                .set_status(UiMessage::Error(ErrorKind::Config));
            return;
        };
        match store.save(&self.config) {
            Ok(()) if show_success => self.workspace.set_status(UiMessage::SettingsSaved),
            Ok(()) => {}
            Err(error) => {
                log::error!("failed to save configuration: {error}");
                self.workspace
                    .set_status(UiMessage::Error(ErrorKind::Config));
            }
        }
    }

    fn toggle_language(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let language = match self.config.language {
            Language::ZhCn => Language::En,
            Language::En => Language::ZhCn,
        };
        self.set_language(language, window, cx);
    }

    fn set_language(&mut self, language: Language, window: &mut Window, cx: &mut Context<Self>) {
        self.config.language = language;
        gpui_component::set_locale(self.config.language.locale());
        // 菜单文案随语言重建：MenuBar 每次渲染都重读 get_menus()，此处只需更新数据。
        cx.set_menus(crate::menus::build_menus());
        self.nav_search.update(cx, |input, input_cx| {
            input.set_placeholder(tr("nav.search_placeholder"), window, input_cx);
        });
        self.qr_input.update(cx, |input, input_cx| {
            input.set_placeholder(tr("placeholder.qr_text"), window, input_cx);
        });
        self.watermark_input.update(cx, |input, input_cx| {
            input.set_placeholder(tr("placeholder.watermark_text"), window, input_cx);
        });
        self.ai_prompt.update(cx, |input, input_cx| {
            input.set_placeholder(tr("ai.placeholder.prompt"), window, input_cx);
        });
        self.poster_title.update(cx, |input, input_cx| {
            input.set_placeholder(tr("ai.placeholder.poster_title"), window, input_cx);
        });
        self.poster_subtitle.update(cx, |input, input_cx| {
            input.set_placeholder(tr("ai.placeholder.poster_subtitle"), window, input_cx);
        });
        self.poster_corner_label.update(cx, |input, input_cx| {
            input.set_placeholder(tr("ai.placeholder.poster_corner"), window, input_cx);
        });
        for input in [&self.seedream_key, &self.nano_banana_key, &self.openai_key] {
            input.update(cx, |input, input_cx| {
                input.set_placeholder(tr("ai.placeholder.api_key"), window, input_cx);
            });
        }
        self.persist_config(true);
        cx.notify();
    }

    fn cycle_png_compression(&mut self, cx: &mut Context<Self>) {
        self.config.default_png_compression =
            next_png_compression(self.config.default_png_compression);
        self.persist_config(true);
        self.schedule_size_estimate(cx);
        cx.notify();
    }

    fn schedule_size_estimate(&mut self, cx: &mut Context<Self>) {
        let Some(image) = self.workspace.image_for_estimate() else {
            self.estimate_state = EstimateState::Unavailable;
            return;
        };
        let generation = self
            .estimate_generation
            .fetch_add(1, Ordering::Relaxed)
            .wrapping_add(1);
        let generation_token = Arc::clone(&self.estimate_generation);
        let settings = default_export_settings(&self.config);
        self.estimate_state = EstimateState::Pending;

        let task = cx.background_executor().spawn(async move {
            Timer::after(Duration::from_millis(180)).await;
            if generation_token.load(Ordering::Relaxed) != generation {
                return None;
            }
            Some(format::encode(image.image(), settings).map(|bytes| bytes.len()))
        });
        cx.spawn(async move |this, cx| {
            let Some(result) = task.await else {
                return;
            };
            let _ = this.update(cx, |this, cx| {
                if this.estimate_generation.load(Ordering::Relaxed) != generation {
                    return;
                }
                this.estimate_state = match result {
                    Ok(bytes) => EstimateState::Ready(bytes),
                    Err(error) => {
                        log::error!("output size estimation failed: {error}");
                        EstimateState::Failed
                    }
                };
                cx.notify();
            });
        })
        .detach();
    }

    /// 切换某个导航分组的展开 / 收起状态。分组用其 `label_key` 标识。
    fn toggle_group(&mut self, label_key: &'static str, cx: &mut Context<Self>) {
        if !self.collapsed_groups.remove(label_key) {
            self.collapsed_groups.insert(label_key);
        }
        cx.notify();
    }

    fn sidebar_feature_button(&self, feature: Feature, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let active = !self.settings_open && self.open == Some(feature);
        let (badge, badge_color) = feature_badge(feature, cx);
        h_flex()
            .id(SharedString::from(format!("nav-{}", feature.id())))
            .h(px(32.0))
            .px_2()
            .gap_2()
            .rounded_md()
            .text_sm()
            .cursor_pointer()
            .text_color(if active {
                theme.sidebar_accent_foreground
            } else {
                theme.sidebar_foreground
            })
            .when(active, |this| this.bg(theme.sidebar_accent))
            .hover(|this| this.bg(theme.sidebar_accent))
            .child(Icon::new(feature_icon(feature)).small().flex_shrink_0())
            .child(
                div()
                    .flex_1()
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .child(tr(feature.name_key())),
            )
            .child(
                div()
                    .px_1()
                    .rounded_sm()
                    .text_size(px(10.0))
                    .text_color(badge_color)
                    .child(badge),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.open_feature(feature, cx);
            }))
    }

    fn sidebar_group(
        &self,
        group: NavigationGroup,
        query: &str,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let features = group
            .features
            .iter()
            .copied()
            .filter(|feature| feature_matches_search(*feature, query))
            .map(|feature| self.sidebar_feature_button(feature, cx).into_any_element())
            .collect::<Vec<_>>();
        if features.is_empty() {
            return None;
        }
        // 搜索时强制展开，方便看到所有匹配项；否则遵循用户的收起状态。
        let collapsed = query.is_empty() && self.collapsed_groups.contains(group.label_key);
        let label_key = group.label_key;
        let muted = cx.theme().muted_foreground;
        let foreground = cx.theme().foreground;
        Some(
            v_flex()
                .gap_0()
                .child(
                    h_flex()
                        .id(SharedString::from(format!("nav-group-{label_key}")))
                        .h(px(30.0))
                        .px_2()
                        .justify_between()
                        .text_xs()
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(muted)
                        .cursor_pointer()
                        .hover(|this| this.text_color(foreground))
                        .child(
                            h_flex()
                                .gap_2()
                                .child(Icon::new(group.icon).small())
                                .child(tr(label_key)),
                        )
                        .child(Icon::new(if collapsed {
                            IconName::ChevronRight
                        } else {
                            IconName::ChevronDown
                        }))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.toggle_group(label_key, cx);
                        })),
                )
                .when(!collapsed, |this| this.children(features))
                .into_any_element(),
        )
    }

    fn open_feature(&mut self, feature: Feature, cx: &mut Context<Self>) {
        self.open = Some(feature);
        self.settings_open = false;
        self.active = section_for_feature(feature);
        if let Some(spec) = feature.ai_spec() {
            self.ai_state.aspect_ratio = spec.default_ratio;
            self.ai_state.selected_tiers.clear();
            self.ai_state.selected_tiers.insert(0);
        }
        if feature == Feature::ImageEdit {
            self.crop_selection.update(cx, |selection, selection_cx| {
                selection.set_ratio(None, selection_cx);
            });
        }
        cx.notify();
    }

    /// 启动后的空工作台：一整块可拖放的画布。拖入或打开图片后进入编辑器。
    /// 取代原先罗列全部工具的门户首页——导航的唯一入口是左侧边栏。
    fn workspace_home(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let border = cx.theme().border;
        let muted = cx.theme().muted_foreground;
        let secondary = cx.theme().secondary;
        let sidebar_accent = cx.theme().sidebar_accent;
        let sidebar_accent_foreground = cx.theme().sidebar_accent_foreground;
        let success = cx.theme().success;
        let status = self.workspace.status_text();

        v_flex()
            .id("workspace-home")
            .size_full()
            .p_6()
            .on_drop(cx.listener(|this, paths: &ExternalPaths, _, cx| {
                this.open = Some(Feature::Edit);
                this.active = Section::BasicImage;
                this.handle_drop(paths.paths().to_vec(), cx);
            }))
            .child(
                v_flex()
                    .flex_1()
                    .min_h(px(0.0))
                    .items_center()
                    .justify_center()
                    .gap_5()
                    .rounded_lg()
                    .border_1()
                    .border_dashed()
                    .border_color(border)
                    .bg(secondary)
                    .child(
                        div()
                            .size(px(64.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_lg()
                            .bg(sidebar_accent)
                            .text_color(sidebar_accent_foreground)
                            .child(Icon::new(IconName::GalleryVerticalEnd).size_6()),
                    )
                    .child(
                        div()
                            .text_xl()
                            .font_weight(gpui::FontWeight::BOLD)
                            .child(tr("workspace.empty_title")),
                    )
                    .child(
                        div()
                            .max_w(px(460.0))
                            .text_center()
                            .text_sm()
                            .text_color(muted)
                            .child(tr("workspace.empty_desc")),
                    )
                    .child(
                        Button::new("home-open-image")
                            .primary()
                            .icon(IconName::FolderOpen)
                            .label(tr("action.open"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.open = Some(Feature::Edit);
                                this.active = Section::BasicImage;
                                this.prompt_for_images(false, cx);
                            })),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .text_xs()
                            .text_color(muted)
                            .child(Icon::new(IconName::CircleCheck).small().text_color(success))
                            .child(tr("home.privacy_note")),
                    )
                    .when(!status.is_empty(), |this| {
                        this.child(div().text_xs().text_color(muted).child(status))
                    }),
            )
    }

    fn feature_page(&self, feature: Feature, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let foreground = cx.theme().foreground;
        let border = cx.theme().border;
        let sidebar_accent = cx.theme().sidebar_accent;
        let sidebar_accent_foreground = cx.theme().sidebar_accent_foreground;
        let success = cx.theme().success;

        let body = if feature.is_v1() {
            self.v1_page(feature, cx).into_any_element()
        } else if feature.is_ai() {
            self.ai_page(feature, cx).into_any_element()
        } else {
            div()
                .text_color(muted)
                .child(tr("placeholder.dev"))
                .into_any_element()
        };

        v_flex()
            .size_full()
            .overflow_hidden()
            .child(
                h_flex()
                    .min_h(px(88.0))
                    .px_7()
                    .py_4()
                    .justify_between()
                    .border_b_1()
                    .border_color(border)
                    .child(
                        h_flex()
                            .gap_3()
                            .child(
                                div()
                                    .size(px(42.0))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded_lg()
                                    .bg(sidebar_accent)
                                    .text_color(sidebar_accent_foreground)
                                    .child(Icon::new(feature_icon(feature)).size_6()),
                            )
                            .child(
                                v_flex()
                                    .gap_1()
                                    .child(
                                        h_flex()
                                            .gap_2()
                                            .child(
                                                div()
                                                    .text_lg()
                                                    .font_weight(gpui::FontWeight::BOLD)
                                                    .text_color(foreground)
                                                    .child(tr(feature.name_key())),
                                            )
                                            .child(feature_header_badge(feature, cx)),
                                    )
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(muted)
                                            .child(tr(feature.desc_key())),
                                    ),
                            ),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .px_3()
                            .py_1()
                            .rounded_md()
                            .border_1()
                            .border_color(border)
                            .text_xs()
                            .text_color(if feature.is_v1() { success } else { muted })
                            .child(Icon::new(if feature.is_v1() {
                                IconName::CircleCheck
                            } else {
                                IconName::Globe
                            }))
                            .child(tr(if feature.is_v1() {
                                "home.offline_available"
                            } else {
                                "home.provider_required"
                            })),
                    ),
            )
            .child(div().flex_1().overflow_hidden().p_6().child(body))
    }

    fn workspace_button(
        &self,
        id: &'static str,
        label_key: &str,
        command: WorkspaceCommand,
        primary: bool,
        cx: &mut Context<Self>,
    ) -> Button {
        let button = Button::new(id)
            .label(tr(label_key))
            .disabled(self.workspace.is_busy());
        let button = if primary {
            button.primary()
        } else {
            button.outline()
        };
        button.on_click(cx.listener(move |this, _, _, cx| {
            this.start_workspace_command(command.clone(), cx);
        }))
    }

    fn clipboard_button(
        &self,
        id: &'static str,
        label_key: &str,
        paste: bool,
        cx: &mut Context<Self>,
    ) -> Button {
        Button::new(id)
            .label(tr(label_key))
            .outline()
            .disabled(self.workspace.is_busy())
            .on_click(cx.listener(move |this, _, _, cx| {
                if paste {
                    this.paste_image(cx);
                } else {
                    this.copy_result(cx);
                }
            }))
    }

    fn start_workspace_command(&mut self, command: WorkspaceCommand, cx: &mut Context<Self>) {
        match command.route() {
            WorkspaceCommandRoute::OpenImages { multiple } => {
                self.prompt_for_images(multiple, cx);
            }
            WorkspaceCommandRoute::SaveResult => {
                self.prompt_for_save(default_export_settings(&self.config), cx);
            }
            WorkspaceCommandRoute::OutputDirectory(command) => {
                self.prompt_for_output_directory(command, cx);
            }
            WorkspaceCommandRoute::Direct(operation) => {
                if let Some(job) = self.workspace.prepare(operation) {
                    self.spawn_workspace_job(job, cx);
                } else {
                    cx.notify();
                }
            }
        }
    }

    /// 使用 GPUI 自带的异步平台对话框选择图片。
    ///
    /// 同步文件对话框会在 Windows 上启动嵌套消息循环；若它仍处于 `cx.listener`
    /// 对 `AppShell` 的可变借用期间，重入的 GPUI 事件就会触发 `RefCell already
    /// borrowed`。GPUI 的路径 prompt 会先返回 receiver，等当前事件回调释放借用后才
    /// 展示系统对话框。
    fn prompt_for_images(&mut self, multiple: bool, cx: &mut Context<Self>) {
        if self.workspace.is_busy() {
            self.workspace.set_status(UiMessage::Busy);
            cx.notify();
            return;
        }

        let paths_receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple,
            prompt: Some(tr(if multiple {
                "action.open_multi"
            } else {
                "action.open"
            })),
        });

        cx.spawn(async move |this, cx| {
            let result = classify_path_prompt_result(paths_receiver.await);
            let _ = this.update(cx, |this, cx| {
                let Some(paths) = resolve_path_prompt(
                    &mut this.workspace,
                    result,
                    UiMessage::CancelledOpen,
                    "image",
                ) else {
                    cx.notify();
                    return;
                };
                if let Some(job) = this.workspace.prepare_paths(paths, multiple) {
                    this.spawn_workspace_job(job, cx);
                } else {
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn prompt_for_save(&mut self, export: EncodeSettings, cx: &mut Context<Self>) {
        if self.workspace.is_busy() {
            self.workspace.set_status(UiMessage::Busy);
            cx.notify();
            return;
        }

        let directory = self.path_prompt_directory();
        let suggested_name = self.workspace.suggested_save_name(export);
        let path_receiver = cx.prompt_for_new_path(&directory, Some(&suggested_name));

        cx.spawn(async move |this, cx| {
            let result = classify_path_prompt_result(path_receiver.await);
            let _ = this.update(cx, |this, cx| {
                let Some(path) = resolve_path_prompt(
                    &mut this.workspace,
                    result,
                    UiMessage::CancelledSave,
                    "save",
                ) else {
                    cx.notify();
                    return;
                };
                if let Some(job) = this.workspace.prepare_save_path(path, export) {
                    this.spawn_workspace_job(job, cx);
                } else {
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn prompt_for_output_directory(
        &mut self,
        command: OutputDirectoryCommand,
        cx: &mut Context<Self>,
    ) {
        if self.workspace.is_busy() {
            self.workspace.set_status(UiMessage::Busy);
            cx.notify();
            return;
        }

        let paths_receiver = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(tr("action.save")),
        });

        cx.spawn(async move |this, cx| {
            let result =
                first_path_prompt_result(classify_path_prompt_result(paths_receiver.await));
            let _ = this.update(cx, |this, cx| {
                let Some(directory) = resolve_path_prompt(
                    &mut this.workspace,
                    result,
                    UiMessage::CancelledSave,
                    "output directory",
                ) else {
                    cx.notify();
                    return;
                };
                match command {
                    OutputDirectoryCommand::Slice(grid) => {
                        if let Some(job) = this.workspace.prepare_slice_directory(grid, directory) {
                            this.spawn_workspace_job(job, cx);
                        } else {
                            cx.notify();
                        }
                    }
                    OutputDirectoryCommand::Batch(request)
                        if matches!(
                            request,
                            BatchRequest::Watermark {
                                source: WatermarkSource::Image,
                                ..
                            }
                        ) =>
                    {
                        this.prompt_for_watermark_image(request, directory, cx);
                    }
                    OutputDirectoryCommand::Batch(request) => {
                        if let Some(job) =
                            this.workspace.prepare_batch_paths(request, directory, None)
                        {
                            this.spawn_workspace_job(job, cx);
                        } else {
                            cx.notify();
                        }
                    }
                }
            });
        })
        .detach();
    }

    fn prompt_for_watermark_image(
        &mut self,
        request: BatchRequest,
        directory: PathBuf,
        cx: &mut Context<Self>,
    ) {
        let paths_receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(tr("action.open")),
        });

        cx.spawn(async move |this, cx| {
            let result =
                first_path_prompt_result(classify_path_prompt_result(paths_receiver.await));
            let _ = this.update(cx, |this, cx| {
                let Some(path) = resolve_path_prompt(
                    &mut this.workspace,
                    result,
                    UiMessage::CancelledOpen,
                    "watermark image",
                ) else {
                    cx.notify();
                    return;
                };
                if let Some(job) =
                    this.workspace
                        .prepare_batch_paths(request, directory, Some(path))
                {
                    this.spawn_workspace_job(job, cx);
                } else {
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn path_prompt_directory(&self) -> PathBuf {
        self.config
            .last_output_dir
            .clone()
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("."))
    }

    fn paste_image(&mut self, cx: &mut Context<Self>) {
        let bytes = cx.read_from_clipboard().and_then(|item| {
            item.entries().iter().find_map(|entry| match entry {
                ClipboardEntry::Image(image) => Some(image.bytes.clone()),
                ClipboardEntry::String(_) => None,
            })
        });
        let Some(bytes) = bytes else {
            self.workspace.set_status(UiMessage::ClipboardHasNoImage);
            cx.notify();
            return;
        };
        if let Some(job) = self.workspace.prepare_paste(bytes) {
            self.spawn_workspace_job(job, cx);
        }
    }

    fn copy_result(&mut self, cx: &mut Context<Self>) {
        if let Some(job) = self.workspace.prepare_copy() {
            self.spawn_workspace_job(job, cx);
        } else {
            cx.notify();
        }
    }

    fn spawn_workspace_job(&mut self, job: WorkspaceJob, cx: &mut Context<Self>) {
        let (sender, receiver) = async_channel::unbounded();
        cx.background_executor()
            .spawn(async move {
                let outcome = job.run_with_progress(|completed, total| {
                    let _ = sender.try_send(WorkspaceEvent::Progress { completed, total });
                });
                let _ = sender
                    .send(WorkspaceEvent::Finished(Box::new(outcome)))
                    .await;
            })
            .detach();
        cx.spawn(async move |this, cx| {
            while let Ok(event) = receiver.recv().await {
                let finished = matches!(&event, WorkspaceEvent::Finished(_));
                if this
                    .update(cx, |this, cx| match event {
                        WorkspaceEvent::Progress { completed, total } => this
                            .workspace
                            .set_status(UiMessage::BatchProgress { completed, total }),
                        WorkspaceEvent::Finished(outcome) => {
                            this.finish_workspace_job(*outcome, cx);
                        }
                    })
                    .is_err()
                {
                    break;
                }
                if finished {
                    break;
                }
            }
        })
        .detach();
        cx.notify();
    }

    fn finish_workspace_job(&mut self, outcome: WorkspaceOutcome, cx: &mut Context<Self>) {
        let effects = self.workspace.apply(outcome);
        if let Some(directory) = effects.last_output_dir {
            self.config.last_output_dir = Some(directory);
            self.persist_config(false);
        }
        if let Some(payload) = effects.clipboard {
            self.write_clipboard(payload, cx);
        }
        self.schedule_size_estimate(cx);
        cx.notify();
    }

    fn write_clipboard(&mut self, payload: ClipboardPayload, cx: &mut Context<Self>) {
        let format = match payload.format {
            ClipboardFormat::Png => ClipboardImageFormat::Png,
            ClipboardFormat::Jpeg => ClipboardImageFormat::Jpeg,
            ClipboardFormat::Webp => ClipboardImageFormat::Webp,
            ClipboardFormat::Gif => ClipboardImageFormat::Gif,
            ClipboardFormat::Bmp => ClipboardImageFormat::Bmp,
        };
        let image = ClipboardImage::from_bytes(format, payload.bytes);
        cx.write_to_clipboard(ClipboardItem::new_image(&image));
    }

    fn open_output_directory(&mut self, cx: &mut Context<Self>) {
        let Some(directory) = &self.config.last_output_dir else {
            self.workspace.set_status(UiMessage::CancelledOpen);
            cx.notify();
            return;
        };
        if let Err(error) = reveal_directory(directory) {
            log::error!(
                "failed to open output directory {}: {error}",
                directory.display()
            );
            self.workspace.set_status(UiMessage::Error(ErrorKind::Io));
        }
        cx.notify();
    }

    /// 菜单栏槽位：Windows / Linux 由 MenuBar 渲染 `set_menus`；macOS 交给系统菜单栏。
    fn menu_bar_slot(&self) -> AnyElement {
        #[cfg(not(target_os = "macos"))]
        return self.menu_bar.clone().into_any_element();
        #[cfg(target_os = "macos")]
        return div().into_any_element();
    }

    // —— 菜单动作处理器（由 `crate::menus::register_actions` 经全局动作分发调用）——

    pub(crate) fn menu_open_images(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.open_images_via_menu(false, cx);
    }

    pub(crate) fn menu_open_multiple_images(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.open_images_via_menu(true, cx);
    }

    /// 各功能页共享同一工作区：已在功能页时原地加载；在首页 / 设置页则先落到对应工作区。
    fn open_images_via_menu(&mut self, multiple: bool, cx: &mut Context<Self>) {
        if self.settings_open || self.open.is_none() {
            let landing = if multiple { Feature::Batch } else { Feature::Edit };
            self.open_feature(landing, cx);
        }
        self.prompt_for_images(multiple, cx);
    }

    pub(crate) fn menu_save_result(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if !self.workspace.has_image() {
            self.workspace.set_status(UiMessage::NeedResult);
            cx.notify();
            return;
        }
        self.prompt_for_save(default_export_settings(&self.config), cx);
    }

    pub(crate) fn menu_reveal_output_directory(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.open_output_directory(cx);
    }

    pub(crate) fn menu_open_settings(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.settings_open = true;
        self.open = None;
        cx.notify();
    }

    pub(crate) fn menu_quit(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        cx.quit();
    }

    pub(crate) fn menu_paste_image(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.settings_open || self.open.is_none() {
            self.open_feature(Feature::Edit, cx);
        }
        self.paste_image(cx);
    }

    pub(crate) fn menu_copy_result(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.copy_result(cx);
    }

    pub(crate) fn menu_go_home(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.open = None;
        self.settings_open = false;
        cx.notify();
    }

    pub(crate) fn menu_switch_language(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.toggle_language(window, cx);
    }

    pub(crate) fn menu_show_about(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        window.open_dialog(cx, |dialog, _, _| {
            dialog.title(tr("menu.about")).child(
                v_flex()
                    .gap_2()
                    .child(
                        div()
                            .text_lg()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(tr("app.title")),
                    )
                    .child(
                        div().text_sm().child(format!(
                            "{} {}",
                            tr("menu.about_version"),
                            env!("CARGO_PKG_VERSION")
                        )),
                    )
                    .child(div().text_sm().child(tr("app.subtitle"))),
            )
        });
    }

    fn adjust_param(&mut self, action: ParamAction, cx: &mut Context<Self>) {
        match self.params.apply(action) {
            ParamEffect::CropRatioChanged => {
                let ratio = self.params.edit.ratio();
                self.crop_selection.update(cx, |selection, selection_cx| {
                    selection.set_ratio(Some(ratio), selection_cx);
                });
            }
            ParamEffect::BeautifyPreview
                if self.workspace.has_image() && !self.workspace.is_busy() =>
            {
                self.start_workspace_command(
                    WorkspaceCommand::Transform(TransformOperation::Beautify(
                        self.params.beautify.core(),
                    )),
                    cx,
                );
            }
            ParamEffect::None | ParamEffect::BeautifyPreview => {}
        }
        cx.notify();
    }

    fn apply_crop(&mut self, cx: &mut Context<Self>) {
        let Some((width, height)) = self.workspace.current_dimensions() else {
            self.workspace.set_status(UiMessage::NeedImage);
            cx.notify();
            return;
        };
        let crop = self.crop_selection.read(cx).to_crop_rect(width, height);
        match crop {
            Ok(rect) => self.start_workspace_command(
                WorkspaceCommand::Transform(TransformOperation::Crop(rect)),
                cx,
            ),
            Err(error) => {
                log::error!("invalid crop selection: {error}");
                self.workspace
                    .set_status(UiMessage::Error(ErrorKind::Operation));
                cx.notify();
            }
        }
    }

    fn generate_qr(&mut self, cx: &mut Context<Self>) {
        let text = self.qr_input.read(cx).value().trim().to_string();
        if text.is_empty() {
            self.workspace.set_status(UiMessage::NeedText);
            cx.notify();
            return;
        }
        self.start_workspace_command(
            WorkspaceCommand::GenerateQr {
                text,
                options: self.params.qr.options(),
            },
            cx,
        );
    }

    fn run_batch(&mut self, cx: &mut Context<Self>) {
        let params = self.params.batch;
        let request = match params.mode {
            BatchMode::Convert | BatchMode::Compress => BatchRequest::Convert {
                format: params.format,
                settings: params.encode_settings(),
            },
            BatchMode::Resize => BatchRequest::Resize {
                width: params.width,
                height: params.height,
            },
            BatchMode::Watermark => {
                let text = self.watermark_input.read(cx).value().trim().to_string();
                if params.watermark_source == WatermarkSource::Text && text.is_empty() {
                    self.workspace.set_status(UiMessage::NeedText);
                    cx.notify();
                    return;
                }
                BatchRequest::Watermark {
                    source: params.watermark_source,
                    text,
                    opacity: f32::from(params.watermark_opacity_percent) / 100.0,
                    placement: params.watermark_placement,
                }
            }
        };
        self.start_workspace_command(WorkspaceCommand::Batch(request), cx);
    }

    fn handle_drop(&mut self, paths: Vec<std::path::PathBuf>, cx: &mut Context<Self>) {
        let multiple = matches!(
            self.open,
            Some(Feature::Collage | Feature::Batch | Feature::Gif)
        );
        if let Some(job) = self.workspace.prepare_paths(paths, multiple) {
            self.spawn_workspace_job(job, cx);
        } else {
            cx.notify();
        }
    }

    fn cycle_control(
        &self,
        label_key: &str,
        value: SharedString,
        button_id: &'static str,
        action: ParamAction,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let help = tr(parameter_help_key(label_key));
        h_flex()
            .justify_between()
            .gap_3()
            .child(div().text_sm().child(tr(label_key)))
            .child(
                Button::new(button_id)
                    .outline()
                    .label(value)
                    .tooltip(help)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.adjust_param(action, cx);
                    })),
            )
            .into_any_element()
    }

    fn step_control(
        &self,
        label_key: &str,
        value: SharedString,
        ids: (&'static str, &'static str),
        actions: (ParamAction, ParamAction),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let help = tr(parameter_help_key(label_key));
        h_flex()
            .justify_between()
            .gap_3()
            .child(div().text_sm().child(tr(label_key)))
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        Button::new(ids.0)
                            .outline()
                            .icon(IconName::Minus)
                            .tooltip(help.clone())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.adjust_param(actions.0, cx);
                            })),
                    )
                    .child(div().min_w(px(72.0)).text_center().child(value))
                    .child(
                        Button::new(ids.1)
                            .outline()
                            .icon(IconName::Plus)
                            .tooltip(help)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.adjust_param(actions.1, cx);
                            })),
                    ),
            )
            .into_any_element()
    }

    fn slider_control(
        &self,
        label_key: &str,
        value: SharedString,
        slider: &Entity<SliderState>,
        id: &'static str,
    ) -> AnyElement {
        let help = tr(parameter_help_key(label_key));
        h_flex()
            .justify_between()
            .gap_3()
            .child(div().text_sm().child(tr(label_key)))
            .child(
                v_flex()
                    .w(px(180.0))
                    .gap_1()
                    .child(div().text_sm().text_center().child(value))
                    .child(
                        div()
                            .id(id)
                            .tooltip(move |window, cx| Tooltip::new(help.clone()).build(window, cx))
                            .child(Slider::new(slider).w_full()),
                    ),
            )
            .into_any_element()
    }

    fn parameter_panel(&self, feature: Feature, cx: &mut Context<Self>) -> AnyElement {
        let mut controls = Vec::new();
        match feature {
            Feature::Edit => {
                controls.push(self.cycle_control(
                    "parameter.aspect_ratio",
                    self.params.edit.ratio().to_string().into(),
                    "param-edit-ratio",
                    ParamAction::EditRatioNext,
                    cx,
                ));
                controls.push(self.step_control(
                    "parameter.output_width",
                    format!("{} px", self.params.edit.width).into(),
                    ("param-edit-width-down", "param-edit-width-up"),
                    (ParamAction::EditWidth(-1), ParamAction::EditWidth(1)),
                    cx,
                ));
                controls.push(self.step_control(
                    "parameter.output_height",
                    format!("{} px", self.params.edit.height).into(),
                    ("param-edit-height-down", "param-edit-height-up"),
                    (ParamAction::EditHeight(-1), ParamAction::EditHeight(1)),
                    cx,
                ));
            }
            Feature::Collage => {
                controls.push(self.cycle_control(
                    "parameter.layout",
                    tr(self.params.collage.mode.label_key()),
                    "param-collage-layout",
                    ParamAction::CollageLayoutNext,
                    cx,
                ));
                controls.push(self.step_control(
                    "parameter.columns",
                    self.params.collage.columns.to_string().into(),
                    ("param-collage-columns-down", "param-collage-columns-up"),
                    (
                        ParamAction::CollageColumns(-1),
                        ParamAction::CollageColumns(1),
                    ),
                    cx,
                ));
                controls.push(self.step_control(
                    "parameter.spacing",
                    format!("{} px", self.params.collage.spacing).into(),
                    ("param-collage-spacing-down", "param-collage-spacing-up"),
                    (
                        ParamAction::CollageSpacing(-1),
                        ParamAction::CollageSpacing(1),
                    ),
                    cx,
                ));
                controls.push(self.cycle_control(
                    "parameter.background",
                    tr(self.params.collage.background_label_key()),
                    "param-collage-background",
                    ParamAction::CollageBackgroundNext,
                    cx,
                ));
            }
            Feature::Batch => self.batch_controls(&mut controls, cx),
            Feature::Slice => {
                controls.push(self.step_control(
                    "parameter.rows",
                    self.params.slice.rows.to_string().into(),
                    ("param-slice-rows-down", "param-slice-rows-up"),
                    (ParamAction::SliceRows(-1), ParamAction::SliceRows(1)),
                    cx,
                ));
                controls.push(self.step_control(
                    "parameter.columns",
                    self.params.slice.columns.to_string().into(),
                    ("param-slice-columns-down", "param-slice-columns-up"),
                    (ParamAction::SliceColumns(-1), ParamAction::SliceColumns(1)),
                    cx,
                ));
            }
            Feature::QrCode => {
                controls.push(
                    v_flex()
                        .gap_1()
                        .child(div().text_sm().child(tr("parameter.qr_text")))
                        .child(Input::new(&self.qr_input).cleanable(true))
                        .into_any_element(),
                );
                controls.push(self.step_control(
                    "parameter.qr_size",
                    format!("{} px", self.params.qr.size).into(),
                    ("param-qr-size-down", "param-qr-size-up"),
                    (ParamAction::QrSize(-1), ParamAction::QrSize(1)),
                    cx,
                ));
                controls.push(self.cycle_control(
                    "parameter.correction",
                    tr(self.params.qr.correction_label_key()),
                    "param-qr-correction",
                    ParamAction::QrCorrectionNext,
                    cx,
                ));
                let foreground_help = tr("help.qr_foreground");
                let background_help = tr("help.qr_background");
                controls.push(
                    h_flex()
                        .justify_between()
                        .gap_3()
                        .child(div().text_sm().child(tr("parameter.qr_colors")))
                        .child(
                            h_flex()
                                .gap_3()
                                .child(
                                    v_flex()
                                        .gap_1()
                                        .child(div().text_xs().child(tr("parameter.qr_foreground")))
                                        .child(
                                            div()
                                                .id("param-qr-foreground")
                                                .tooltip(move |window, cx| {
                                                    Tooltip::new(foreground_help.clone())
                                                        .build(window, cx)
                                                })
                                                .child(ColorPicker::new(&self.qr_foreground)),
                                        ),
                                )
                                .child(
                                    v_flex()
                                        .gap_1()
                                        .child(div().text_xs().child(tr("parameter.qr_background")))
                                        .child(
                                            div()
                                                .id("param-qr-background")
                                                .tooltip(move |window, cx| {
                                                    Tooltip::new(background_help.clone())
                                                        .build(window, cx)
                                                })
                                                .child(ColorPicker::new(&self.qr_background)),
                                        ),
                                ),
                        )
                        .into_any_element(),
                );
            }
            Feature::Beautify => self.beautify_controls(&mut controls, cx),
            Feature::Gif => self.gif_controls(&mut controls, cx),
            _ => {}
        }
        v_flex()
            .w_full()
            .gap_3()
            .p_4()
            .rounded_lg()
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().background)
            .child(
                div()
                    .text_sm()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(tr("workspace.parameters")),
            )
            .children(controls)
            .into_any_element()
    }

    fn batch_controls(&self, controls: &mut Vec<AnyElement>, cx: &mut Context<Self>) {
        let params = self.params.batch;
        controls.push(self.cycle_control(
            "parameter.mode",
            tr(params.mode.label_key()),
            "param-batch-mode",
            ParamAction::BatchModeNext,
            cx,
        ));
        match params.mode {
            BatchMode::Convert | BatchMode::Compress => {
                controls.push(self.cycle_control(
                    "parameter.format",
                    params.format.to_string().into(),
                    "param-batch-format",
                    ParamAction::BatchFormatNext,
                    cx,
                ));
                if params.format == OutputFormat::Png {
                    controls.push(self.cycle_control(
                        "parameter.png_compression",
                        tr(png_compression_label_key(params.png_compression)),
                        "param-batch-png-compression",
                        ParamAction::BatchPngCompressionNext,
                        cx,
                    ));
                } else {
                    controls.push(self.slider_control(
                        "parameter.quality",
                        format!("{}%", params.quality.get()).into(),
                        &self.batch_quality_slider,
                        "param-batch-quality-slider",
                    ));
                }
            }
            BatchMode::Resize => {
                controls.push(self.step_control(
                    "parameter.output_width",
                    format!("{} px", params.width).into(),
                    ("param-batch-width-down", "param-batch-width-up"),
                    (ParamAction::BatchWidth(-1), ParamAction::BatchWidth(1)),
                    cx,
                ));
                controls.push(self.step_control(
                    "parameter.output_height",
                    format!("{} px", params.height).into(),
                    ("param-batch-height-down", "param-batch-height-up"),
                    (ParamAction::BatchHeight(-1), ParamAction::BatchHeight(1)),
                    cx,
                ));
            }
            BatchMode::Watermark => {
                controls.push(self.cycle_control(
                    "parameter.watermark_source",
                    tr(params.watermark_source.label_key()),
                    "param-batch-watermark-source",
                    ParamAction::BatchWatermarkSourceNext,
                    cx,
                ));
                if params.watermark_source == WatermarkSource::Text {
                    controls.push(
                        v_flex()
                            .gap_1()
                            .child(div().text_sm().child(tr("parameter.watermark_text")))
                            .child(Input::new(&self.watermark_input).cleanable(true))
                            .into_any_element(),
                    );
                }
                controls.push(self.step_control(
                    "parameter.opacity",
                    format!("{}%", params.watermark_opacity_percent).into(),
                    ("param-batch-opacity-down", "param-batch-opacity-up"),
                    (ParamAction::BatchOpacity(-5), ParamAction::BatchOpacity(5)),
                    cx,
                ));
                controls.push(self.cycle_control(
                    "parameter.position",
                    tr(params.watermark_placement.label_key()),
                    "param-batch-position",
                    ParamAction::BatchPositionNext,
                    cx,
                ));
            }
        }
    }

    fn beautify_controls(&self, controls: &mut Vec<AnyElement>, cx: &mut Context<Self>) {
        let params = self.params.beautify;
        controls.push(self.step_control(
            "parameter.corner_radius",
            format!("{} px", params.radius).into(),
            ("param-beautify-radius-down", "param-beautify-radius-up"),
            (
                ParamAction::BeautifyRadius(-1),
                ParamAction::BeautifyRadius(1),
            ),
            cx,
        ));
        controls.push(self.step_control(
            "parameter.padding",
            format!("{} px", params.padding).into(),
            ("param-beautify-padding-down", "param-beautify-padding-up"),
            (
                ParamAction::BeautifyPadding(-1),
                ParamAction::BeautifyPadding(1),
            ),
            cx,
        ));
        controls.push(self.cycle_control(
            "parameter.background",
            tr(params.background.label_key()),
            "param-beautify-background",
            ParamAction::BeautifyBackgroundNext,
            cx,
        ));
        controls.push(self.step_control(
            "parameter.border",
            format!("{} px", params.border_width).into(),
            ("param-beautify-border-down", "param-beautify-border-up"),
            (
                ParamAction::BeautifyBorder(-1),
                ParamAction::BeautifyBorder(1),
            ),
            cx,
        ));
        controls.push(self.cycle_control(
            "parameter.shadow",
            tr(if params.shadow {
                "option.enabled"
            } else {
                "option.disabled"
            }),
            "param-beautify-shadow",
            ParamAction::BeautifyShadowToggle,
            cx,
        ));
    }

    fn gif_controls(&self, controls: &mut Vec<AnyElement>, cx: &mut Context<Self>) {
        let params = self.params.gif;
        controls.push(self.step_control(
            "parameter.frame_delay",
            format!("{} ms", params.delay_ms).into(),
            ("param-gif-delay-down", "param-gif-delay-up"),
            (ParamAction::GifDelay(-1), ParamAction::GifDelay(1)),
            cx,
        ));
        controls.push(self.cycle_control(
            "parameter.size_mode",
            tr(if params.custom_size {
                "option.custom_size"
            } else {
                "option.original_size"
            }),
            "param-gif-size-mode",
            ParamAction::GifSizeModeToggle,
            cx,
        ));
        if params.custom_size {
            controls.push(self.step_control(
                "parameter.output_width",
                format!("{} px", params.width).into(),
                ("param-gif-width-down", "param-gif-width-up"),
                (ParamAction::GifWidth(-1), ParamAction::GifWidth(1)),
                cx,
            ));
            controls.push(self.step_control(
                "parameter.output_height",
                format!("{} px", params.height).into(),
                ("param-gif-height-down", "param-gif-height-up"),
                (ParamAction::GifHeight(-1), ParamAction::GifHeight(1)),
                cx,
            ));
        }
        controls.push(self.cycle_control(
            "parameter.playback",
            tr(params.playback_label_key()),
            "param-gif-playback",
            ParamAction::GifPlaybackNext,
            cx,
        ));
    }

    fn feature_action_buttons(&self, feature: Feature, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let mut buttons = Vec::new();
        macro_rules! command {
            ($id:expr, $label:expr, $workspace_command:expr, $primary:expr $(,)?) => {
                buttons.push(
                    self.workspace_button($id, $label, $workspace_command, $primary, cx)
                        .into_any_element(),
                )
            };
        }
        match feature {
            Feature::Edit => {
                command!(
                    "act-open",
                    "action.open",
                    WorkspaceCommand::OpenSingle,
                    true,
                );
                command!(
                    "act-rotate",
                    "action.rotate90",
                    WorkspaceCommand::Transform(TransformOperation::Rotate(Rotation::Cw90)),
                    false,
                );
                buttons.push(
                    Button::new("act-crop")
                        .outline()
                        .label(tr("action.apply_crop"))
                        .disabled(self.workspace.is_busy())
                        .on_click(cx.listener(|this, _, _, cx| this.apply_crop(cx)))
                        .into_any_element(),
                );
                command!(
                    "act-resize",
                    "action.apply_resize",
                    WorkspaceCommand::Transform(TransformOperation::Resize {
                        width: self.params.edit.width,
                        height: self.params.edit.height,
                    }),
                    false,
                );
                command!(
                    "act-save",
                    "action.save",
                    WorkspaceCommand::SaveResult,
                    false,
                );
            }
            Feature::Collage => {
                command!(
                    "act-open",
                    "action.open_multi",
                    WorkspaceCommand::OpenMultiple,
                    true,
                );
                command!(
                    "act-collage",
                    "action.compose_collage",
                    WorkspaceCommand::Collage {
                        layout: self.params.collage.layout(),
                        options: self.params.collage.options(),
                    },
                    false,
                );
                command!(
                    "act-save",
                    "action.save",
                    WorkspaceCommand::SaveResult,
                    false,
                );
            }
            Feature::Batch => {
                command!(
                    "act-open",
                    "action.open_multi",
                    WorkspaceCommand::OpenMultiple,
                    true,
                );
                buttons.push(
                    Button::new("act-batch")
                        .outline()
                        .label(tr("action.run_batch"))
                        .disabled(self.workspace.is_busy())
                        .on_click(cx.listener(|this, _, _, cx| this.run_batch(cx)))
                        .into_any_element(),
                );
            }
            Feature::Slice => {
                command!(
                    "act-open",
                    "action.open",
                    WorkspaceCommand::OpenSingle,
                    true,
                );
                command!(
                    "act-slice",
                    "action.export_slices",
                    WorkspaceCommand::Slice(self.params.slice.grid()),
                    false,
                );
            }
            Feature::QrCode => {
                buttons.push(
                    Button::new("act-generate-qr")
                        .primary()
                        .label(tr("action.generate_qr"))
                        .disabled(self.workspace.is_busy())
                        .on_click(cx.listener(|this, _, _, cx| this.generate_qr(cx)))
                        .into_any_element(),
                );
                command!(
                    "act-open",
                    "action.open",
                    WorkspaceCommand::OpenSingle,
                    false,
                );
                command!(
                    "act-decode",
                    "action.decode_qr",
                    WorkspaceCommand::DecodeQr,
                    false,
                );
                command!(
                    "act-save",
                    "action.save",
                    WorkspaceCommand::SaveResult,
                    false,
                );
            }
            Feature::Exif => {
                command!(
                    "act-open",
                    "action.open",
                    WorkspaceCommand::OpenSingle,
                    true,
                );
                command!(
                    "act-read-exif",
                    "action.read_exif",
                    WorkspaceCommand::ReadExif,
                    false,
                );
                command!(
                    "act-strip",
                    "action.strip_exif",
                    WorkspaceCommand::StripExif,
                    false,
                );
                command!(
                    "act-save",
                    "action.save",
                    WorkspaceCommand::SaveResult,
                    false,
                );
            }
            Feature::Beautify => {
                command!(
                    "act-open",
                    "action.open",
                    WorkspaceCommand::OpenSingle,
                    true,
                );
                command!(
                    "act-beautify",
                    "action.beautify",
                    WorkspaceCommand::Transform(TransformOperation::Beautify(
                        self.params.beautify.core(),
                    )),
                    false,
                );
                command!(
                    "act-save",
                    "action.save",
                    WorkspaceCommand::SaveResult,
                    false,
                );
            }
            Feature::Gif => {
                command!(
                    "act-open",
                    "action.open_multi",
                    WorkspaceCommand::OpenMultiple,
                    true,
                );
                command!(
                    "act-gif",
                    "action.make_gif",
                    WorkspaceCommand::MakeGif(self.params.gif.core()),
                    false,
                );
                command!(
                    "act-save",
                    "action.save",
                    WorkspaceCommand::SaveResult,
                    false,
                );
            }
            _ => {}
        }
        buttons.push(
            self.clipboard_button("act-paste", "action.paste", true, cx)
                .into_any_element(),
        );
        buttons.push(
            self.clipboard_button("act-copy", "action.copy", false, cx)
                .into_any_element(),
        );
        if self.config.last_output_dir.is_some() {
            buttons.push(
                Button::new("act-open-output-directory")
                    .outline()
                    .label(tr("action.open_output_directory"))
                    .on_click(cx.listener(|this, _, _, cx| this.open_output_directory(cx)))
                    .into_any_element(),
            );
        }
        buttons
    }

    /// 预览画布可用的最大宽高（逻辑像素）：由窗口客户区减去侧栏、参数列、页头与各处
    /// 内边距等固定占用得到。让预览随窗口伸缩，同时保证不挤占右侧控制区（估算偏保守，
    /// 宁可略留白也不溢出，避免历史上出现过的裁剪问题）。
    fn preview_budget(&self) -> (f32, f32) {
        // 水平固定占用：侧栏 216 + 页面内边距 48 + 预览/参数间距 20 + 参数列 300
        //   + 画布内边距 32 + 安全余量 24。
        const HORIZONTAL_CHROME: f32 = 216.0 + 48.0 + 20.0 + 300.0 + 32.0 + 24.0;
        // 垂直固定占用：顶栏 40 + 功能页头 88 + 页面内边距 48 + 画布标题与信息行约 64
        //   + 画布内边距 32 + 安全余量 24。
        const VERTICAL_CHROME: f32 = 40.0 + 88.0 + 48.0 + 64.0 + 32.0 + 24.0;
        let width = (self.viewport_width - HORIZONTAL_CHROME).max(360.0);
        let height = (self.viewport_height - VERTICAL_CHROME).max(320.0);
        (width, height)
    }

    fn preview_panel(&self, feature: Feature, cx: &mut Context<Self>) -> Option<AnyElement> {
        let image = self.workspace.preview()?;
        let (max_width, max_height) = self.preview_budget();
        let (width, height) = self.workspace.preview_size(max_width, max_height)?;
        let preview = div()
            .relative()
            .w(px(width))
            .h(px(height))
            .child(img(image).size_full());
        let preview = if feature == Feature::Edit {
            let down = self.crop_selection.clone();
            let moving = self.crop_selection.clone();
            let up = self.crop_selection.clone();
            let up_out = self.crop_selection.clone();
            preview.child(
                div()
                    .id("crop-overlay")
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |_, event: &MouseDownEvent, _, cx| {
                            down.update(cx, |selection, selection_cx| {
                                selection.on_down(event.position, selection_cx);
                            });
                        }),
                    )
                    .on_mouse_move(cx.listener(move |_, event: &MouseMoveEvent, _, cx| {
                        moving.update(cx, |selection, selection_cx| {
                            selection.on_move(event.position, selection_cx);
                        });
                    }))
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(move |_, _: &MouseUpEvent, _, cx| {
                            up.update(cx, |selection, selection_cx| {
                                selection.on_up(selection_cx);
                            });
                        }),
                    )
                    .on_mouse_up_out(
                        MouseButton::Left,
                        cx.listener(move |_, _: &MouseUpEvent, _, cx| {
                            up_out.update(cx, |selection, selection_cx| {
                                selection.on_up(selection_cx);
                            });
                        }),
                    )
                    .child(selection_overlay(self.crop_selection.clone())),
            )
        } else {
            preview
        };
        Some(preview.into_any_element())
    }

    fn workspace_canvas(&self, feature: Feature, cx: &mut Context<Self>) -> AnyElement {
        let border = cx.theme().border;
        let background = cx.theme().background;
        let secondary = cx.theme().secondary;
        let muted = cx.theme().muted_foreground;
        let sidebar_accent = cx.theme().sidebar_accent;
        let sidebar_accent_foreground = cx.theme().sidebar_accent_foreground;
        let preview = self.preview_panel(feature, cx);
        let info = self.workspace.info_text();
        let status = self.workspace.status_text();
        v_flex()
            .flex_1()
            .min_h(px(520.0))
            .p_4()
            .gap_3()
            .rounded_lg()
            .border_1()
            .border_color(border)
            .bg(background)
            .child(
                h_flex()
                    .justify_between()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(tr("workspace.preview")),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(muted)
                            .child(feature_detail(feature)),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h(px(420.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .overflow_hidden()
                    .rounded_lg()
                    .border_1()
                    .border_dashed()
                    .border_color(border)
                    .bg(secondary)
                    .when_some(preview, |this, preview| this.child(preview))
                    .when(self.workspace.preview().is_none(), |this| {
                        this.child(
                            v_flex()
                                .items_center()
                                .gap_3()
                                .px_8()
                                .text_center()
                                .child(
                                    div()
                                        .size(px(52.0))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .rounded_lg()
                                        .bg(sidebar_accent)
                                        .text_color(sidebar_accent_foreground)
                                        .child(Icon::new(IconName::GalleryVerticalEnd).size_6()),
                                )
                                .child(
                                    div()
                                        .font_weight(gpui::FontWeight::SEMIBOLD)
                                        .child(tr("workspace.empty_title")),
                                )
                                .child(
                                    div()
                                        .max_w(px(420.0))
                                        .text_sm()
                                        .text_color(muted)
                                        .child(tr("workspace.empty_desc")),
                                ),
                        )
                    }),
            )
            .child(
                h_flex()
                    .min_h(px(24.0))
                    .justify_between()
                    .text_xs()
                    .text_color(muted)
                    .child(if info.is_empty() {
                        tr("workspace.ready")
                    } else {
                        info
                    })
                    .when(!status.is_empty(), |this| this.child(status)),
            )
            .into_any_element()
    }

    fn v1_page(&self, feature: Feature, cx: &mut Context<Self>) -> impl IntoElement {
        let primary = cx.theme().primary;
        let muted = cx.theme().muted_foreground;
        let border = cx.theme().border;
        let background = cx.theme().background;
        let buttons = self.feature_action_buttons(feature, cx);
        let status = self.workspace.status_text();

        h_flex()
            .id("v1-workspace")
            .size_full()
            .flex_1()
            .overflow_y_scroll()
            .pb_6()
            .gap_5()
            .items_start()
            .on_drop(cx.listener(|this, paths: &ExternalPaths, _, cx| {
                this.handle_drop(paths.paths().to_vec(), cx);
            }))
            .child(
                div()
                    .flex_1()
                    .min_w(px(420.0))
                    .child(self.workspace_canvas(feature, cx)),
            )
            .child(
                v_flex()
                    .w(px(280.0))
                    .flex_shrink_0()
                    .gap_4()
                    .child(self.parameter_panel(feature, cx))
                    .child(
                        v_flex()
                            .gap_2()
                            .p_4()
                            .rounded_lg()
                            .border_1()
                            .border_color(border)
                            .bg(background)
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .child(tr("workspace.actions")),
                            )
                            .children(buttons),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .text_xs()
                            .text_color(primary)
                            .child(Icon::new(IconName::CircleCheck).small())
                            .child(tr("home.offline_available")),
                    )
                    .when(!status.is_empty(), |this| {
                        this.child(div().text_sm().text_color(muted).child(status))
                    }),
            )
    }

    fn cycle_ai_provider(&mut self, cx: &mut Context<Self>) {
        self.ai_state.cycle_provider();
        if let Some(capabilities) = self.provider_registry.capabilities(self.ai_state.provider) {
            self.ai_state.apply_capabilities(capabilities);
        }
        self.config.default_provider = self.ai_state.provider.as_str().to_string();
        self.persist_config(false);
        cx.notify();
    }

    fn cycle_ai_ratio(&mut self, cx: &mut Context<Self>) {
        if let Some(capabilities) = self.provider_registry.capabilities(self.ai_state.provider) {
            self.ai_state.cycle_ratio(capabilities);
        }
        cx.notify();
    }

    fn cycle_ai_count(&mut self, cx: &mut Context<Self>) {
        let maximum = self
            .provider_registry
            .capabilities(self.ai_state.provider)
            .map_or(1, |capabilities| capabilities.max_generation_count);
        self.ai_state.count = if self.ai_state.count >= maximum {
            1
        } else {
            self.ai_state.count + 1
        };
        cx.notify();
    }

    fn cycle_ai_quality(&mut self, cx: &mut Context<Self>) {
        self.ai_state.cycle_quality();
        cx.notify();
    }

    fn cycle_edit_preset(&mut self, cx: &mut Context<Self>) {
        self.ai_state.edit_preset_index =
            (self.ai_state.edit_preset_index + 1) % AiEditPreset::ALL.len();
        cx.notify();
    }

    fn api_key_input(&self, provider: ProviderId) -> &Entity<InputState> {
        match provider {
            ProviderId::Seedream => &self.seedream_key,
            ProviderId::NanoBanana => &self.nano_banana_key,
            ProviderId::OpenAi => &self.openai_key,
        }
    }

    fn save_api_key(&mut self, provider: ProviderId, window: &mut Window, cx: &mut Context<Self>) {
        let value = self
            .api_key_input(provider)
            .read(cx)
            .value()
            .trim()
            .to_string();
        let result = ApiKey::new(value).and_then(|key| self.credential_store.set(provider, &key));
        match result {
            Ok(()) => {
                self.ai_state.status = AiStatus::Idle;
                self.api_key_input(provider).update(cx, |input, input_cx| {
                    input.set_value("", window, input_cx);
                });
                self.workspace.set_status(UiMessage::SettingsSaved);
            }
            Err(error) => self.ai_state.status = AiStatus::Error(error.into()),
        }
        cx.notify();
    }

    fn delete_api_key(&mut self, provider: ProviderId, cx: &mut Context<Self>) {
        self.ai_state.status = match self.credential_store.delete(provider) {
            Ok(()) => AiStatus::Idle,
            Err(error) => AiStatus::Error(error.into()),
        };
        cx.notify();
    }

    fn clear_ai_history(&mut self, cx: &mut Context<Self>) {
        self.config.generation_history.clear();
        self.persist_config(true);
        cx.notify();
    }

    fn ai_prompts(
        &self,
        feature: Feature,
        cx: &Context<Self>,
    ) -> Result<Vec<AiPromptTask>, AiErrorKind> {
        let mut instruction = self.ai_prompt.read(cx).value().trim().to_string();
        if feature == Feature::TextToImage && instruction.is_empty() {
            return Err(AiErrorKind::InvalidRequest);
        }
        if matches!(
            feature,
            Feature::CoverFactory | Feature::ArticleIllustration
        ) && instruction.is_empty()
        {
            return Err(AiErrorKind::InvalidRequest);
        }
        if feature == Feature::ImageEdit {
            let preset = AiEditPreset::ALL[self.ai_state.edit_preset_index];
            let prompt = rastery_presets::render_edit_prompt(preset, preset.id(), &instruction)
                .unwrap_or(instruction);
            if prompt.trim().is_empty() {
                return Err(AiErrorKind::InvalidRequest);
            }
            return Ok(vec![AiPromptTask {
                tier_id: preset.id().to_string(),
                prompt,
            }]);
        }
        if feature == Feature::Poster {
            let spec = feature.ai_spec().ok_or(AiErrorKind::Unsupported)?;
            let tier = self
                .ai_state
                .selected_tiers
                .iter()
                .next()
                .and_then(|index| spec.tiers.get(*index))
                .copied()
                .unwrap_or("tech-launch");
            return Ok(vec![AiPromptTask {
                tier_id: tier.to_string(),
                prompt: rastery_presets::render_poster_background_prompt(tier, &instruction),
            }]);
        }
        if let Some(tool) = industry_tool(feature) {
            if feature == Feature::PromotionalPoster {
                let title = self.poster_title.read(cx).value().trim().to_string();
                if !title.is_empty() {
                    instruction = format!("Main title: {title}. {instruction}");
                }
            }
            let spec = feature.ai_spec().ok_or(AiErrorKind::Unsupported)?;
            return Ok(self
                .ai_state
                .selected_tiers
                .iter()
                .filter_map(|index| spec.tiers.get(*index))
                .map(|tier| {
                    let instruction = if feature == Feature::ProductRecolor && *tier == "custom" {
                        let color = self
                            .ai_custom_color
                            .read(cx)
                            .value()
                            .map(|color| color.to_hex())
                            .unwrap_or_else(|| "#000000".into());
                        format!("Requested custom color: {color}. {instruction}")
                    } else {
                        instruction.clone()
                    };
                    AiPromptTask {
                        tier_id: (*tier).to_string(),
                        prompt: rastery_presets::render_industry_prompt(tool, tier, &instruction),
                    }
                })
                .collect());
        }
        Ok(vec![AiPromptTask {
            tier_id: "prompt".into(),
            prompt: instruction,
        }])
    }

    fn start_ai_generation(&mut self, feature: Feature, cx: &mut Context<Self>) {
        if matches!(self.ai_state.status, AiStatus::Generating) {
            return;
        }
        let Some(spec) = feature.ai_spec() else {
            self.ai_state.status = AiStatus::Error(AiErrorKind::Unsupported);
            cx.notify();
            return;
        };
        let prompts = match self.ai_prompts(feature, cx) {
            Ok(prompts) if !prompts.is_empty() => prompts,
            Ok(_) | Err(_) => {
                self.ai_state.status = AiStatus::Error(AiErrorKind::InvalidRequest);
                cx.notify();
                return;
            }
        };
        let images = self.workspace.ai_reference_images();
        if spec.needs_image && images.is_empty() {
            self.ai_state.status = AiStatus::Error(AiErrorKind::InvalidRequest);
            cx.notify();
            return;
        }
        let provider = self.ai_state.provider;
        let Some(capabilities) = self.provider_registry.capabilities(provider) else {
            self.ai_state.status = AiStatus::Error(AiErrorKind::Provider);
            cx.notify();
            return;
        };
        if images.len() > usize::from(capabilities.max_reference_images) {
            self.ai_state.status = AiStatus::Error(AiErrorKind::Unsupported);
            cx.notify();
            return;
        }
        let aspect_ratio = if feature == Feature::Poster {
            AiAspectRatio::PortraitNineSixteen
        } else {
            self.ai_state.aspect_ratio
        };
        let count = if prompts.len() == 1 {
            self.ai_state.count
        } else {
            1
        };
        let quality = self.ai_state.quality;
        let mask_rect = if feature == Feature::ImageEdit
            && AiEditPreset::ALL[self.ai_state.edit_preset_index] == AiEditPreset::Removal
        {
            if !capabilities.region_edit {
                self.ai_state.status = AiStatus::Error(AiErrorKind::Unsupported);
                cx.notify();
                return;
            }
            Some(self.crop_selection.read(cx).rect)
        } else {
            None
        };
        let registry = self.provider_registry.clone();
        let credentials = Arc::clone(&self.credential_store);
        self.ai_state.status = AiStatus::Generating;
        self.ai_state.results.clear();
        self.ai_state.selected_results.clear();

        let task = cx.background_executor().spawn(async move {
            run_ai_task(AiTaskRequest {
                registry,
                credentials,
                provider,
                feature,
                prompts,
                images,
                mask_rect,
                aspect_ratio,
                count,
                quality,
            })
        });
        cx.spawn(async move |this, cx| {
            let outcome = task.await;
            let _ = this.update(cx, |this, cx| {
                match outcome {
                    AiTaskOutcome::Finished(results) => {
                        let count = results.len();
                        this.ai_state.set_results(results);
                        this.config.generation_history.push(
                            rastery_core::config::GenerationHistoryRecord {
                                created_at_unix_ms: unix_time_ms(),
                                provider: provider.as_str().to_string(),
                                feature: feature.id().to_string(),
                                output_count: count,
                                saved_paths: Vec::new(),
                            },
                        );
                        this.persist_config(false);
                    }
                    AiTaskOutcome::Failed(error) => {
                        this.ai_state.status = AiStatus::Error(error);
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn prompt_for_ai_output_directory(&mut self, feature: Feature, cx: &mut Context<Self>) {
        if self.ai_state.selected_results.is_empty() {
            self.ai_state.status = AiStatus::Error(AiErrorKind::InvalidRequest);
            cx.notify();
            return;
        }
        let paths_receiver = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(tr("ai.action.save_selected")),
        });
        cx.spawn(async move |this, cx| {
            let result =
                first_path_prompt_result(classify_path_prompt_result(paths_receiver.await));
            let _ = this.update(cx, |this, cx| {
                let PathPromptResult::Selected(directory) = result else {
                    this.ai_state.status = AiStatus::Idle;
                    cx.notify();
                    return;
                };
                let selected = this
                    .ai_state
                    .selected_results
                    .iter()
                    .filter_map(|index| this.ai_state.results.get(*index))
                    .map(|result| result.generated.clone())
                    .collect::<Vec<_>>();
                let poster = (feature == Feature::Poster).then(|| {
                    (
                        [
                            this.poster_title.read(cx).value().to_string(),
                            this.poster_subtitle.read(cx).value().to_string(),
                            this.poster_corner_label.read(cx).value().to_string(),
                        ],
                        this.poster_layout.read(cx).layers(),
                    )
                });
                let task = cx.background_executor().spawn(async move {
                    let paths = if let Some((texts, layout)) = poster {
                        write_poster_results(&directory, &selected, &texts, layout)
                    } else {
                        write_ai_results(&directory, &selected)
                    }?;
                    Ok::<_, std::io::Error>((directory, paths))
                });
                cx.spawn(async move |this, cx| {
                    let outcome = task.await;
                    let _ = this.update(cx, |this, cx| {
                        match outcome {
                            Ok((directory, paths)) => {
                                this.config.last_output_dir = Some(directory);
                                if let Some(record) = this.config.generation_history.last_mut() {
                                    record.saved_paths = paths.clone();
                                }
                                this.persist_config(false);
                                this.ai_state.status = AiStatus::Saved(paths.len());
                            }
                            Err(error) => {
                                log::error!("AI result save failed: {error}");
                                this.ai_state.status = AiStatus::Error(AiErrorKind::Io);
                            }
                        }
                        cx.notify();
                    });
                })
                .detach();
            });
        })
        .detach();
    }

    fn ai_status_text(&self) -> SharedString {
        match self.ai_state.status {
            AiStatus::Idle => SharedString::default(),
            AiStatus::Generating => tr("ai.status.generating"),
            AiStatus::Ready(count) => format!("{}: {count}", t!("ai.status.ready")).into(),
            AiStatus::Saved(count) => format!("{}: {count}", t!("ai.status.saved")).into(),
            AiStatus::Error(error) => tr(error.i18n_key()),
        }
    }

    fn poster_canvas(&self, cx: &mut Context<Self>) -> impl IntoElement {
        const PREVIEW_WIDTH: f32 = 270.0;
        const PREVIEW_HEIGHT: f32 = 480.0;
        let theme = cx.theme();
        let layout = self.poster_layout.read(cx).layers();
        let values = [
            self.poster_title.read(cx).value().to_string(),
            self.poster_subtitle.read(cx).value().to_string(),
            self.poster_corner_label.read(cx).value().to_string(),
        ];
        let empty_keys = [
            "ai.poster.empty_title",
            "ai.poster.empty_subtitle",
            "ai.poster.empty_corner",
        ];
        let layers = layout
            .into_iter()
            .enumerate()
            .flat_map(|(index, layer)| {
                let text = if values[index].trim().is_empty() {
                    tr(empty_keys[index])
                } else {
                    values[index].clone().into()
                };
                let moving = self.poster_layout.clone();
                let resizing = self.poster_layout.clone();
                let handle_x = (layer.x * PREVIEW_WIDTH + 82.0).min(PREVIEW_WIDTH - 18.0);
                let handle_y =
                    (layer.y * PREVIEW_HEIGHT + layer.font_size / 4.0).min(PREVIEW_HEIGHT - 18.0);
                vec![
                    div()
                        .id(SharedString::from(format!("poster-layer-{index}")))
                        .absolute()
                        .left(px(layer.x * PREVIEW_WIDTH))
                        .top(px(layer.y * PREVIEW_HEIGHT))
                        .max_w(px(PREVIEW_WIDTH * 0.84))
                        .text_size(px(layer.font_size / 4.0))
                        .font_weight(gpui::FontWeight::BOLD)
                        .text_color(white())
                        .cursor_pointer()
                        .child(text)
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |_, event: &MouseDownEvent, _, cx| {
                                moving.update(cx, |layout, layout_cx| {
                                    layout.begin_move(index, event.position, layout_cx);
                                });
                            }),
                        )
                        .into_any_element(),
                    div()
                        .id(SharedString::from(format!("poster-resize-{index}")))
                        .absolute()
                        .left(px(handle_x))
                        .top(px(handle_y))
                        .w(px(18.0))
                        .h(px(18.0))
                        .rounded_sm()
                        .bg(theme.primary)
                        .text_color(theme.primary_foreground)
                        .cursor_pointer()
                        .child(Icon::new(IconName::ResizeCorner).small())
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |_, event: &MouseDownEvent, _, cx| {
                                resizing.update(cx, |layout, layout_cx| {
                                    layout.begin_resize(index, event.position, layout_cx);
                                });
                            }),
                        )
                        .into_any_element(),
                ]
            })
            .collect::<Vec<_>>();
        let moving = self.poster_layout.clone();
        let up = self.poster_layout.clone();
        let up_out = self.poster_layout.clone();

        v_flex()
            .gap_2()
            .child(
                div()
                    .id("poster-canvas")
                    .relative()
                    .w(px(PREVIEW_WIDTH))
                    .h(px(PREVIEW_HEIGHT))
                    .overflow_hidden()
                    .rounded_lg()
                    .border_1()
                    .border_color(theme.border)
                    .bg(black())
                    .when_some(self.ai_state.results.first(), |this, result| {
                        this.child(img(result.preview.clone()).size_full())
                    })
                    .child(
                        div()
                            .absolute()
                            .top_0()
                            .left_0()
                            .size_full()
                            .child(bounds_capture(self.poster_layout.clone())),
                    )
                    .children(layers)
                    .on_mouse_move(cx.listener(move |_, event: &MouseMoveEvent, _, cx| {
                        moving.update(cx, |layout, layout_cx| {
                            layout.on_move(event.position, layout_cx);
                        });
                    }))
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(move |_, _: &MouseUpEvent, _, cx| {
                            up.update(cx, |layout, layout_cx| layout.on_up(layout_cx));
                        }),
                    )
                    .on_mouse_up_out(
                        MouseButton::Left,
                        cx.listener(move |_, _: &MouseUpEvent, _, cx| {
                            up_out.update(cx, |layout, layout_cx| layout.on_up(layout_cx));
                        }),
                    ),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(tr("ai.poster.drag_help")),
            )
    }

    fn ai_reference_panel(
        &self,
        selectable_region: bool,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let image = self.workspace.preview()?;
        let (width, height) = self.workspace.preview_size(520.0, 320.0)?;
        let preview = div()
            .relative()
            .w(px(width))
            .h(px(height))
            .child(img(image).size_full());
        let preview = if selectable_region {
            let down = self.crop_selection.clone();
            let moving = self.crop_selection.clone();
            let up = self.crop_selection.clone();
            let up_out = self.crop_selection.clone();
            preview.child(
                div()
                    .id("ai-region-overlay")
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |_, event: &MouseDownEvent, _, cx| {
                            down.update(cx, |selection, selection_cx| {
                                selection.on_down(event.position, selection_cx);
                            });
                        }),
                    )
                    .on_mouse_move(cx.listener(move |_, event: &MouseMoveEvent, _, cx| {
                        moving.update(cx, |selection, selection_cx| {
                            selection.on_move(event.position, selection_cx);
                        });
                    }))
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(move |_, _: &MouseUpEvent, _, cx| {
                            up.update(cx, |selection, selection_cx| {
                                selection.on_up(selection_cx);
                            });
                        }),
                    )
                    .on_mouse_up_out(
                        MouseButton::Left,
                        cx.listener(move |_, _: &MouseUpEvent, _, cx| {
                            up_out.update(cx, |selection, selection_cx| {
                                selection.on_up(selection_cx);
                            });
                        }),
                    )
                    .child(selection_overlay(self.crop_selection.clone())),
            )
        } else {
            preview
        };
        Some(
            v_flex()
                .gap_2()
                .child(
                    div()
                        .p_2()
                        .rounded_lg()
                        .border_1()
                        .border_color(cx.theme().border)
                        .bg(cx.theme().secondary)
                        .child(preview),
                )
                .when(selectable_region, |this| {
                    this.child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(tr("ai.edit.region_help")),
                    )
                })
                .into_any_element(),
        )
    }

    fn ai_page(&self, feature: Feature, cx: &mut Context<Self>) -> impl IntoElement {
        let primary = cx.theme().primary;
        let muted = cx.theme().muted_foreground;
        let border = cx.theme().border;
        let background = cx.theme().background;
        let secondary = cx.theme().secondary;
        let sidebar_accent = cx.theme().sidebar_accent;
        let sidebar_accent_foreground = cx.theme().sidebar_accent_foreground;
        let spec = feature.ai_spec().expect("AI page must have a feature spec");
        let capabilities = self
            .provider_registry
            .capabilities(self.ai_state.provider)
            .expect("registered provider");
        let busy = matches!(self.ai_state.status, AiStatus::Generating);
        let region_edit = feature == Feature::ImageEdit
            && AiEditPreset::ALL[self.ai_state.edit_preset_index] == AiEditPreset::Removal;
        let region_supported = !region_edit || capabilities.region_edit;
        let reference = self.ai_reference_panel(region_edit, cx);
        let poster =
            (feature == Feature::Poster).then(|| self.poster_canvas(cx).into_any_element());
        let tier_buttons = spec
            .tiers
            .iter()
            .enumerate()
            .map(|(index, tier)| {
                let selected = self.ai_state.selected_tiers.contains(&index);
                Button::new(SharedString::from(format!("ai-tier-{index}")))
                    .label(tr(&format!("ai.tier.{tier}")))
                    .when(selected, |button| button.primary())
                    .when(!selected, |button| button.outline())
                    .disabled(busy)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.ai_state.toggle_tier(feature, index);
                        cx.notify();
                    }))
                    .into_any_element()
            })
            .collect::<Vec<_>>();
        let original_preview = self.workspace.preview();
        let original_dimensions = self.workspace.current_dimensions();
        let results = self
            .ai_state
            .results
            .iter()
            .enumerate()
            .map(|(index, result)| {
                let selected = self.ai_state.selected_results.contains(&index);
                let show_original = feature == Feature::OldPhotoRestoration
                    && self.ai_state.compare_original
                    && original_preview.is_some()
                    && original_dimensions.is_some();
                let (preview, preview_width, preview_height) = if show_original {
                    let (width, height) = original_dimensions.expect("checked above");
                    (
                        original_preview.clone().expect("checked above"),
                        width,
                        height,
                    )
                } else {
                    (result.preview.clone(), result.width, result.height)
                };
                let scale = (240.0 / preview_width as f32)
                    .min(180.0 / preview_height as f32)
                    .min(1.0);
                v_flex()
                    .id(SharedString::from(format!("ai-result-{index}")))
                    .gap_2()
                    .p_2()
                    .rounded_lg()
                    .border_1()
                    .border_color(if selected { primary } else { border })
                    .cursor_pointer()
                    .child(
                        img(preview)
                            .w(px(preview_width as f32 * scale))
                            .h(px(preview_height as f32 * scale)),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(muted)
                            .child(tr(&format!("ai.tier.{}", result.tier_id))),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.ai_state.toggle_result(index);
                        cx.notify();
                    }))
                    .when(feature == Feature::OldPhotoRestoration, |this| {
                        this.on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _: &MouseDownEvent, _, cx| {
                                this.ai_state.compare_original = true;
                                cx.notify();
                            }),
                        )
                        .on_mouse_up(
                            MouseButton::Left,
                            cx.listener(|this, _: &MouseUpEvent, _, cx| {
                                this.ai_state.compare_original = false;
                                cx.notify();
                            }),
                        )
                        .on_mouse_up_out(
                            MouseButton::Left,
                            cx.listener(|this, _: &MouseUpEvent, _, cx| {
                                this.ai_state.compare_original = false;
                                cx.notify();
                            }),
                        )
                    })
                    .into_any_element()
            })
            .collect::<Vec<_>>();

        let has_results = !results.is_empty();
        let has_reference = reference.is_some();
        let has_poster = poster.is_some();
        h_flex()
            .id("ai-workspace")
            .size_full()
            .items_start()
            .gap_5()
            .overflow_y_scroll()
            .pb_6()
            .on_drop(cx.listener(|this, paths: &ExternalPaths, _, cx| {
                this.handle_drop(paths.paths().to_vec(), cx);
            }))
            .child(
                v_flex()
                    .flex_1()
                    .min_w(px(420.0))
                    .min_h(px(520.0))
                    .p_4()
                    .gap_4()
                    .rounded_lg()
                    .border_1()
                    .border_color(border)
                    .bg(background)
                    .child(
                        h_flex()
                            .justify_between()
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .child(tr("workspace.preview")),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(muted)
                                    .child(tr("ai.workspace.result_hint")),
                            ),
                    )
                    .when_some(reference, |this, reference| this.child(reference))
                    .when_some(poster, |this, poster| this.child(poster))
                    .when(!has_reference && !has_poster && !has_results, |this| {
                        this.child(
                            div()
                                .flex_1()
                                .min_h(px(420.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded_lg()
                                .border_1()
                                .border_dashed()
                                .border_color(border)
                                .bg(secondary)
                                .child(
                                    v_flex()
                                        .items_center()
                                        .gap_3()
                                        .px_8()
                                        .text_center()
                                        .child(
                                            div()
                                                .size(px(54.0))
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .rounded_lg()
                                                .bg(sidebar_accent)
                                                .text_color(sidebar_accent_foreground)
                                                .child(Icon::new(IconName::Bot).size_6()),
                                        )
                                        .child(
                                            div()
                                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                                .child(tr("ai.workspace.empty_title")),
                                        )
                                        .child(
                                            div()
                                                .max_w(px(420.0))
                                                .text_sm()
                                                .text_color(muted)
                                                .child(tr("ai.workspace.empty_desc")),
                                        ),
                                ),
                        )
                    })
                    .when(
                        feature == Feature::OldPhotoRestoration && has_results,
                        |this| {
                            this.child(
                                div()
                                    .text_xs()
                                    .text_color(muted)
                                    .child(tr("ai.comparison.hold_original")),
                            )
                        },
                    )
                    .when(has_results, |this| {
                        this.child(h_flex().flex_wrap().gap_3().children(results))
                    }),
            )
            .child(
                v_flex()
                    .w(px(300.0))
                    .flex_shrink_0()
                    .gap_3()
                    .p_4()
                    .rounded_lg()
                    .border_1()
                    .border_color(border)
                    .bg(background)
                    .child(
                        div()
                            .text_sm()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(tr("workspace.parameters")),
                    )
                    .child(Input::new(&self.ai_prompt).cleanable(true))
                    .when(feature == Feature::Poster, |this| {
                        this.child(Input::new(&self.poster_title).cleanable(true))
                            .child(Input::new(&self.poster_subtitle).cleanable(true))
                            .child(Input::new(&self.poster_corner_label).cleanable(true))
                    })
                    .when(feature == Feature::PromotionalPoster, |this| {
                        this.child(Input::new(&self.poster_title).cleanable(true))
                    })
                    .when(feature == Feature::ImageEdit, |this| {
                        this.child(
                            Button::new("ai-edit-preset")
                                .outline()
                                .label(tr(&format!(
                                    "ai.edit.{}",
                                    AiEditPreset::ALL[self.ai_state.edit_preset_index].id()
                                )))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.cycle_edit_preset(cx);
                                })),
                        )
                    })
                    .child(
                        v_flex()
                            .gap_2()
                            .child(
                                Button::new("ai-provider")
                                    .outline()
                                    .label(tr(provider_label_key(self.ai_state.provider)))
                                    .disabled(busy)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.cycle_ai_provider(cx);
                                    })),
                            )
                            .child(
                                h_flex()
                                    .gap_2()
                                    .child(
                                        Button::new("ai-ratio")
                                            .outline()
                                            .label(self.ai_state.aspect_ratio.to_string())
                                            .disabled(busy)
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.cycle_ai_ratio(cx);
                                            })),
                                    )
                                    .child(
                                        Button::new("ai-count")
                                            .outline()
                                            .label(format!(
                                                "{}: {}",
                                                t!("ai.parameter.count"),
                                                self.ai_state.count
                                            ))
                                            .disabled(
                                                busy || capabilities.max_generation_count == 1,
                                            )
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.cycle_ai_count(cx);
                                            })),
                                    ),
                            )
                            .child(
                                Button::new("ai-quality")
                                    .outline()
                                    .label(tr(quality_label_key(self.ai_state.quality)))
                                    .disabled(busy)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.cycle_ai_quality(cx);
                                    })),
                            ),
                    )
                    .when(!tier_buttons.is_empty(), |this| {
                        this.child(h_flex().flex_wrap().gap_2().children(tier_buttons))
                    })
                    .when(
                        feature == Feature::ProductRecolor
                            && self.ai_state.selected_tiers.contains(&3),
                        |this| {
                            this.child(
                                h_flex()
                                    .gap_2()
                                    .child(tr("ai.parameter.custom_color"))
                                    .child(ColorPicker::new(&self.ai_custom_color)),
                            )
                        },
                    )
                    .child(div().text_xs().text_color(muted).child(format!(
                        "{} {} · {} {}",
                        t!("ai.capability.references"),
                        capabilities.max_reference_images,
                        t!("ai.capability.outputs"),
                        capabilities.max_generation_count
                    )))
                    .when(
                        spec.needs_image || feature == Feature::TextToImage,
                        |this| {
                            this.child(
                                Button::new("ai-open-reference")
                                    .outline()
                                    .icon(IconName::FolderOpen)
                                    .label(tr("ai.action.open_reference"))
                                    .disabled(busy)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.prompt_for_images(true, cx);
                                    })),
                            )
                        },
                    )
                    .child(
                        Button::new("ai-generate")
                            .primary()
                            .icon(IconName::Bot)
                            .label(tr("ai.action.generate"))
                            .disabled(busy || !region_supported)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.start_ai_generation(feature, cx);
                            })),
                    )
                    .when(has_results, |this| {
                        this.child(
                            Button::new("ai-save-selected")
                                .outline()
                                .label(tr("ai.action.save_selected"))
                                .disabled(self.ai_state.selected_results.is_empty())
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.prompt_for_ai_output_directory(feature, cx);
                                })),
                        )
                    })
                    .when(!self.ai_status_text().is_empty(), |this| {
                        this.child(
                            h_flex()
                                .gap_2()
                                .text_sm()
                                .text_color(primary)
                                .when(busy, |this| this.child(Spinner::new()))
                                .child(self.ai_status_text()),
                        )
                    })
                    .when(!region_supported, |this| {
                        this.child(
                            div()
                                .text_sm()
                                .text_color(muted)
                                .child(tr("ai.edit.region_provider_required")),
                        )
                    }),
            )
    }

    fn settings_page(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let credential_rows = ProviderId::ALL
            .into_iter()
            .map(|provider| {
                h_flex()
                    .gap_3()
                    .child(
                        div()
                            .w(px(170.0))
                            .text_sm()
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .child(tr(provider_label_key(provider))),
                    )
                    .child(
                        div().flex_1().child(
                            Input::new(self.api_key_input(provider))
                                .mask_toggle()
                                .cleanable(true),
                        ),
                    )
                    .child(
                        Button::new(SharedString::from(format!(
                            "settings-save-key-{}",
                            provider.as_str()
                        )))
                        .primary()
                        .label(tr("ai.settings.save_key"))
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                this.save_api_key(provider, window, cx);
                            },
                        )),
                    )
                    .child(
                        Button::new(SharedString::from(format!(
                            "settings-delete-key-{}",
                            provider.as_str()
                        )))
                        .outline()
                        .icon(IconName::Delete)
                        .tooltip(tr("ai.settings.delete_key"))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.delete_api_key(provider, cx);
                        })),
                    )
                    .into_any_element()
            })
            .collect::<Vec<_>>();
        let capability_rows = ProviderId::ALL
            .into_iter()
            .map(|provider| {
                let capabilities = self
                    .provider_registry
                    .capabilities(provider)
                    .expect("registered provider");
                let ratios = capabilities
                    .supported_aspect_ratios
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(" · ");
                h_flex()
                    .min_h(px(48.0))
                    .px_4()
                    .border_t_1()
                    .border_color(theme.border)
                    .text_sm()
                    .child(
                        div()
                            .w(px(220.0))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .child(tr(provider_label_key(provider))),
                    )
                    .child(
                        div()
                            .w(px(150.0))
                            .text_color(theme.muted_foreground)
                            .child(capabilities.max_reference_images.to_string()),
                    )
                    .child(
                        div()
                            .w(px(150.0))
                            .text_color(theme.muted_foreground)
                            .child(capabilities.max_generation_count.to_string()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .text_xs()
                            .text_color(theme.primary)
                            .child(ratios),
                    )
                    .into_any_element()
            })
            .collect::<Vec<_>>();
        let output_dir: SharedString = self
            .config
            .last_output_dir
            .as_ref()
            .map(|path| path.display().to_string().into())
            .unwrap_or_else(|| tr("settings.no_output_directory"));
        let config_path: SharedString = self
            .config_store
            .as_ref()
            .map(|store| store.path().display().to_string().into())
            .unwrap_or_default();
        let status = self.workspace.status_text();
        let estimate = estimate_text(self.estimate_state);

        v_flex()
            .id("settings-workspace")
            .size_full()
            .items_center()
            .flex_1()
            .overflow_y_scroll()
            .child(
                v_flex()
                    .w_full()
                    .max_w(px(940.0))
                    .p_6()
                    .pb_8()
                    .gap_5()
                    .child(
                        div()
                            .text_xl()
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(theme.foreground)
                            .child(tr("settings.title")),
                    )
                    .child(
                        v_flex()
                            .gap_3()
                            .p_5()
                            .rounded_lg()
                            .border_1()
                            .border_color(theme.border)
                            .bg(theme.background)
                            .child(
                                h_flex()
                                    .gap_2()
                                    .child(
                                        Icon::new(IconName::Asterisk)
                                            .small()
                                            .text_color(theme.primary),
                                    )
                                    .child(
                                        div()
                                            .text_lg()
                                            .font_weight(gpui::FontWeight::SEMIBOLD)
                                            .child(tr("ai.settings.title")),
                                    ),
                            )
                            .child(
                                h_flex()
                                    .gap_2()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child(
                                        Icon::new(IconName::CircleCheck)
                                            .small()
                                            .text_color(theme.success),
                                    )
                                    .child(tr("ai.settings.key_security")),
                            )
                            .children(credential_rows)
                            .child(
                                h_flex()
                                    .gap_2()
                                    .child(
                                        Button::new("settings-default-provider")
                                            .outline()
                                            .label(format!(
                                                "{}: {}",
                                                t!("ai.settings.default_provider"),
                                                t!(provider_label_key(self.ai_state.provider))
                                            ))
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.cycle_ai_provider(cx);
                                            })),
                                    )
                                    .child(format!(
                                        "{}: {}",
                                        t!("ai.settings.history"),
                                        self.config.generation_history.len()
                                    ))
                                    .child(
                                        Button::new("settings-clear-ai-history")
                                            .outline()
                                            .label(tr("ai.settings.clear_history"))
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.clear_ai_history(cx);
                                            })),
                                    ),
                            ),
                    )
                    .child(
                        v_flex()
                            .rounded_lg()
                            .border_1()
                            .border_color(theme.border)
                            .bg(theme.background)
                            .child(
                                h_flex()
                                    .h(px(54.0))
                                    .px_4()
                                    .gap_2()
                                    .child(
                                        Icon::new(IconName::Bot).small().text_color(theme.primary),
                                    )
                                    .child(
                                        div()
                                            .font_weight(gpui::FontWeight::SEMIBOLD)
                                            .child(tr("settings.provider_capabilities")),
                                    ),
                            )
                            .child(
                                h_flex()
                                    .h(px(38.0))
                                    .px_4()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child(div().w(px(220.0)).child(tr("settings.provider")))
                                    .child(div().w(px(150.0)).child(tr("settings.reference_limit")))
                                    .child(div().w(px(150.0)).child(tr("settings.output_limit")))
                                    .child(div().flex_1().child(tr("settings.aspect_ratios"))),
                            )
                            .children(capability_rows),
                    )
                    .child(
                        v_flex()
                            .gap_4()
                            .p_5()
                            .rounded_lg()
                            .border_1()
                            .border_color(theme.border)
                            .bg(theme.background)
                            .child(
                                h_flex()
                                    .gap_2()
                                    .child(
                                        Icon::new(IconName::Settings2)
                                            .small()
                                            .text_color(theme.primary),
                                    )
                                    .child(
                                        div()
                                            .text_lg()
                                            .font_weight(gpui::FontWeight::SEMIBOLD)
                                            .child(tr("settings.export_title")),
                                    ),
                            )
                            .child(
                                h_flex()
                                    .gap_3()
                                    .child(
                                        v_flex()
                                            .w(px(250.0))
                                            .gap_2()
                                            .child(tr("settings.export_format"))
                                            .child(
                                                Select::new(&self.export_format_select).w_full(),
                                            ),
                                    )
                                    .child(
                                        v_flex()
                                            .flex_1()
                                            .gap_2()
                                            .when(
                                                self.config.default_export_format
                                                    == OutputFormat::Png,
                                                |this| {
                                                    this.child(tr("settings.png_compression"))
                                                        .child(
                                                        Button::new("settings-png-compression")
                                                            .outline()
                                                            .label(tr(png_compression_label_key(
                                                                self.config.default_png_compression,
                                                            )))
                                                            .tooltip(tr("help.png_compression"))
                                                            .on_click(cx.listener(
                                                                |this, _, _, cx| {
                                                                    this.cycle_png_compression(cx);
                                                                },
                                                            )),
                                                    )
                                                },
                                            )
                                            .when(
                                                self.config.default_export_format
                                                    != OutputFormat::Png,
                                                |this| {
                                                    this.child(format!(
                                                        "{}: {}",
                                                        t!("settings.export_quality"),
                                                        self.config.default_export_quality.get()
                                                    ))
                                                    .child(
                                                        div()
                                                            .id("settings-quality-slider")
                                                            .w_full()
                                                            .tooltip(|window, cx| {
                                                                Tooltip::new(tr("help.quality"))
                                                                    .build(window, cx)
                                                            })
                                                            .child(
                                                                Slider::new(&self.quality_slider)
                                                                    .w_full(),
                                                            ),
                                                    )
                                                },
                                            )
                                            .child(
                                                div()
                                                    .text_sm()
                                                    .text_color(theme.muted_foreground)
                                                    .child(estimate),
                                            ),
                                    ),
                            )
                            .child(
                                v_flex()
                                    .gap_1()
                                    .child(tr("settings.output_directory"))
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(theme.muted_foreground)
                                            .child(output_dir),
                                    ),
                            ),
                    )
                    .child(
                        v_flex()
                            .gap_4()
                            .p_5()
                            .rounded_lg()
                            .border_1()
                            .border_color(theme.border)
                            .bg(theme.background)
                            .child(
                                h_flex()
                                    .gap_2()
                                    .child(
                                        Icon::new(IconName::Globe)
                                            .small()
                                            .text_color(theme.primary),
                                    )
                                    .child(
                                        div()
                                            .text_lg()
                                            .font_weight(gpui::FontWeight::SEMIBOLD)
                                            .child(tr("lang.label")),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(theme.muted_foreground)
                                            .child(tr("settings.language_hint")),
                                    ),
                            )
                            .child(
                                h_flex()
                                    .gap_2()
                                    .child(
                                        Button::new("settings-language-zh")
                                            .label(tr("lang.zh_cn"))
                                            .when(
                                                self.config.language == Language::ZhCn,
                                                |button| button.primary(),
                                            )
                                            .when(
                                                self.config.language != Language::ZhCn,
                                                |button| button.outline(),
                                            )
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.set_language(Language::ZhCn, window, cx);
                                            })),
                                    )
                                    .child(
                                        Button::new("settings-language-en")
                                            .label(tr("lang.english"))
                                            .when(self.config.language == Language::En, |button| {
                                                button.primary()
                                            })
                                            .when(self.config.language != Language::En, |button| {
                                                button.outline()
                                            })
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.set_language(Language::En, window, cx);
                                            })),
                                    ),
                            )
                            .when(!config_path.is_empty(), |this| {
                                this.child(
                                    div()
                                        .text_xs()
                                        .text_color(theme.muted_foreground)
                                        .child(config_path),
                                )
                            }),
                    )
                    .when(!status.is_empty(), |this| {
                        this.child(div().text_sm().text_color(theme.primary).child(status))
                    }),
            )
    }
}

fn section_for_feature(feature: Feature) -> Section {
    match feature {
        Feature::Edit
        | Feature::Collage
        | Feature::Batch
        | Feature::Slice
        | Feature::QrCode
        | Feature::Exif
        | Feature::Beautify => Section::BasicImage,
        Feature::TextToImage
        | Feature::ImageEdit
        | Feature::VideoWatermark
        | Feature::VideoSubtitle
        | Feature::VideoClarity => Section::AiGeneration,
        Feature::Gif | Feature::Poster => Section::CreativeOutput,
        _ => Section::IndustryTools,
    }
}

struct NavigationGroup {
    label_key: &'static str,
    icon: IconName,
    features: &'static [Feature],
}

fn navigation_groups() -> [NavigationGroup; 5] {
    [
        NavigationGroup {
            label_key: Section::BasicImage.nav_key(),
            icon: IconName::GalleryVerticalEnd,
            features: &[
                Feature::Edit,
                Feature::Batch,
                Feature::QrCode,
                Feature::Exif,
                Feature::Slice,
            ],
        },
        NavigationGroup {
            label_key: Section::AiGeneration.nav_key(),
            icon: IconName::Bot,
            features: &[Feature::TextToImage, Feature::ImageEdit],
        },
        NavigationGroup {
            label_key: Section::IndustryTools.nav_key(),
            icon: IconName::Building2,
            features: &[
                Feature::OldPhotoRestoration,
                Feature::IdPhoto,
                Feature::AvatarStudio,
                Feature::MemeGenerator,
                Feature::AiPortrait,
                Feature::ModelTryOn,
                Feature::ProductRecolor,
                Feature::PromotionalPoster,
                Feature::PlatformAdaptation,
                Feature::CoverFactory,
                Feature::ArticleIllustration,
                Feature::FoodEnhancement,
                Feature::InteriorPreview,
            ],
        },
        NavigationGroup {
            label_key: Section::CreativeOutput.nav_key(),
            icon: IconName::Palette,
            features: &[
                Feature::Collage,
                Feature::Gif,
                Feature::Beautify,
                Feature::Poster,
            ],
        },
        NavigationGroup {
            label_key: "nav.video_tools",
            icon: IconName::WindowMaximize,
            features: &[
                Feature::VideoWatermark,
                Feature::VideoSubtitle,
                Feature::VideoClarity,
            ],
        },
    ]
}

fn feature_icon(feature: Feature) -> IconName {
    match feature {
        Feature::Edit => IconName::Frame,
        Feature::Collage => IconName::GalleryVerticalEnd,
        Feature::Batch => IconName::Replace,
        Feature::Slice => IconName::LayoutDashboard,
        Feature::QrCode => IconName::Asterisk,
        Feature::Exif => IconName::File,
        Feature::Beautify => IconName::Palette,
        Feature::Gif => IconName::GalleryVerticalEnd,
        Feature::Poster => IconName::Frame,
        Feature::TextToImage | Feature::ImageEdit => IconName::Bot,
        Feature::VideoWatermark | Feature::VideoSubtitle | Feature::VideoClarity => {
            IconName::WindowMaximize
        }
        Feature::OldPhotoRestoration => IconName::GalleryVerticalEnd,
        Feature::IdPhoto | Feature::AiPortrait => IconName::CircleUser,
        Feature::AvatarStudio | Feature::ModelTryOn => IconName::User,
        Feature::MemeGenerator => IconName::Heart,
        Feature::ProductRecolor => IconName::Palette,
        Feature::PromotionalPoster => IconName::ChartPie,
        Feature::PlatformAdaptation => IconName::ResizeCorner,
        Feature::CoverFactory => IconName::BookOpen,
        Feature::ArticleIllustration => IconName::File,
        Feature::FoodEnhancement => IconName::Star,
        Feature::InteriorPreview => IconName::Building2,
    }
}

fn feature_badge(feature: Feature, cx: &Context<AppShell>) -> (SharedString, Hsla) {
    if feature.is_v1() {
        (tr("home.badge_local"), cx.theme().success)
    } else if feature.is_ai() {
        (tr("home.badge_ai"), cx.theme().primary)
    } else {
        (tr("home.badge_dev"), cx.theme().muted_foreground)
    }
}

fn feature_header_badge(feature: Feature, cx: &Context<AppShell>) -> AnyElement {
    let (label, color) = feature_badge(feature, cx);
    div()
        .px_2()
        .py_0p5()
        .rounded_md()
        .bg(cx.theme().sidebar_accent)
        .text_xs()
        .text_color(color)
        .child(label)
        .into_any_element()
}

fn feature_matches_search(feature: Feature, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    let name = t!(feature.name_key()).to_string().to_lowercase();
    let description = t!(feature.desc_key()).to_string().to_lowercase();
    name.contains(query) || description.contains(query)
}

fn provider_from_config(value: &str) -> ProviderId {
    match value {
        "nano-banana" => ProviderId::NanoBanana,
        "openai" => ProviderId::OpenAi,
        _ => ProviderId::Seedream,
    }
}

fn provider_label_key(provider: ProviderId) -> &'static str {
    match provider {
        ProviderId::Seedream => "ai.provider.seedream",
        ProviderId::NanoBanana => "ai.provider.nano_banana",
        ProviderId::OpenAi => "ai.provider.openai",
    }
}

fn quality_label_key(quality: GenerationQuality) -> &'static str {
    match quality {
        GenerationQuality::Low => "ai.quality.low",
        GenerationQuality::Medium => "ai.quality.medium",
        GenerationQuality::High => "ai.quality.high",
    }
}

fn industry_tool(feature: Feature) -> Option<IndustryTool> {
    match feature {
        Feature::OldPhotoRestoration => Some(IndustryTool::OldPhotoRestoration),
        Feature::IdPhoto => Some(IndustryTool::IdPhoto),
        Feature::AvatarStudio => Some(IndustryTool::AvatarStudio),
        Feature::MemeGenerator => Some(IndustryTool::MemeGenerator),
        Feature::AiPortrait => Some(IndustryTool::AiPortrait),
        Feature::ModelTryOn => Some(IndustryTool::ModelTryOn),
        Feature::ProductRecolor => Some(IndustryTool::ProductRecolor),
        Feature::PromotionalPoster => Some(IndustryTool::PromotionalPoster),
        Feature::PlatformAdaptation => Some(IndustryTool::PlatformAdaptation),
        Feature::CoverFactory => Some(IndustryTool::CoverFactory),
        Feature::ArticleIllustration => Some(IndustryTool::ArticleIllustration),
        Feature::FoodEnhancement => Some(IndustryTool::FoodEnhancement),
        Feature::InteriorPreview => Some(IndustryTool::InteriorPreview),
        _ => None,
    }
}

fn unix_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

fn run_ai_task(task_request: AiTaskRequest) -> AiTaskOutcome {
    let AiTaskRequest {
        registry,
        credentials,
        provider,
        feature,
        prompts,
        images,
        mask_rect,
        aspect_ratio,
        count,
        quality,
    } = task_request;
    let references = match images
        .iter()
        .map(|image| {
            format::encode(
                image,
                EncodeSettings::Png {
                    compression: PngCompression::Default,
                },
            )
            .map_err(|_| AiError::InvalidRequest("reference image encoding failed".into()))
            .and_then(|bytes| ImageInput::new(bytes, "image/png"))
        })
        .collect::<Result<Vec<_>, _>>()
    {
        Ok(references) => references,
        Err(error) => return AiTaskOutcome::Failed(error.into()),
    };
    let mask = match mask_rect {
        Some(rect) => match images.first() {
            Some(image) => match selection_mask(image.width(), image.height(), rect) {
                Ok(mask) => Some(mask),
                Err(error) => return AiTaskOutcome::Failed(error.into()),
            },
            None => return AiTaskOutcome::Failed(AiErrorKind::InvalidRequest),
        },
        None => None,
    };

    let mut key = None;
    let mut rendered = Vec::new();
    for task in prompts {
        if feature == Feature::PlatformAdaptation
            && let (Some(source), Some((width, height))) = (
                images.first(),
                exact_output_dimensions(feature, &task.tier_id),
            )
            && aspect_ratio_is_close(source.width(), source.height(), width, height)
        {
            match exact_cover(source, width, height).and_then(|image| {
                format::encode(
                    &image,
                    EncodeSettings::Png {
                        compression: PngCompression::Default,
                    },
                )
                .map(|bytes| (image, bytes))
            }) {
                Ok((image, bytes)) => {
                    rendered.push(RenderedAiImage {
                        generated: GeneratedImage {
                            bytes,
                            mime_type: "image/png".into(),
                        },
                        preview: crate::workspace::to_render_image(&image),
                        width,
                        height,
                        tier_id: task.tier_id,
                    });
                    continue;
                }
                Err(_) => return AiTaskOutcome::Failed(AiErrorKind::Provider),
            }
        }

        if key.is_none() {
            key = match credentials.get(provider) {
                Ok(key) => Some(key),
                Err(error) => return AiTaskOutcome::Failed(error.into()),
            };
        }
        let request = GenerationRequest {
            prompt: task.prompt,
            reference_images: references.clone(),
            mask: mask.clone(),
            aspect_ratio,
            count,
            quality,
        };
        let response = match registry.generate(
            provider,
            &request,
            key.as_ref().expect("credential initialized above"),
        ) {
            Ok(response) => response,
            Err(error) => return AiTaskOutcome::Failed(error.into()),
        };
        if response.images.is_empty() {
            return AiTaskOutcome::Failed(AiErrorKind::Provider);
        }
        for generated in response.images {
            let image = match format::decode(&generated.bytes) {
                Ok(image) => image,
                Err(_) => return AiTaskOutcome::Failed(AiErrorKind::Provider),
            };
            let (image, generated) =
                match normalize_ai_output(feature, &task.tier_id, image, generated) {
                    Ok(output) => output,
                    Err(_) => return AiTaskOutcome::Failed(AiErrorKind::Provider),
                };
            let (width, height) = image.dimensions();
            rendered.push(RenderedAiImage {
                generated,
                preview: crate::workspace::to_render_image(&image),
                width,
                height,
                tier_id: task.tier_id.clone(),
            });
        }
    }

    AiTaskOutcome::Finished(rendered)
}

fn selection_mask(width: u32, height: u32, rect: NormRect) -> Result<ImageInput, AiError> {
    let mut mask =
        rastery_core::RgbaImage::from_pixel(width, height, image::Rgba([255, 255, 255, 255]));
    let left = (rect.x.clamp(0.0, 1.0) * width as f32).round() as u32;
    let top = (rect.y.clamp(0.0, 1.0) * height as f32).round() as u32;
    let right = ((rect.x + rect.w).clamp(0.0, 1.0) * width as f32).round() as u32;
    let bottom = ((rect.y + rect.h).clamp(0.0, 1.0) * height as f32).round() as u32;
    for y in top.min(height)..bottom.min(height) {
        for x in left.min(width)..right.min(width) {
            mask.put_pixel(x, y, image::Rgba([0, 0, 0, 0]));
        }
    }
    let bytes = format::encode(
        &mask,
        EncodeSettings::Png {
            compression: PngCompression::Default,
        },
    )
    .map_err(|_| AiError::InvalidRequest("region mask encoding failed".into()))?;
    ImageInput::new(bytes, "image/png")
}

fn normalize_ai_output(
    feature: Feature,
    tier_id: &str,
    image: rastery_core::RgbaImage,
    generated: GeneratedImage,
) -> rastery_core::Result<(rastery_core::RgbaImage, GeneratedImage)> {
    let Some((width, height)) = exact_output_dimensions(feature, tier_id) else {
        return Ok((image, generated));
    };
    let image = exact_cover(&image, width, height)?;
    let bytes = format::encode(
        &image,
        EncodeSettings::Png {
            compression: PngCompression::Default,
        },
    )?;
    Ok((
        image,
        GeneratedImage {
            bytes,
            mime_type: "image/png".into(),
        },
    ))
}

fn exact_output_dimensions(feature: Feature, tier_id: &str) -> Option<(u32, u32)> {
    match feature {
        Feature::Poster => Some((POSTER_WIDTH, POSTER_HEIGHT)),
        Feature::IdPhoto if tier_id.starts_with("one-inch-") => Some((295, 413)),
        Feature::IdPhoto if tier_id.starts_with("two-inch-") => Some((413, 579)),
        Feature::IdPhoto if tier_id.starts_with("visa-") => Some((600, 600)),
        Feature::AvatarStudio | Feature::MemeGenerator => Some((1024, 1024)),
        Feature::PromotionalPoster => Some((1200, 1600)),
        Feature::PlatformAdaptation => match tier_id {
            "taobao-main-800x800" => Some((800, 800)),
            "xiaohongshu-1242x1656" => Some((1242, 1656)),
            "wechat-cover-900x383" => Some((900, 383)),
            "douyin-1080x1920" => Some((1080, 1920)),
            _ => None,
        },
        _ => None,
    }
}

fn aspect_ratio_is_close(source_width: u32, source_height: u32, width: u32, height: u32) -> bool {
    let source = f64::from(source_width) / f64::from(source_height.max(1));
    let target = f64::from(width) / f64::from(height.max(1));
    ((source - target) / target).abs() <= 0.08
}

fn exact_cover(
    image: &rastery_core::RgbaImage,
    width: u32,
    height: u32,
) -> rastery_core::Result<rastery_core::RgbaImage> {
    use rastery_core::transform::{CropRect, ResizeFilter};

    let source_ratio = f64::from(image.width()) / f64::from(image.height());
    let target_ratio = f64::from(width) / f64::from(height);
    let crop = if source_ratio > target_ratio {
        let crop_width = (f64::from(image.height()) * target_ratio).round() as u32;
        CropRect {
            x: (image.width() - crop_width) / 2,
            y: 0,
            width: crop_width,
            height: image.height(),
        }
    } else {
        let crop_height = (f64::from(image.width()) / target_ratio).round() as u32;
        CropRect {
            x: 0,
            y: (image.height() - crop_height) / 2,
            width: image.width(),
            height: crop_height,
        }
    };
    let cropped = rastery_core::transform::crop(image, crop)?;
    rastery_core::transform::resize(&cropped, width, height, ResizeFilter::Lanczos3)
}

fn write_ai_results(directory: &Path, images: &[GeneratedImage]) -> std::io::Result<Vec<PathBuf>> {
    fs::create_dir_all(directory)?;
    let timestamp = unix_time_ms();
    images
        .iter()
        .enumerate()
        .map(|(index, image)| {
            let extension = match image.mime_type.as_str() {
                "image/jpeg" => "jpg",
                "image/webp" => "webp",
                _ => "png",
            };
            let path = directory.join(rastery_core::naming::ai_filename(
                timestamp,
                index + 1,
                extension,
            ));
            fs::write(&path, &image.bytes)?;
            Ok(path)
        })
        .collect()
}

fn write_poster_results(
    directory: &Path,
    backgrounds: &[GeneratedImage],
    texts: &[String; 3],
    layout: [PosterTextLayer; 3],
) -> std::io::Result<Vec<PathBuf>> {
    fs::create_dir_all(directory)?;
    let timestamp = unix_time_ms();
    backgrounds
        .iter()
        .enumerate()
        .map(|(index, background)| {
            let background = format::decode(&background.bytes)
                .and_then(|image| exact_cover(&image, POSTER_WIDTH, POSTER_HEIGHT))
                .map_err(std::io::Error::other)?;
            let layers = texts
                .iter()
                .zip(layout)
                .filter(|(text, _)| !text.trim().is_empty())
                .map(|(text, layout)| {
                    crate::text_watermark::rasterize_with_size(text, layout.font_size)
                        .map(|image| rastery_core::poster::RasterLayer {
                            image,
                            x: (layout.x * POSTER_WIDTH as f32).round() as u32,
                            y: (layout.y * POSTER_HEIGHT as f32).round() as u32,
                        })
                        .map_err(std::io::Error::other)
                })
                .collect::<std::io::Result<Vec<_>>>()?;
            let output = rastery_core::poster::compose(&background, &layers);
            let bytes = format::encode(
                &output,
                EncodeSettings::Png {
                    compression: PngCompression::Default,
                },
            )
            .map_err(std::io::Error::other)?;
            let path = directory.join(rastery_core::naming::ai_filename(
                timestamp,
                index + 1,
                "png",
            ));
            fs::write(&path, bytes)?;
            Ok(path)
        })
        .collect()
}

fn default_export_settings(config: &AppConfig) -> EncodeSettings {
    match config.default_export_format {
        OutputFormat::Png => EncodeSettings::Png {
            compression: config.default_png_compression,
        },
        OutputFormat::Jpeg => EncodeSettings::Jpeg {
            quality: config.default_export_quality,
        },
        OutputFormat::Webp => EncodeSettings::WebpLossy {
            quality: config.default_export_quality,
        },
    }
}

fn initial_paths_feature(path_count: usize) -> Feature {
    if path_count > 1 {
        Feature::Batch
    } else {
        Feature::Edit
    }
}

fn next_png_compression(compression: PngCompression) -> PngCompression {
    match compression {
        PngCompression::Fast => PngCompression::Default,
        PngCompression::Default => PngCompression::Best,
        PngCompression::Best => PngCompression::Fast,
    }
}

/// 默认导出格式下拉的候选项，顺序必须与 [`export_format_index`] 保持一致。
/// 用格式缩写（不翻译）作为条目文案，选中后由 [`export_format_from_label`] 映射回枚举。
fn export_format_items() -> Vec<SharedString> {
    vec!["PNG".into(), "JPEG".into(), "WebP".into()]
}

fn export_format_index(format: OutputFormat) -> usize {
    match format {
        OutputFormat::Png => 0,
        OutputFormat::Jpeg => 1,
        OutputFormat::Webp => 2,
    }
}

fn export_format_from_label(label: &str) -> OutputFormat {
    match label {
        "PNG" => OutputFormat::Png,
        "JPEG" => OutputFormat::Jpeg,
        _ => OutputFormat::Webp,
    }
}

fn png_compression_label_key(compression: PngCompression) -> &'static str {
    match compression {
        PngCompression::Fast => "option.png_fast",
        PngCompression::Default => "option.png_default",
        PngCompression::Best => "option.png_best",
    }
}

fn estimate_text(state: EstimateState) -> SharedString {
    match state {
        EstimateState::Unavailable => tr("settings.estimated_size_unavailable"),
        EstimateState::Pending => tr("settings.estimated_size_pending"),
        EstimateState::Ready(bytes) => format!(
            "{}: {}",
            t!("settings.estimated_size"),
            format_file_size(bytes)
        )
        .into(),
        EstimateState::Failed => tr("settings.estimated_size_failed"),
    }
}

fn format_file_size(bytes: usize) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = KIB * 1024.0;
    let bytes = bytes as f64;
    if bytes < KIB {
        format!("{bytes:.0} B")
    } else if bytes < MIB {
        format!("{:.1} KiB", bytes / KIB)
    } else {
        format!("{:.2} MiB", bytes / MIB)
    }
}

fn hsla_to_image_rgba(color: Hsla) -> image::Rgba<u8> {
    let rgba = color.to_rgb();
    let channel = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    image::Rgba([
        channel(rgba.r),
        channel(rgba.g),
        channel(rgba.b),
        channel(rgba.a),
    ])
}

fn parameter_help_key(label_key: &str) -> &'static str {
    match label_key {
        "parameter.aspect_ratio" => "help.aspect_ratio",
        "parameter.output_width" => "help.output_width",
        "parameter.output_height" => "help.output_height",
        "parameter.layout" => "help.layout",
        "parameter.columns" => "help.columns",
        "parameter.spacing" => "help.spacing",
        "parameter.background" => "help.background",
        "parameter.mode" => "help.batch_mode",
        "parameter.format" => "help.format",
        "parameter.quality" => "help.quality",
        "parameter.png_compression" => "help.png_compression",
        "parameter.watermark_source" => "help.watermark_source",
        "parameter.opacity" => "help.opacity",
        "parameter.position" => "help.position",
        "parameter.rows" => "help.rows",
        "parameter.qr_size" => "help.qr_size",
        "parameter.correction" => "help.correction",
        "parameter.corner_radius" => "help.corner_radius",
        "parameter.padding" => "help.padding",
        "parameter.border" => "help.border",
        "parameter.shadow" => "help.shadow",
        "parameter.frame_delay" => "help.frame_delay",
        "parameter.size_mode" => "help.size_mode",
        "parameter.playback" => "help.playback",
        _ => "help.generic",
    }
}

#[cfg(target_os = "windows")]
fn reveal_directory(path: &std::path::Path) -> std::io::Result<()> {
    std::process::Command::new("explorer")
        .arg(path)
        .spawn()
        .map(|_| ())
}

#[cfg(target_os = "macos")]
fn reveal_directory(path: &std::path::Path) -> std::io::Result<()> {
    std::process::Command::new("open")
        .arg(path)
        .spawn()
        .map(|_| ())
}

#[cfg(all(unix, not(target_os = "macos")))]
fn reveal_directory(path: &std::path::Path) -> std::io::Result<()> {
    std::process::Command::new("xdg-open")
        .arg(path)
        .spawn()
        .map(|_| ())
}

fn feature_detail(feature: Feature) -> SharedString {
    match feature {
        Feature::Edit => tr("feature.edit.detail"),
        Feature::Collage => tr("feature.collage.detail"),
        Feature::Batch => tr("feature.batch.detail"),
        Feature::Slice => tr("feature.slice.detail"),
        Feature::QrCode => tr("feature.qrcode.detail"),
        Feature::Exif => tr("feature.exif.detail"),
        Feature::Beautify => tr("feature.beautify.detail"),
        Feature::Gif => tr("feature.gif.detail"),
        _ => SharedString::from("rastery-core"),
    }
}

impl Render for AppShell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // 记录当前客户区尺寸，供预览画布按可用空间自适应缩放。
        let viewport = window.viewport_size();
        self.viewport_width = f32::from(viewport.width);
        self.viewport_height = f32::from(viewport.height);
        let query = self.nav_search.read(cx).value().to_string().to_lowercase();
        let nav_groups = navigation_groups()
            .into_iter()
            .filter_map(|group| self.sidebar_group(group, &query, cx))
            .collect::<Vec<_>>();

        let content = if self.settings_open {
            self.settings_page(cx).into_any_element()
        } else {
            match self.open {
                Some(feature) => self.feature_page(feature, cx).into_any_element(),
                None => self.workspace_home(cx).into_any_element(),
            }
        };

        let secondary = cx.theme().secondary;
        let border = cx.theme().border;
        let background = cx.theme().background;
        let foreground = cx.theme().foreground;
        let sidebar = cx.theme().sidebar;
        let sidebar_border = cx.theme().sidebar_border;
        v_flex()
            .size_full()
            .overflow_hidden()
            .bg(secondary)
            .child(
                h_flex()
                    .h(px(40.0))
                    .flex_shrink_0()
                    .px_3()
                    .gap_4()
                    .items_center()
                    .border_b_1()
                    .border_color(border)
                    .bg(background)
                    .child(
                        div()
                            .size(px(26.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_md()
                            .bg(foreground)
                            .text_color(background)
                            .child(Icon::new(IconName::GalleryVerticalEnd).small()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .overflow_hidden()
                            .child(self.menu_bar_slot()),
                    ),
            )
            .child(
                h_flex()
                    .flex_1()
                    .h(px(0.0))
                    .min_h(px(0.0))
                    .overflow_hidden()
                    .child(
                        v_flex()
                            .w(px(216.0))
                            .h_full()
                            .min_h(px(0.0))
                            .flex_shrink_0()
                            .relative()
                            .bg(sidebar)
                            .border_r_1()
                            .border_color(sidebar_border)
                            .child(
                                v_flex().p_3().child(
                                    Input::new(&self.nav_search)
                                        .prefix(Icon::new(IconName::Search).small())
                                        .cleanable(true),
                                ),
                            )
                            .child(
                                v_flex()
                                    .id("navigation-scroll")
                                    .flex_1()
                                    .h(px(0.0))
                                    .overflow_y_scroll()
                                    .px_3()
                                    .pb_3()
                                    .gap_2()
                                    .children(nav_groups),
                            ),
                    )
                    .child(
                        v_flex()
                            .flex_1()
                            .h_full()
                            .min_h(px(0.0))
                            .overflow_hidden()
                            .child(content),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_prompt_resolver_maps_cancel_and_leaves_workspace_reusable() {
        let mut workspace = Workspace::default();
        let result = classify_path_prompt_result::<PathBuf, (), ()>(Ok(Ok(None)));

        let selected =
            resolve_path_prompt(&mut workspace, result, UiMessage::CancelledOpen, "image");

        assert!(selected.is_none());
        assert_eq!(workspace.status_text(), UiMessage::CancelledOpen.text());
        assert!(!workspace.is_busy());
        assert!(
            workspace
                .prepare_paths(vec![PathBuf::from("photo.png")], false)
                .is_some()
        );
    }

    #[test]
    fn path_prompt_resolver_maps_failures_and_leaves_workspace_reusable() {
        for result in [
            classify_path_prompt_result::<PathBuf, (), ()>(Ok(Err(()))),
            classify_path_prompt_result::<PathBuf, (), ()>(Err(())),
        ] {
            let mut workspace = Workspace::default();

            let selected =
                resolve_path_prompt(&mut workspace, result, UiMessage::CancelledOpen, "image");

            assert!(selected.is_none());
            assert_eq!(
                workspace.status_text(),
                UiMessage::Error(ErrorKind::Io).text()
            );
            assert!(!workspace.is_busy());
            assert!(
                workspace
                    .prepare_paths(vec![PathBuf::from("photo.png")], false)
                    .is_some()
            );
        }
    }

    #[test]
    fn file_size_format_uses_readable_binary_units() {
        assert_eq!(format_file_size(512), "512 B");
        assert_eq!(format_file_size(1536), "1.5 KiB");
        assert_eq!(format_file_size(2 * 1024 * 1024), "2.00 MiB");
    }

    #[test]
    fn default_png_export_uses_persisted_compression() {
        let config = AppConfig {
            default_png_compression: PngCompression::Best,
            ..AppConfig::default()
        };
        assert_eq!(
            default_export_settings(&config),
            EncodeSettings::Png {
                compression: PngCompression::Best,
            }
        );
    }

    #[test]
    fn color_picker_conversion_preserves_opaque_black_and_white() {
        assert_eq!(hsla_to_image_rgba(black()), image::Rgba([0, 0, 0, 255]));
        assert_eq!(
            hsla_to_image_rgba(white()),
            image::Rgba([255, 255, 255, 255])
        );
    }

    #[test]
    fn multiple_initial_paths_open_the_batch_workspace() {
        assert_eq!(initial_paths_feature(1), Feature::Edit);
        assert_eq!(initial_paths_feature(2), Feature::Batch);
        assert_eq!(initial_paths_feature(100), Feature::Batch);
    }

    #[test]
    fn ai_region_mask_is_transparent_only_inside_selection() {
        let input = selection_mask(
            10,
            10,
            NormRect {
                x: 0.2,
                y: 0.3,
                w: 0.4,
                h: 0.2,
            },
        )
        .expect("mask");
        let mask = format::decode(&input.bytes).expect("decode mask");
        assert_eq!(mask.get_pixel(0, 0).0[3], 255);
        assert_eq!(mask.get_pixel(3, 4).0[3], 0);
        assert_eq!(mask.get_pixel(8, 8).0[3], 255);
    }

    #[test]
    fn ai_output_postprocessing_uses_exact_business_dimensions() {
        assert_eq!(
            exact_output_dimensions(Feature::IdPhoto, "one-inch-blue-suit"),
            Some((295, 413))
        );
        assert_eq!(
            exact_output_dimensions(Feature::PromotionalPoster, "new-arrival"),
            Some((1200, 1600))
        );
        assert_eq!(
            exact_output_dimensions(Feature::PlatformAdaptation, "wechat-cover-900x383"),
            Some((900, 383))
        );
    }

    #[test]
    fn local_platform_crop_has_exact_dimensions_without_stretching() {
        let source = rastery_core::RgbaImage::from_pixel(1_000, 1_000, image::Rgba([1, 2, 3, 255]));
        let output = exact_cover(&source, 800, 800).expect("local crop");
        assert_eq!(output.dimensions(), (800, 800));
        assert!(aspect_ratio_is_close(1_000, 1_000, 800, 800));
        assert!(!aspect_ratio_is_close(1_000, 1_000, 900, 383));
    }
}
