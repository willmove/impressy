//! 应用根视图：四板块导航、设置与后台工作区。
//!
//! GPUI 主线程只更新状态和渲染；文件读取、编解码及图像变换通过 background
//! executor 执行。渲染和交互仍须按 ADR-0002 在桌面真机验收。

use gpui::{
    AnyElement, AppContext, ClipboardEntry, ClipboardItem, Context, Entity, ExternalPaths,
    Image as ClipboardImage, ImageFormat as ClipboardImageFormat, InteractiveElement, IntoElement,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement, Render, SharedString,
    StatefulInteractiveElement, Styled, Window, div, img, prelude::FluentBuilder, px,
};
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::input::{Input, InputState};
use gpui_component::{ActiveTheme, Disableable, h_flex, v_flex};
use rastery_core::config::{AppConfig, Language};
use rastery_core::format::{EncodeSettings, OutputFormat, PngCompression, Quality};
use rastery_core::transform::Rotation;
use rust_i18n::t;

use crate::config_store::ConfigStore;
use crate::crop_frame::{Selection, selection_overlay};
use crate::feature_params::{BatchMode, FeatureParams, ParamAction, ParamEffect, WatermarkSource};
use crate::section::{Feature, Section};
use crate::ui_message::{ErrorKind, UiMessage};
use crate::workspace::{
    BatchRequest, ClipboardFormat, ClipboardPayload, TransformOperation, Workspace,
    WorkspaceCommand, WorkspaceJob, WorkspaceOutcome,
};

fn tr(key: &str) -> SharedString {
    t!(key).to_string().into()
}

