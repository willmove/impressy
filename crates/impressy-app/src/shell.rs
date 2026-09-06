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
use gpui_component::input::{Input, InputEvent, InputState, NumberInput, NumberInputEvent, StepAction};
use gpui_component::select::{Select, SelectEvent, SelectState};
use gpui_component::slider::{Slider, SliderEvent, SliderState};
use gpui_component::spinner::Spinner;
use gpui_component::tooltip::Tooltip;
use gpui_component::{
    ActiveTheme, Colorize, Disableable, Icon, IconName, IndexPath, Root, Sizable, WindowExt,
    h_flex, v_flex,
};
use impressy_ai::{
    AiError, ApiKey, AspectRatio as AiAspectRatio, CredentialStore, GeneratedImage,
    GenerationQuality, GenerationRequest, ImageInput, ProviderId, ProviderRegistry,
    SystemCredentialStore,
};
use impressy_core::config::{AppConfig, Language};
use impressy_core::animation::Playback;
use impressy_core::format::{self, EncodeSettings, OutputFormat, PngCompression, Quality};
use impressy_core::qr::ErrorCorrection;
use impressy_core::transform::{AspectRatio, Rotation};
use impressy_presets::{AiEditPreset, IndustryTool};
use rust_i18n::t;

use crate::ai_state::{
    AiErrorKind, AiStatus, AiUiState, ARTICLE_STYLES, ARTICLE_USAGES, COVER_PLATFORMS, COVER_STYLES,
    ID_ATTIRES, ID_BACKGROUNDS, ID_SIZES, RenderedAiImage, TRY_ON_MODELS, TRY_ON_SCENES,
};
use crate::config_store::ConfigStore;
use crate::crop_frame::{NormRect, Selection, selection_overlay};
use crate::feature_params::{
    BatchMode, BeautifyBackground, CollageMode, FREE_RATIO_INDEX, FeatureParams, ParamAction,
    ParamEffect, WatermarkPlacement, WatermarkSource,
};
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

/// 仓库主页（在线文档入口）。仅在用户主动点击「在线文档」时打开，不构成联网依赖。
const IMPRESSY_README_URL: &str = "https://github.com/willmove/impressy#readme";
/// 问题反馈入口。
const IMPRESSY_ISSUES_URL: &str = "https://github.com/willmove/impressy/issues/new";

/// 快捷键对照表里显示的修饰键前缀：macOS 用 ⌘，其余平台用 Ctrl。
fn shortcut_modifier() -> &'static str {
    if cfg!(target_os = "macos") {
        "⌘"
    } else {
        "Ctrl"
    }
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
    images: Arc<Vec<impressy_core::RgbaImage>>,
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

#[derive(Clone, Copy)]
enum NumericField {
    EditWidth,
    EditHeight,
    CollageColumns,
    CollageSpacing,
    BatchWidth,
    BatchHeight,
    BatchOpacity,
    SliceRows,
    SliceColumns,
    QrSize,
    BeautifyRadius,
    BeautifyPadding,
    BeautifyBorder,
    GifDelay,
    GifWidth,
    GifHeight,
}

impl NumericField {
    const ALL: [Self; 16] = [
        Self::EditWidth,
        Self::EditHeight,
        Self::CollageColumns,
        Self::CollageSpacing,
        Self::BatchWidth,
        Self::BatchHeight,
        Self::BatchOpacity,
        Self::SliceRows,
        Self::SliceColumns,
        Self::QrSize,
        Self::BeautifyRadius,
        Self::BeautifyPadding,
        Self::BeautifyBorder,
        Self::GifDelay,
        Self::GifWidth,
        Self::GifHeight,
    ];

    fn step(self) -> u32 {
        match self {
            Self::EditWidth
            | Self::EditHeight
            | Self::BatchWidth
            | Self::BatchHeight
            | Self::GifWidth
            | Self::GifHeight => 10,
            Self::QrSize => 64,
            Self::CollageSpacing | Self::BeautifyRadius => 4,
            Self::BeautifyPadding => 8,
            Self::GifDelay => 50,
            Self::BatchOpacity => 5,
            Self::CollageColumns
            | Self::SliceRows
            | Self::SliceColumns
            | Self::BeautifyBorder => 1,
        }
    }

    fn current(self, params: &FeatureParams) -> u32 {
        match self {
            Self::EditWidth => params.edit.width,
            Self::EditHeight => params.edit.height,
            Self::CollageColumns => params.collage.columns,
            Self::CollageSpacing => params.collage.spacing,
            Self::BatchWidth => params.batch.width,
            Self::BatchHeight => params.batch.height,
            Self::BatchOpacity => u32::from(params.batch.watermark_opacity_percent),
            Self::SliceRows => params.slice.rows,
            Self::SliceColumns => params.slice.columns,
            Self::QrSize => params.qr.size,
            Self::BeautifyRadius => params.beautify.radius,
            Self::BeautifyPadding => params.beautify.padding,
            Self::BeautifyBorder => params.beautify.border_width,
            Self::GifDelay => params.gif.delay_ms,
            Self::GifWidth => params.gif.width,
            Self::GifHeight => params.gif.height,
        }
    }

    fn action(self, value: u32) -> ParamAction {
        match self {
            Self::EditWidth => ParamAction::SetEditWidth(value),
            Self::EditHeight => ParamAction::SetEditHeight(value),
            Self::CollageColumns => ParamAction::SetCollageColumns(value),
            Self::CollageSpacing => ParamAction::SetCollageSpacing(value),
            Self::BatchWidth => ParamAction::SetBatchWidth(value),
            Self::BatchHeight => ParamAction::SetBatchHeight(value),
            Self::BatchOpacity => ParamAction::SetBatchOpacity(value as u8),
            Self::SliceRows => ParamAction::SetSliceRows(value),
            Self::SliceColumns => ParamAction::SetSliceColumns(value),
            Self::QrSize => ParamAction::SetQrSize(value),
            Self::BeautifyRadius => ParamAction::SetBeautifyRadius(value),
            Self::BeautifyPadding => ParamAction::SetBeautifyPadding(value),
            Self::BeautifyBorder => ParamAction::SetBeautifyBorder(value),
            Self::GifDelay => ParamAction::SetGifDelay(value),
            Self::GifWidth => ParamAction::SetGifWidth(value),
            Self::GifHeight => ParamAction::SetGifHeight(value),
        }
    }
}