enum WorkspaceEvent {
    Progress { completed: usize, total: usize },
    Finished(Box<WorkspaceOutcome>),
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
    qr_input: Entity<InputState>,
    watermark_input: Entity<InputState>,
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
        let qr_input =
            cx.new(|cx| InputState::new(window, cx).placeholder(tr("placeholder.qr_text")));
        let watermark_input =
            cx.new(|cx| InputState::new(window, cx).placeholder(tr("placeholder.watermark_text")));
        Self {
            active: Section::BasicImage,
            open: None,
            settings_open: false,
            config,
            config_store,
            workspace,
            params,
            crop_selection,
            qr_input,
            watermark_input,
        }
    }

    pub fn open_initial_path(&mut self, path: std::path::PathBuf, cx: &mut Context<Self>) {
        if let Some(job) = self.workspace.prepare_paths(vec![path], false) {
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
        self.config.language = match self.config.language {
            Language::ZhCn => Language::En,
            Language::En => Language::ZhCn,
        };
        gpui_component::set_locale(self.config.language.locale());
        self.qr_input.update(cx, |input, input_cx| {
            input.set_placeholder(tr("placeholder.qr_text"), window, input_cx);
        });
        self.watermark_input.update(cx, |input, input_cx| {
            input.set_placeholder(tr("placeholder.watermark_text"), window, input_cx);
        });
        self.persist_config(true);
        cx.notify();
    }

    fn cycle_export_format(&mut self, cx: &mut Context<Self>) {
        self.config.default_export_format = match self.config.default_export_format {
            OutputFormat::Png => OutputFormat::Jpeg,
            OutputFormat::Jpeg => OutputFormat::Webp,
            OutputFormat::Webp => OutputFormat::Png,
        };
        self.persist_config(true);
        cx.notify();
    }

    fn adjust_quality(&mut self, delta: i16, cx: &mut Context<Self>) {
        let current = i16::from(self.config.default_export_quality.get());
        let next = (current + delta).clamp(1, 100) as u8;
        if let Ok(quality) = Quality::new(next) {
            self.config.default_export_quality = quality;
            self.persist_config(true);
            cx.notify();
        }
    }

    fn nav_button(&self, section: Section, cx: &mut Context<Self>) -> impl IntoElement {
        let active = !self.settings_open && self.active == section && self.open.is_none();
        let theme = cx.theme();
        let (foreground, active_bg, hover_bg) = (
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
            .text_color(foreground)
            .when(active, |this| this.bg(active_bg))
            .hover(|this| this.bg(hover_bg))
            .child(tr(section.nav_key()))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.active = section;
                this.open = None;
                this.settings_open = false;
                cx.notify();
            }))
    }

    fn settings_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        div()
            .id("settings-nav")
            .px_3()
            .py_2()
            .rounded_md()
            .text_sm()
            .cursor_pointer()
            .text_color(theme.sidebar_foreground)
            .when(self.settings_open, |this| this.bg(theme.sidebar_accent))
            .hover(|this| this.bg(theme.sidebar_accent))
            .child(tr("settings.title"))
            .on_click(cx.listener(|this, _, _, cx| {
                this.settings_open = true;
                this.open = None;
                cx.notify();
            }))
    }

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
            .hover(|this| this.bg(theme.sidebar_accent))
            .child(tr("lang.toggle"))
            .on_click(cx.listener(|this, _, window, cx| this.toggle_language(window, cx)))
    }

    fn feature_card(&self, feature: Feature, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let is_v1 = feature.is_v1();
        let badge = if is_v1 {
            tr("placeholder.local_badge")
        } else {
            SharedString::from("v2")
        };
        let badge_color = if is_v1 {
            theme.primary
        } else {
            theme.muted_foreground
        };

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
            .hover(|this| this.border_color(theme.primary))
            .child(
                h_flex()
                    .justify_between()
                    .items_center()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(tr(feature.name_key())),
                    )
                    .child(div().text_xs().text_color(badge_color).child(badge)),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(tr(feature.desc_key())),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.open = Some(feature);
                this.settings_open = false;
                cx.notify();
            }))
    }

    fn section_home(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let cards = self
            .active
            .features()
            .iter()
            .map(|&feature| self.feature_card(feature, cx).into_any_element())
            .collect::<Vec<_>>();
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

    fn feature_page(&self, feature: Feature, cx: &mut Context<Self>) -> impl IntoElement {
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
            .hover(move |this| this.bg(secondary))
            .child(SharedString::from(format!("← {}", t!("placeholder.back"))))
            .on_click(cx.listener(|this, _, _, cx| {
                this.open = None;
                cx.notify();
            }));

        let body = if feature.is_v1() {
            self.v1_page(feature, cx).into_any_element()
        } else {
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
        let export = default_export_settings(&self.config);
        let directory = self.config.last_output_dir.clone();
        if let Some(job) = self
            .workspace
            .prepare(command, directory.as_deref(), export)
        {
            self.spawn_workspace_job(job, cx);
        } else {
            cx.notify();
        }
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
        h_flex()
            .justify_between()
            .gap_3()
            .child(div().text_sm().child(tr(label_key)))
            .child(
                Button::new(button_id)
                    .outline()
                    .label(value)
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
                            .label("−")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.adjust_param(actions.0, cx);
                            })),
                    )
                    .child(div().min_w(px(72.0)).text_center().child(value))
                    .child(
                        Button::new(ids.1)
                            .outline()
                            .label("+")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.adjust_param(actions.1, cx);
                            })),
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
                controls.push(self.cycle_control(
                    "parameter.palette",
                    tr(self.params.qr.palette_label_key()),
                    "param-qr-palette",
                    ParamAction::QrPaletteNext,
                    cx,
                ));
            }
            Feature::Beautify => self.beautify_controls(&mut controls, cx),
            Feature::Gif => self.gif_controls(&mut controls, cx),
            _ => {}
        }
        v_flex()
            .w(px(420.0))
            .gap_2()
            .p_3()
            .rounded_lg()
            .border_1()
            .border_color(cx.theme().border)
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
                if params.mode == BatchMode::Compress {
                    controls.push(self.step_control(
                        "parameter.quality",
                        format!("{}%", params.quality.get()).into(),
                        ("param-batch-quality-down", "param-batch-quality-up"),
                        (ParamAction::BatchQuality(-5), ParamAction::BatchQuality(5)),
                        cx,
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

    fn preview_panel(&self, feature: Feature, cx: &mut Context<Self>) -> Option<AnyElement> {
        let image = self.workspace.preview()?;
        let (width, height) = self.workspace.preview_size(520.0, 320.0)?;
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
        Some(
            div()
                .p_2()
                .rounded_lg()
                .border_1()
                .border_color(cx.theme().border)
                .bg(cx.theme().secondary)
                .child(preview)
                .into_any_element(),
        )
    }

    fn v1_page(&self, feature: Feature, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let (primary, muted, foreground) =
            (theme.primary, theme.muted_foreground, theme.foreground);
        let buttons = self.feature_action_buttons(feature, cx);
        let detail = feature_detail(feature);
        let status = self.workspace.status_text();
        let info = self.workspace.info_text();
        let preview = self.preview_panel(feature, cx);

        v_flex()
            .id("v1-workspace")
            .flex_1()
            .overflow_y_scroll()
            .pb_6()
            .gap_3()
            .on_drop(cx.listener(|this, paths: &ExternalPaths, _, cx| {
                this.handle_drop(paths.paths().to_vec(), cx);
            }))
            .child(self.parameter_panel(feature, cx))
            .child(h_flex().flex_wrap().gap_2().children(buttons))
            .child(
                div()
                    .text_xs()
                    .text_color(primary)
                    .child(tr("placeholder.local_badge")),
            )
            .child(div().text_xs().text_color(muted).child(detail))
            .child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child(tr("placeholder.drop_images")),
            )
            .when(!info.is_empty(), |this| {
                this.child(div().text_sm().text_color(foreground).child(info))
            })
            .when(!status.is_empty(), |this| {
                this.child(div().text_sm().text_color(muted).child(status))
            })
            .when_some(preview, |this, preview| this.child(preview))
    }

    fn settings_page(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
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

        v_flex()
            .size_full()
            .p_6()
            .gap_5()
            .child(
                div()
                    .text_xl()
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_color(theme.foreground)
                    .child(tr("settings.title")),
            )
            .child(
                v_flex().gap_2().child(tr("settings.export_format")).child(
                    Button::new("settings-format")
                        .outline()
                        .label(format!(
                            "{} · {}",
                            self.config.default_export_format,
                            t!("action.cycle_format")
                        ))
                        .on_click(cx.listener(|this, _, _, cx| this.cycle_export_format(cx))),
                ),
            )
            .child(
                v_flex()
                    .gap_2()
                    .child(format!(
                        "{}: {}",
                        t!("settings.export_quality"),
                        self.config.default_export_quality.get()
                    ))
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                Button::new("quality-down")
                                    .outline()
                                    .label(tr("action.quality_down"))
                                    .on_click(
                                        cx.listener(|this, _, _, cx| this.adjust_quality(-5, cx)),
                                    ),
                            )
                            .child(
                                Button::new("quality-up")
                                    .outline()
                                    .label(tr("action.quality_up"))
                                    .on_click(
                                        cx.listener(|this, _, _, cx| this.adjust_quality(5, cx)),
                                    ),
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
            )
            .when(!config_path.is_empty(), |this| {
                this.child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(config_path),
                )
            })
            .when(!status.is_empty(), |this| {
                this.child(div().text_sm().text_color(theme.primary).child(status))
            })
    }
}

fn default_export_settings(config: &AppConfig) -> EncodeSettings {
    match config.default_export_format {
        OutputFormat::Png => EncodeSettings::Png {
            compression: PngCompression::Default,
        },
        OutputFormat::Jpeg => EncodeSettings::Jpeg {
            quality: config.default_export_quality,
        },
        OutputFormat::Webp => EncodeSettings::WebpLossy {
            quality: config.default_export_quality,
        },
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
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let nav_items = Section::ALL
            .into_iter()
            .map(|section| self.nav_button(section, cx).into_any_element())
            .collect::<Vec<_>>();
        let settings = self.settings_button(cx).into_any_element();
        let language = self.language_button(cx).into_any_element();

        let content = if self.settings_open {
            self.settings_page(cx).into_any_element()
        } else {
            match self.open {
                Some(feature) => self.feature_page(feature, cx).into_any_element(),
                None => self.section_home(cx).into_any_element(),
            }
        };

        let theme = cx.theme();
        h_flex()
            .size_full()
            .bg(theme.background)
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
                    .child(settings)
                    .child(language),
            )
            .child(v_flex().flex_1().h_full().child(content))
    }
}