struct NumberFields {
    edit_width: Entity<InputState>,
    edit_height: Entity<InputState>,
    collage_columns: Entity<InputState>,
    collage_spacing: Entity<InputState>,
    batch_width: Entity<InputState>,
    batch_height: Entity<InputState>,
    batch_opacity: Entity<InputState>,
    slice_rows: Entity<InputState>,
    slice_columns: Entity<InputState>,
    qr_size: Entity<InputState>,
    beautify_radius: Entity<InputState>,
    beautify_padding: Entity<InputState>,
    beautify_border: Entity<InputState>,
    gif_delay: Entity<InputState>,
    gif_width: Entity<InputState>,
    gif_height: Entity<InputState>,
}

impl NumberFields {
    fn new(params: &FeatureParams, window: &mut Window, cx: &mut Context<AppShell>) -> Self {
        Self {
            edit_width: number_input(params.edit.width, window, cx),
            edit_height: number_input(params.edit.height, window, cx),
            collage_columns: number_input(params.collage.columns, window, cx),
            collage_spacing: number_input(params.collage.spacing, window, cx),
            batch_width: number_input(params.batch.width, window, cx),
            batch_height: number_input(params.batch.height, window, cx),
            batch_opacity: number_input(u32::from(params.batch.watermark_opacity_percent), window, cx),
            slice_rows: number_input(params.slice.rows, window, cx),
            slice_columns: number_input(params.slice.columns, window, cx),
            qr_size: number_input(params.qr.size, window, cx),
            beautify_radius: number_input(params.beautify.radius, window, cx),
            beautify_padding: number_input(params.beautify.padding, window, cx),
            beautify_border: number_input(params.beautify.border_width, window, cx),
            gif_delay: number_input(params.gif.delay_ms, window, cx),
            gif_width: number_input(params.gif.width, window, cx),
            gif_height: number_input(params.gif.height, window, cx),
        }
    }

    fn get(&self, field: NumericField) -> &Entity<InputState> {
        match field {
            NumericField::EditWidth => &self.edit_width,
            NumericField::EditHeight => &self.edit_height,
            NumericField::CollageColumns => &self.collage_columns,
            NumericField::CollageSpacing => &self.collage_spacing,
            NumericField::BatchWidth => &self.batch_width,
            NumericField::BatchHeight => &self.batch_height,
            NumericField::BatchOpacity => &self.batch_opacity,
            NumericField::SliceRows => &self.slice_rows,
            NumericField::SliceColumns => &self.slice_columns,
            NumericField::QrSize => &self.qr_size,
            NumericField::BeautifyRadius => &self.beautify_radius,
            NumericField::BeautifyPadding => &self.beautify_padding,
            NumericField::BeautifyBorder => &self.beautify_border,
            NumericField::GifDelay => &self.gif_delay,
            NumericField::GifWidth => &self.gif_width,
            NumericField::GifHeight => &self.gif_height,
        }
    }
}

fn number_input(value: u32, window: &mut Window, cx: &mut Context<AppShell>) -> Entity<InputState> {
    cx.new(|cx| InputState::new(window, cx).default_value(value.to_string()))
}

/// GPUI 0.2.2 的 `flex_wrap` 换行后经常不计入父级高度，后面的控件会叠在按钮上。
/// 这里改成固定每行列数的 `h_flex` 行，高度由行数决定。
fn chip_flow(
    chips: impl IntoIterator<Item = AnyElement>,
    per_row: usize,
    stretch: bool,
) -> AnyElement {
    let chips: Vec<AnyElement> = chips.into_iter().collect();
    if chips.is_empty() {
        return div().into_any_element();
    }
    let per_row = per_row.max(1);
    let mut rows = Vec::new();
    let mut row = Vec::new();
    for chip in chips {
        let cell = if stretch {
            div().flex_1().child(chip).into_any_element()
        } else {
            chip
        };
        row.push(cell);
        if row.len() == per_row {
            rows.push(
                h_flex()
                    .w_full()
                    .gap_2()
                    .children(std::mem::take(&mut row))
                    .into_any_element(),
            );
        }
    }
    if !row.is_empty() {
        rows.push(
            h_flex()
                .w_full()
                .gap_2()
                .children(row)
                .into_any_element(),
        );
    }
    v_flex()
        .w_full()
        .gap_2()
        .children(rows)
        .into_any_element()
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
    numbers: NumberFields,
    estimate_state: EstimateState,
    estimate_generation: Arc<AtomicU64>,
    /// AI 生成代际：每次发起生成自增。完成/保存回调据此丢弃过期代际的结果，
    /// 防止跨功能页结果污染与 history 记录张冠李戴。
    ai_generation: Arc<AtomicU64>,
    /// 上一帧窗口客户区尺寸（逻辑像素）。渲染时更新，供预览画布随窗口伸缩计算可用空间。
    viewport_width: f32,
    viewport_height: f32,
    /// 侧边栏中已收起的导航分组，用分组的 `label_key` 标识；默认全部展开（空集）。
    collapsed_groups: std::collections::HashSet<&'static str>,
    /// 左侧导航侧栏是否显示；默认展开。隐藏时不渲染侧栏子树，内部状态保留以便恢复。
    sidebar_open: bool,
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
        let crop_selection = cx.new(|_| Selection::new(params.edit.ratio()));
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
        let numbers = NumberFields::new(&params, window, cx);

        let mut _subscriptions = vec![
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
        for field in NumericField::ALL {
            let entity = numbers.get(field).clone();
            _subscriptions.push(cx.subscribe_in(&entity, window, {
                move |this, state, event: &InputEvent, window, cx| {
                    this.on_numeric_input(field, state, event, window, cx);
                }
            }));
            _subscriptions.push(cx.subscribe_in(&entity, window, {
                move |this, state, event: &NumberInputEvent, window, cx| {
                    this.on_numeric_step(field, state, event, window, cx);
                }
            }));
        }
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
            numbers,
            estimate_state: EstimateState::Unavailable,
            estimate_generation: Arc::new(AtomicU64::new(0)),
            ai_generation: Arc::new(AtomicU64::new(0)),
            // 初始沿用窗口请求尺寸；首帧渲染即被真实客户区尺寸覆盖。
            viewport_width: 1280.0,
            viewport_height: 800.0,
            collapsed_groups: std::collections::HashSet::new(),
            sidebar_open: true,
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
        // 菜单文案随语言重建，并带上当前侧栏显隐以保持 hide/show 文案正确。
        cx.set_menus(crate::menus::build_menus(self.sidebar_open));
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
        let changed = self.open != Some(feature);
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
                selection.set_free_region(NormRect::centered_fraction(0.32), selection_cx);
            });
        }
        if changed {
            // 切换功能页时清理上一页的选中态与状态文案：结果按 `results_feature`
            // 只在所属页面展示；进行中的生成保留 Generating（按钮保持禁用），
            // 由完成回调决定结果的归宿。
            self.ai_state.selected_results.clear();
            if self.ai_state.results_feature == Some(feature) && !self.ai_state.results.is_empty()
            {
                self.ai_state.status = AiStatus::Ready(self.ai_state.results.len());
            } else if !matches!(self.ai_state.status, AiStatus::Generating) {
                self.ai_state.status = AiStatus::Idle;
            }
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
                let paths = paths.paths().to_vec();
                let supported = paths
                    .iter()
                    .filter(|path| is_supported_image_path(path))
                    .count();
                this.open = Some(initial_paths_feature(supported));
                this.active = Section::BasicImage;
                this.handle_drop(paths, cx);
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

    pub(crate) fn menu_toggle_sidebar(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.sidebar_open = !self.sidebar_open;
        cx.set_menus(crate::menus::build_menus(self.sidebar_open));
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
                    .child(div().text_sm().child(tr("app.subtitle")))
                    .child(div().text_sm().child(format!(
                        "{}: Apache-2.0",
                        tr("menu.about_license")
                    )))
                    .child(div().text_sm().child(format!(
                        "{}: {IMPRESSY_README_URL}",
                        tr("menu.about_repo")
                    )))
                    .child(div().text_sm().child(format!(
                        "{}: GPUI · gpui-component · image · rust-i18n",
                        tr("menu.about_built_with")
                    ))),
            )
        });
    }

    pub(crate) fn menu_show_shortcuts(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let modifier = shortcut_modifier();
        // （操作名, 键位）对照：键位用平台修饰符前缀。文案复用 menu.* 键。
        let rows: [(SharedString, String); 11] = [
            (tr("menu.open"), format!("{modifier}+O")),
            (tr("menu.open_multi"), format!("{modifier}+Shift+O")),
            (tr("menu.save"), format!("{modifier}+S")),
            (tr("menu.reveal_output"), format!("{modifier}+Shift+R")),
            (tr("menu.paste"), format!("{modifier}+V")),
            (tr("menu.copy"), format!("{modifier}+Shift+C")),
            (tr("menu.settings"), format!("{modifier}+,")),
            (tr("menu.quit"), format!("{modifier}+Q")),
            (tr("menu.home"), format!("{modifier}+Shift+H")),
            (tr("menu.hide_sidebar"), format!("{modifier}+B")),
            (tr("menu.switch_language"), format!("{modifier}+Shift+L")),
        ];
        window.open_dialog(cx, move |dialog, _, _| {
            let mut body = v_flex().gap_1().child(
                h_flex()
                    .gap_4()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(tr("menu.kbd_action")),
                    )
                    .child(
                        div()
                            .text_sm()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(tr("menu.kbd_shortcut")),
                    ),
            );
            for (name, keys) in &rows {
                body = body.child(
                    h_flex()
                        .gap_4()
                        .child(div().text_sm().child(name.clone()))
                        .child(div().text_sm().child(keys.clone())),
                );
            }
            dialog.title(tr("menu.shortcuts")).child(body)
        });
    }

    pub(crate) fn menu_open_docs(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        cx.open_url(IMPRESSY_README_URL);
    }

    pub(crate) fn menu_open_issue_tracker(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        cx.open_url(IMPRESSY_ISSUES_URL);
    }

    pub(crate) fn menu_goto_basic_image(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.open_feature(Feature::Edit, cx);
    }

    pub(crate) fn menu_goto_ai_generation(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.open_feature(Feature::TextToImage, cx);
    }

    pub(crate) fn menu_goto_industry_tools(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_feature(Feature::OldPhotoRestoration, cx);
    }

    pub(crate) fn menu_goto_creative_output(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // 创作输出板块：选 GIF 作为代表入口（其 Section 归属为 CreativeOutput，
        // 能让侧栏正确高亮该板块；Collage 虽列在创作输出分组，但 Section 归属为
        // BasicImage，跳它会让侧栏高亮错位）。
        self.open_feature(Feature::Gif, cx);
    }

    fn adjust_param(&mut self, action: ParamAction, cx: &mut Context<Self>) {
        match self.params.apply(action) {
            ParamEffect::CropRatioChanged => {
                let ratio = self.params.edit.ratio();
                self.crop_selection.update(cx, |selection, selection_cx| {
                    selection.set_ratio(ratio, selection_cx);
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

    fn on_numeric_input(
        &mut self,
        field: NumericField,
        state: &Entity<InputState>,
        event: &InputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            InputEvent::Change => {
                if let Ok(value) = state.read(cx).value().parse::<u32>() {
                    self.adjust_param(field.action(value), cx);
                }
            }
            InputEvent::Blur | InputEvent::PressEnter { .. } => {
                let value = field.current(&self.params);
                state.update(cx, |input, input_cx| {
                    input.set_value(value.to_string(), window, input_cx);
                });
                cx.notify();
            }
            InputEvent::Focus => {}
        }
    }

    fn on_numeric_step(
        &mut self,
        field: NumericField,
        state: &Entity<InputState>,
        event: &NumberInputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let NumberInputEvent::Step(step) = event;
        let current = field.current(&self.params);
        let next = match step {
            StepAction::Increment => current.saturating_add(field.step()),
            StepAction::Decrement => current.saturating_sub(field.step()),
        };
        self.adjust_param(field.action(next), cx);
        let value = field.current(&self.params);
        state.update(cx, |input, input_cx| {
            input.set_value(value.to_string(), window, input_cx);
        });
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

    fn param_chip(
        &self,
        id: impl Into<SharedString>,
        label: impl Into<SharedString>,
        selected: bool,
        action: ParamAction,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        Button::new(id.into())
            .small()
            .w_full()
            .label(label.into())
            .when(selected, |button| button.primary())
            .when(!selected, |button| button.outline())
            .on_click(cx.listener(move |this, _, _, cx| {
                this.adjust_param(action, cx);
            }))
            .into_any_element()
    }

    fn chip_row(
        &self,
        label_key: &str,
        chips: impl IntoIterator<Item = AnyElement>,
    ) -> AnyElement {
        let help = tr(parameter_help_key(label_key));
        v_flex()
            .w_full()
            .gap_1()
            .child(
                div()
                    .id(SharedString::from(format!("{label_key}-label")))
                    .text_sm()
                    .tooltip(move |window, cx| Tooltip::new(help.clone()).build(window, cx))
                    .child(tr(label_key)),
            )
            .child(chip_flow(chips, 2, true))
            .into_any_element()
    }

    fn number_control(
        &self,
        label_key: &str,
        field: &Entity<InputState>,
        suffix: &'static str,
    ) -> AnyElement {
        let help = tr(parameter_help_key(label_key));
        h_flex()
            .w_full()
            .justify_between()
            .gap_3()
            .child(div().text_sm().child(tr(label_key)))
            .child(
                h_flex()
                    .w(px(148.0))
                    .gap_1()
                    .child(
                        div()
                            .id(SharedString::from(format!("{label_key}-number")))
                            .flex_1()
                            .tooltip(move |window, cx| Tooltip::new(help.clone()).build(window, cx))
                            .child(NumberInput::new(field).small()),
                    )
                    .when(!suffix.is_empty(), |this| {
                        this.child(div().text_xs().child(suffix))
                    }),
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
                let selected = self.params.edit.ratio_index;
                let mut ratio_chips = AspectRatio::PRESETS
                    .iter()
                    .enumerate()
                    .map(|(index, ratio)| {
                        self.param_chip(
                            format!("chip-edit-ratio-{index}"),
                            ratio.to_string(),
                            selected == index,
                            ParamAction::SetEditRatio(index),
                            cx,
                        )
                    })
                    .collect::<Vec<_>>();
                ratio_chips.push(self.param_chip(
                    "chip-edit-ratio-free",
                    tr("option.free_ratio"),
                    selected == FREE_RATIO_INDEX,
                    ParamAction::SetEditRatio(FREE_RATIO_INDEX),
                    cx,
                ));
                controls.push(self.chip_row("parameter.aspect_ratio", ratio_chips));
                controls.push(self.number_control(
                    "parameter.output_width",
                    &self.numbers.edit_width,
                    "px",
                ));
                controls.push(self.number_control(
                    "parameter.output_height",
                    &self.numbers.edit_height,
                    "px",
                ));
            }
            Feature::Collage => {
                let mode = self.params.collage.mode;
                controls.push(self.chip_row(
                    "parameter.layout",
                    [
                        (CollageMode::Vertical, "chip-collage-vertical"),
                        (CollageMode::Horizontal, "chip-collage-horizontal"),
                        (CollageMode::Grid, "chip-collage-grid"),
                    ]
                    .into_iter()
                    .map(|(candidate, id)| {
                        self.param_chip(
                            id,
                            tr(candidate.label_key()),
                            mode == candidate,
                            ParamAction::SetCollageMode(candidate),
                            cx,
                        )
                    }),
                ));
                if mode == CollageMode::Grid {
                    controls.push(self.number_control(
                        "parameter.columns",
                        &self.numbers.collage_columns,
                        "",
                    ));
                }
                controls.push(self.number_control(
                    "parameter.spacing",
                    &self.numbers.collage_spacing,
                    "px",
                ));
                let background = self.params.collage.background_index;
                controls.push(self.chip_row(
                    "parameter.background",
                    (0..4).map(|index| {
                        let label = match index {
                            0 => "option.white",
                            1 => "option.black",
                            2 => "option.gray",
                            _ => "option.transparent",
                        };
                        self.param_chip(
                            format!("chip-collage-bg-{index}"),
                            tr(label),
                            background == index,
                            ParamAction::SetCollageBackground(index),
                            cx,
                        )
                    }),
                ));
            }
            Feature::Batch => self.batch_controls(&mut controls, cx),
            Feature::Slice => {
                controls.push(self.number_control("parameter.rows", &self.numbers.slice_rows, ""));
                controls.push(self.number_control(
                    "parameter.columns",
                    &self.numbers.slice_columns,
                    "",
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
                controls.push(self.number_control(
                    "parameter.qr_size",
                    &self.numbers.qr_size,
                    "px",
                ));
                let correction = self.params.qr.correction;
                controls.push(self.chip_row(
                    "parameter.correction",
                    [
                        ErrorCorrection::Low,
                        ErrorCorrection::Medium,
                        ErrorCorrection::Quartile,
                        ErrorCorrection::High,
                    ]
                    .into_iter()
                    .map(|candidate| {
                        self.param_chip(
                            format!("chip-qr-correction-{candidate:?}"),
                            tr(match candidate {
                                ErrorCorrection::Low => "option.correction_low",
                                ErrorCorrection::Medium => "option.correction_medium",
                                ErrorCorrection::Quartile => "option.correction_quartile",
                                ErrorCorrection::High => "option.correction_high",
                            }),
                            correction == candidate,
                            ParamAction::SetQrCorrection(candidate),
                            cx,
                        )
                    }),
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
        controls.push(self.chip_row(
            "parameter.mode",
            [
                BatchMode::Convert,
                BatchMode::Compress,
                BatchMode::Resize,
                BatchMode::Watermark,
            ]
            .into_iter()
            .map(|candidate| {
                self.param_chip(
                    format!("chip-batch-mode-{candidate:?}"),
                    tr(candidate.label_key()),
                    params.mode == candidate,
                    ParamAction::SetBatchMode(candidate),
                    cx,
                )
            }),
        ));
        match params.mode {
            BatchMode::Convert | BatchMode::Compress => {
                let formats = if params.mode == BatchMode::Compress {
                    vec![OutputFormat::Jpeg, OutputFormat::Webp]
                } else {
                    vec![OutputFormat::Png, OutputFormat::Jpeg, OutputFormat::Webp]
                };
                controls.push(self.chip_row(
                    "parameter.format",
                    formats.into_iter().map(|output_format| {
                        self.param_chip(
                            format!("chip-batch-format-{output_format}"),
                            output_format.to_string(),
                            params.format == output_format,
                            ParamAction::SetBatchFormat(output_format),
                            cx,
                        )
                    }),
                ));
                if params.format == OutputFormat::Png {
                    controls.push(self.chip_row(
                        "parameter.png_compression",
                        [
                            PngCompression::Fast,
                            PngCompression::Default,
                            PngCompression::Best,
                        ]
                        .into_iter()
                        .map(|candidate| {
                            self.param_chip(
                                format!("chip-batch-png-{candidate:?}"),
                                tr(png_compression_label_key(candidate)),
                                params.png_compression == candidate,
                                ParamAction::SetBatchPngCompression(candidate),
                                cx,
                            )
                        }),
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
                controls.push(self.number_control(
                    "parameter.output_width",
                    &self.numbers.batch_width,
                    "px",
                ));
                controls.push(self.number_control(
                    "parameter.output_height",
                    &self.numbers.batch_height,
                    "px",
                ));
            }
            BatchMode::Watermark => {
                controls.push(self.chip_row(
                    "parameter.watermark_source",
                    [WatermarkSource::Text, WatermarkSource::Image]
                        .into_iter()
                        .map(|candidate| {
                            self.param_chip(
                                format!("chip-batch-wm-{candidate:?}"),
                                tr(candidate.label_key()),
                                params.watermark_source == candidate,
                                ParamAction::SetBatchWatermarkSource(candidate),
                                cx,
                            )
                        }),
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
                controls.push(self.number_control(
                    "parameter.opacity",
                    &self.numbers.batch_opacity,
                    "%",
                ));
                controls.push(self.chip_row(
                    "parameter.position",
                    [WatermarkPlacement::BottomRight, WatermarkPlacement::Tiled]
                        .into_iter()
                        .map(|candidate| {
                            self.param_chip(
                                format!("chip-batch-pos-{candidate:?}"),
                                tr(candidate.label_key()),
                                params.watermark_placement == candidate,
                                ParamAction::SetBatchPosition(candidate),
                                cx,
                            )
                        }),
                ));
            }
        }
    }

    fn beautify_controls(&self, controls: &mut Vec<AnyElement>, cx: &mut Context<Self>) {
        let params = self.params.beautify;
        controls.push(self.number_control(
            "parameter.corner_radius",
            &self.numbers.beautify_radius,
            "px",
        ));
        controls.push(self.number_control(
            "parameter.padding",
            &self.numbers.beautify_padding,
            "px",
        ));
        controls.push(self.chip_row(
            "parameter.background",
            [
                BeautifyBackground::Gradient,
                BeautifyBackground::Solid,
                BeautifyBackground::Transparent,
            ]
            .into_iter()
            .map(|candidate| {
                self.param_chip(
                    format!("chip-beautify-bg-{candidate:?}"),
                    tr(candidate.label_key()),
                    params.background == candidate,
                    ParamAction::SetBeautifyBackground(candidate),
                    cx,
                )
            }),
        ));
        controls.push(self.number_control(
            "parameter.border",
            &self.numbers.beautify_border,
            "px",
        ));
        controls.push(self.chip_row(
            "parameter.shadow",
            [true, false].into_iter().map(|enabled| {
                self.param_chip(
                    format!("chip-beautify-shadow-{enabled}"),
                    tr(if enabled {
                        "option.enabled"
                    } else {
                        "option.disabled"
                    }),
                    params.shadow == enabled,
                    ParamAction::SetBeautifyShadow(enabled),
                    cx,
                )
            }),
        ));
    }

    fn gif_controls(&self, controls: &mut Vec<AnyElement>, cx: &mut Context<Self>) {
        let params = self.params.gif;
        controls.push(self.number_control(
            "parameter.frame_delay",
            &self.numbers.gif_delay,
            "ms",
        ));
        controls.push(self.chip_row(
            "parameter.size_mode",
            [false, true].into_iter().map(|custom| {
                self.param_chip(
                    format!("chip-gif-size-{custom}"),
                    tr(if custom {
                        "option.custom_size"
                    } else {
                        "option.original_size"
                    }),
                    params.custom_size == custom,
                    ParamAction::SetGifCustomSize(custom),
                    cx,
                )
            }),
        ));
        if params.custom_size {
            controls.push(self.number_control(
                "parameter.output_width",
                &self.numbers.gif_width,
                "px",
            ));
            controls.push(self.number_control(
                "parameter.output_height",
                &self.numbers.gif_height,
                "px",
            ));
        }
        controls.push(self.chip_row(
            "parameter.playback",
            [Playback::Forward, Playback::Reverse, Playback::PingPong]
                .into_iter()
                .map(|candidate| {
                    self.param_chip(
                        format!("chip-gif-playback-{candidate:?}"),
                        tr(match candidate {
                            Playback::Forward => "option.forward",
                            Playback::Reverse => "option.reverse",
                            Playback::PingPong => "option.ping_pong",
                        }),
                        params.playback == candidate,
                        ParamAction::SetGifPlayback(candidate),
                        cx,
                    )
                }),
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
                command!(
                    "act-rotate-180",
                    "action.rotate180",
                    WorkspaceCommand::Transform(TransformOperation::Rotate(Rotation::Cw180)),
                    false,
                );
                command!(
                    "act-rotate-270",
                    "action.rotate270",
                    WorkspaceCommand::Transform(TransformOperation::Rotate(Rotation::Cw270)),
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

    fn image_strip(&self, feature: Feature, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !matches!(feature, Feature::Collage | Feature::Batch | Feature::Gif) {
            return None;
        }
        let count = self.workspace.image_count();
        if count == 0 {
            return None;
        }
        let busy = self.workspace.is_busy();
        let focus = self.workspace.focus_index();
        let border = cx.theme().border;
        let primary = cx.theme().primary;
        let items = (0..count).map(|index| {
            let selected = index == focus;
            let name = self
                .workspace
                .source_name(index)
                .unwrap_or("—")
                .to_string();
            let short_name = if name.chars().count() > 14 {
                format!("{}…", name.chars().take(12).collect::<String>())
            } else {
                name
            };
            let thumb = self.workspace.thumb(index);
            v_flex()
                .id(SharedString::from(format!("strip-item-{index}")))
                .w(px(96.0))
                .gap_1()
                .p_1()
                .rounded_md()
                .border_1()
                .border_color(if selected { primary } else { border })
                .when_some(thumb, |this, thumb| {
                    this.child(
                        img(thumb)
                            .w(px(88.0))
                            .h(px(64.0)),
                    )
                })
                .child(
                    div()
                        .text_xs()
                        .truncate()
                        .child(SharedString::from(short_name)),
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.workspace.set_focus(index);
                    cx.notify();
                }))
                .child(
                    h_flex()
                        .gap_1()
                        .child(
                            Button::new(SharedString::from(format!("strip-left-{index}")))
                                .outline()
                                .small()
                                .icon(IconName::ArrowLeft)
                                .tooltip(tr("workspace.strip.move_left"))
                                .disabled(busy || index == 0)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.workspace.move_image(index, index.saturating_sub(1));
                                    cx.notify();
                                })),
                        )
                        .child(
                            Button::new(SharedString::from(format!("strip-right-{index}")))
                                .outline()
                                .small()
                                .icon(IconName::ChevronRight)
                                .tooltip(tr("workspace.strip.move_right"))
                                .disabled(busy || index + 1 >= count)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.workspace.move_image(index, index + 1);
                                    cx.notify();
                                })),
                        )
                        .child(
                            Button::new(SharedString::from(format!("strip-remove-{index}")))
                                .outline()
                                .small()
                                .icon(IconName::Delete)
                                .tooltip(tr("workspace.strip.remove"))
                                .disabled(busy)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.workspace.remove_image(index);
                                    cx.notify();
                                })),
                        ),
                )
                .into_any_element()
        });
        Some(
            v_flex()
                .gap_2()
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(tr("workspace.strip.title")),
                )
                .child(chip_flow(items, 5, false))
                .into_any_element(),
        )
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
            .when_some(self.image_strip(feature, cx), |this, strip| this.child(strip))
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
        self.set_ai_provider(self.ai_state.provider, cx);
    }

    fn set_ai_provider(&mut self, provider: ProviderId, cx: &mut Context<Self>) {
        self.ai_state.provider = provider;
        if let Some(capabilities) = self.provider_registry.capabilities(provider) {
            self.ai_state.apply_capabilities(capabilities);
        }
        self.config.default_provider = provider.as_str().to_string();
        self.persist_config(false);
        cx.notify();
    }

    fn select_edit_preset(&mut self, index: usize, cx: &mut Context<Self>) {
        self.ai_state.set_edit_preset(index);
        if AiEditPreset::ALL.get(self.ai_state.edit_preset_index) == Some(&AiEditPreset::Removal)
            && self.crop_selection.read(cx).rect == NormRect::FULL
        {
            self.crop_selection.update(cx, |selection, selection_cx| {
                selection.set_free_region(NormRect::centered_fraction(0.32), selection_cx);
            });
        }
        cx.notify();
    }

    fn labeled_ai_chips(
        &self,
        label_key: &'static str,
        chips: impl IntoIterator<Item = AnyElement>,
    ) -> AnyElement {
        v_flex()
            .w_full()
            .gap_1()
            .child(div().text_sm().child(tr(label_key)))
            .child(chip_flow(chips, 2, true))
            .into_any_element()
    }

    fn selectable_ai_chip(
        &self,
        id: impl Into<SharedString>,
        label: impl Into<SharedString>,
        selected: bool,
        disabled: bool,
        cx: &mut Context<Self>,
        on_click: impl Fn(&mut Self, &mut Context<Self>) + 'static,
    ) -> AnyElement {
        Button::new(id.into())
            .small()
            .w_full()
            .label(label.into())
            .when(selected, |button| button.primary())
            .when(!selected, |button| button.outline())
            .disabled(disabled)
            .on_click(cx.listener(move |this, _, _, cx| on_click(this, cx)))
            .into_any_element()
    }

    fn industry_axis_rows(
        &self,
        feature: Feature,
        busy: bool,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        match feature {
            Feature::IdPhoto => vec![
                self.axis_index_row(
                    AxisRow {
                        label_key: "ai.axis.size",
                        id_prefix: "ai-id-size",
                        items: ID_SIZES,
                        selected: self.ai_state.id_size,
                        set_index: |state, index| state.id_size = index,
                    },
                    busy,
                    cx,
                ),
                self.axis_index_row(
                    AxisRow {
                        label_key: "ai.axis.background",
                        id_prefix: "ai-id-bg",
                        items: ID_BACKGROUNDS,
                        selected: self.ai_state.id_background,
                        set_index: |state, index| state.id_background = index,
                    },
                    busy,
                    cx,
                ),
                self.axis_index_row(
                    AxisRow {
                        label_key: "ai.axis.attire",
                        id_prefix: "ai-id-attire",
                        items: ID_ATTIRES,
                        selected: self.ai_state.id_attire,
                        set_index: |state, index| state.id_attire = index,
                    },
                    busy,
                    cx,
                ),
            ],
            Feature::ModelTryOn => vec![
                self.axis_index_row(
                    AxisRow {
                        label_key: "ai.axis.model",
                        id_prefix: "ai-tryon-model",
                        items: TRY_ON_MODELS,
                        selected: self.ai_state.try_on_model,
                        set_index: |state, index| state.try_on_model = index,
                    },
                    busy,
                    cx,
                ),
                self.axis_index_row(
                    AxisRow {
                        label_key: "ai.axis.scene",
                        id_prefix: "ai-tryon-scene",
                        items: TRY_ON_SCENES,
                        selected: self.ai_state.try_on_scene,
                        set_index: |state, index| state.try_on_scene = index,
                    },
                    busy,
                    cx,
                ),
            ],
            Feature::CoverFactory => vec![
                self.axis_index_row(
                    AxisRow {
                        label_key: "ai.axis.platform",
                        id_prefix: "ai-cover-platform",
                        items: COVER_PLATFORMS,
                        selected: self.ai_state.cover_platform,
                        set_index: |state, index| state.cover_platform = index,
                    },
                    busy,
                    cx,
                ),
                self.axis_index_row(
                    AxisRow {
                        label_key: "ai.axis.style",
                        id_prefix: "ai-cover-style",
                        items: COVER_STYLES,
                        selected: self.ai_state.cover_style,
                        set_index: |state, index| state.cover_style = index,
                    },
                    busy,
                    cx,
                ),
            ],
            Feature::ArticleIllustration => vec![
                self.axis_index_row(
                    AxisRow {
                        label_key: "ai.axis.usage",
                        id_prefix: "ai-article-usage",
                        items: ARTICLE_USAGES,
                        selected: self.ai_state.article_usage,
                        set_index: |state, index| state.article_usage = index,
                    },
                    busy,
                    cx,
                ),
                self.axis_index_row(
                    AxisRow {
                        label_key: "ai.axis.style",
                        id_prefix: "ai-article-style",
                        items: ARTICLE_STYLES,
                        selected: self.ai_state.article_style,
                        set_index: |state, index| state.article_style = index,
                    },
                    busy,
                    cx,
                ),
            ],
            _ => Vec::new(),
        }
    }

    fn axis_index_row(&self, row: AxisRow, busy: bool, cx: &mut Context<Self>) -> AnyElement {
        self.labeled_ai_chips(
            row.label_key,
            row.items.iter().enumerate().map(|(index, id)| {
                self.selectable_ai_chip(
                    format!("{}-{index}", row.id_prefix),
                    tr(&format!("ai.tier.{id}")),
                    row.selected == index,
                    busy,
                    cx,
                    move |this, cx| {
                        (row.set_index)(&mut this.ai_state, index);
                        cx.notify();
                    },
                )
            }),
        )
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
            let prompt = impressy_presets::render_edit_prompt(preset, preset.id(), &instruction)
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
                prompt: impressy_presets::render_poster_background_prompt(tier, &instruction),
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
            if let Some(tier) = self.ai_state.composed_tier(feature) {
                return Ok(vec![AiPromptTask {
                    tier_id: tier.clone(),
                    prompt: impressy_presets::render_industry_prompt(tool, &tier, &instruction),
                }]);
            }
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
                        prompt: impressy_presets::render_industry_prompt(tool, tier, &instruction),
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
            let (rect, edited) = {
                let selection = self.crop_selection.read(cx);
                (selection.rect, selection.has_been_edited())
            };
            // 默认选区覆盖整张图：不框选直接生成会把整图当作「待移除区域」发给
            // Provider。要求先显式框选，避免误烧额度与错误输出。
            if rect == NormRect::FULL && !edited {
                self.ai_state.status = AiStatus::Error(AiErrorKind::RegionNotSelected);
                cx.notify();
                return;
            }
            Some(rect)
        } else {
            None
        };
        let registry = self.provider_registry.clone();
        let credentials = Arc::clone(&self.credential_store);
        // 代际令牌：完成回调只有在「仍在发起功能页且无更新的生成」时才应用结果。
        let generation = self
            .ai_generation
            .fetch_add(1, Ordering::Relaxed)
            .wrapping_add(1);
        self.ai_state.status = AiStatus::Generating;
        self.ai_state.results.clear();
        self.ai_state.selected_results.clear();
        self.ai_state.results_feature = None;

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
                        // Provider 调用已完成并消耗额度：无论用户是否还在该功能页，
                        // 历史记录都如实落盘（不含提示词与密钥）。
                        let count = results.len();
                        this.config.generation_history.push(
                            impressy_core::config::GenerationHistoryRecord {
                                created_at_unix_ms: unix_time_ms(),
                                provider: provider.as_str().to_string(),
                                feature: feature.id().to_string(),
                                output_count: count,
                                saved_paths: Vec::new(),
                            },
                        );
                        this.persist_config(false);
                        if this.open == Some(feature)
                            && this.ai_generation.load(Ordering::Relaxed) == generation
                        {
                            this.ai_state.set_results(feature, results);
                        } else {
                            // 用户已离开发起页：结果不跨页展示，状态归位。
                            this.ai_state.status = AiStatus::Idle;
                        }
                    }
                    AiTaskOutcome::Failed(error) => {
                        if this.open == Some(feature)
                            && this.ai_generation.load(Ordering::Relaxed) == generation
                        {
                            this.ai_state.status = AiStatus::Error(error);
                        } else {
                            log::error!(
                                "AI generation for {} finished with {error:?} after navigating away",
                                feature.id()
                            );
                            this.ai_state.status = AiStatus::Idle;
                        }
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
        // 快照与代际在点击时捕获：目录对话框期间状态不得被后续操作改写——
        // 保存内容、历史记录下标、状态文案都以点击时刻为准。
        let generation = self.ai_generation.load(Ordering::Relaxed);
        // 点击时刻，最近一条历史记录必属于当前结果集（新生成只有在完成后才会
        // 替换结果）。用固定下标而非 last_mut，避免保存期间新生成完成导致
        // saved_paths 写进错误的记录。
        let history_index = self.config.generation_history.len().checked_sub(1);
        let selected = self
            .ai_state
            .selected_results
            .iter()
            .filter_map(|index| self.ai_state.results.get(*index))
            .map(|result| result.generated.clone())
            .collect::<Vec<_>>();
        let poster = (feature == Feature::Poster).then(|| {
            (
                [
                    self.poster_title.read(cx).value().to_string(),
                    self.poster_subtitle.read(cx).value().to_string(),
                    self.poster_corner_label.read(cx).value().to_string(),
                ],
                self.poster_layout.read(cx).layers(),
            )
        });
        let paths_receiver = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(tr("ai.action.save_selected")),
        });
        cx.spawn(async move |this, cx| {
            let result =
                first_path_prompt_result(classify_path_prompt_result(paths_receiver.await));
            let _ = this.update(cx, |_, cx| {
                let PathPromptResult::Selected(directory) = result else {
                    // 取消目录选择：结果与状态保持原样，可再次点击保存。
                    cx.notify();
                    return;
                };
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
                                if let Some(index) = history_index
                                    && let Some(record) =
                                        this.config.generation_history.get_mut(index)
                                {
                                    record.saved_paths = paths.clone();
                                }
                                this.persist_config(false);
                                // 保存期间若已发起新生成，代际已变：只更新历史记录，
                                // 不再用旧保存结果覆盖新状态文案。
                                if this.ai_generation.load(Ordering::Relaxed) == generation {
                                    this.ai_state.status = AiStatus::Saved(paths.len());
                                }
                            }
                            Err(error) => {
                                log::error!("AI result save failed: {error}");
                                if this.ai_generation.load(Ordering::Relaxed) == generation {
                                    this.ai_state.status = AiStatus::Error(AiErrorKind::Io);
                                }
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

    fn poster_canvas(&self, show_background: bool, cx: &mut Context<Self>) -> impl IntoElement {
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
                    .when(show_background, |this| match self.ai_state.results.first() {
                        Some(result) => this.child(img(result.preview.clone()).size_full()),
                        None => this,
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
        // 结果只属于其生成时的功能页：海报背景、结果网格与保存入口都以此为门，
        // 防止别的工具的结果串到当前页面。
        let results_belong_here = self.ai_state.results_feature == Some(feature);
        let poster = (feature == Feature::Poster)
            .then(|| self.poster_canvas(results_belong_here, cx).into_any_element());
        let tier_buttons = spec
            .tiers
            .iter()
            .enumerate()
            .map(|(index, tier)| {
                let selected = self.ai_state.selected_tiers.contains(&index);
                Button::new(SharedString::from(format!("ai-tier-{index}")))
                    .small()
                    .w_full()
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

        let has_results = results_belong_here && !results.is_empty();
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
                        this.child(chip_flow(results, 2, false))
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
                    .when(shows_free_prompt(feature), |this| {
                        this.child(Input::new(&self.ai_prompt).cleanable(true))
                    })
                    .when(feature == Feature::Poster, |this| {
                        this.child(Input::new(&self.poster_title).cleanable(true))
                            .child(Input::new(&self.poster_subtitle).cleanable(true))
                            .child(Input::new(&self.poster_corner_label).cleanable(true))
                    })
                    .when(feature == Feature::PromotionalPoster, |this| {
                        this.child(Input::new(&self.poster_title).cleanable(true))
                    })
                    .when(feature == Feature::ImageEdit, |this| {
                        this.child(self.labeled_ai_chips(
                            "ai.parameter.edit_preset",
                            AiEditPreset::ALL.iter().enumerate().map(|(index, preset)| {
                                self.selectable_ai_chip(
                                    format!("ai-edit-{index}"),
                                    tr(&format!("ai.edit.{}", preset.id())),
                                    self.ai_state.edit_preset_index == index,
                                    busy,
                                    cx,
                                    move |this, cx| this.select_edit_preset(index, cx),
                                )
                            }),
                        ))
                    })
                    .child(self.labeled_ai_chips(
                        "ai.parameter.provider",
                        ProviderId::ALL.into_iter().map(|provider| {
                            self.selectable_ai_chip(
                                format!("ai-provider-{}", provider.as_str()),
                                tr(provider_label_key(provider)),
                                self.ai_state.provider == provider,
                                busy,
                                cx,
                                move |this, cx| this.set_ai_provider(provider, cx),
                            )
                        }),
                    ))
                    .when(!hides_ai_ratio(feature), |this| {
                        this.child(self.labeled_ai_chips(
                            "ai.parameter.ratio",
                            capabilities.supported_aspect_ratios.iter().copied().map(
                                |ratio| {
                                    self.selectable_ai_chip(
                                        format!("ai-ratio-{ratio}"),
                                        ratio.to_string(),
                                        self.ai_state.aspect_ratio == ratio,
                                        busy,
                                        cx,
                                        move |this, cx| {
                                            this.ai_state.aspect_ratio = ratio;
                                            cx.notify();
                                        },
                                    )
                                },
                            ),
                        ))
                    })
                    .child(self.labeled_ai_chips(
                        "ai.parameter.count",
                        (1..=capabilities.max_generation_count).map(|count| {
                            self.selectable_ai_chip(
                                format!("ai-count-{count}"),
                                count.to_string(),
                                self.ai_state.count == count,
                                busy || capabilities.max_generation_count == 1,
                                cx,
                                move |this, cx| {
                                    this.ai_state.count = count;
                                    cx.notify();
                                },
                            )
                        }),
                    ))
                    .child(self.labeled_ai_chips(
                        "ai.parameter.quality",
                        [
                            GenerationQuality::Low,
                            GenerationQuality::Medium,
                            GenerationQuality::High,
                        ]
                        .into_iter()
                        .map(|quality| {
                            self.selectable_ai_chip(
                                format!("ai-quality-{quality:?}"),
                                tr(quality_label_key(quality)),
                                self.ai_state.quality == quality,
                                busy,
                                cx,
                                move |this, cx| {
                                    this.ai_state.quality = quality;
                                    cx.notify();
                                },
                            )
                        }),
                    ))
                    .children(self.industry_axis_rows(feature, busy, cx))
                    .when(!tier_buttons.is_empty(), |this| {
                        this.child(chip_flow(tier_buttons, 2, true))
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

fn shows_free_prompt(feature: Feature) -> bool {
    matches!(
        feature,
        Feature::TextToImage
            | Feature::ImageEdit
            | Feature::CoverFactory
            | Feature::ArticleIllustration
            | Feature::Poster
            | Feature::PromotionalPoster
    )
}

fn hides_ai_ratio(feature: Feature) -> bool {
    matches!(
        feature,
        Feature::Poster
            | Feature::IdPhoto
            | Feature::AvatarStudio
            | Feature::MemeGenerator
            | Feature::PromotionalPoster
            | Feature::PlatformAdaptation
    )
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

struct AxisRow {
    label_key: &'static str,
    id_prefix: &'static str,
    items: &'static [&'static str],
    selected: usize,
    set_index: fn(&mut AiUiState, usize),
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
        impressy_core::RgbaImage::from_pixel(width, height, image::Rgba([255, 255, 255, 255]));
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
    image: impressy_core::RgbaImage,
    generated: GeneratedImage,
) -> impressy_core::Result<(impressy_core::RgbaImage, GeneratedImage)> {
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
    image: &impressy_core::RgbaImage,
    width: u32,
    height: u32,
) -> impressy_core::Result<impressy_core::RgbaImage> {
    use impressy_core::transform::{CropRect, ResizeFilter};

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
    let cropped = impressy_core::transform::crop(image, crop)?;
    impressy_core::transform::resize(&cropped, width, height, ResizeFilter::Lanczos3)
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
            let path = directory.join(impressy_core::naming::ai_filename(
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
                        .map(|image| impressy_core::poster::RasterLayer {
                            image,
                            x: (layout.x * POSTER_WIDTH as f32).round() as u32,
                            y: (layout.y * POSTER_HEIGHT as f32).round() as u32,
                        })
                        .map_err(std::io::Error::other)
                })
                .collect::<std::io::Result<Vec<_>>>()?;
            let output = impressy_core::poster::compose(&background, &layers);
            let bytes = format::encode(
                &output,
                EncodeSettings::Png {
                    compression: PngCompression::Default,
                },
            )
            .map_err(std::io::Error::other)?;
            let path = directory.join(impressy_core::naming::ai_filename(
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
        _ => SharedString::from("impressy-core"),
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
                    .when(self.sidebar_open, |this| {
                        this.child(
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
                    })
                    .child(
                        v_flex()
                            .flex_1()
                            .h_full()
                            .min_h(px(0.0))
                            .overflow_hidden()
                            .child(content),
                    ),
            )
            // gpui-component 的对话框由 Root 记录状态，但弹层须由应用在自己的
            // 渲染树末尾挂载才会真正绘制（参照 examples/dialog_overlay.rs）。
            .children(Root::render_dialog_layer(window, cx))
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
        let source = impressy_core::RgbaImage::from_pixel(1_000, 1_000, image::Rgba([1, 2, 3, 255]));
        let output = exact_cover(&source, 800, 800).expect("local crop");
        assert_eq!(output.dimensions(), (800, 800));
        assert!(aspect_ratio_is_close(1_000, 1_000, 800, 800));
        assert!(!aspect_ratio_is_close(1_000, 1_000, 900, 383));
    }
}
