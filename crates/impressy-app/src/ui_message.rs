//! 结构化 UI 消息。
//!
//! 状态只保存语义和数据，不保存已翻译字符串；因此运行时切换语言后，已有状态也会
//! 立即按新 locale 重绘（Requirement 35、36）。

use gpui::SharedString;
use impressy_core::exif::ExifData;
use rust_i18n::t;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    UnsupportedFormat,
    CorruptedImage,
    Encode,
    Operation,
    Exif,
    Qr,
    DiskFull,
    Permission,
    Io,
    Config,
    Clipboard,
    Skipped,
}

impl ErrorKind {
    fn text_key(self) -> &'static str {
        match self {
            Self::UnsupportedFormat => "error.unsupported_format",
            Self::CorruptedImage => "error.corrupted_image",
            Self::Encode => "error.encode",
            Self::Operation => "error.operation",
            Self::Exif => "error.exif",
            Self::Qr => "error.qr",
            Self::DiskFull => "error.disk_full",
            Self::Permission => "error.permission",
            Self::Io => "error.io",
            Self::Config => "error.config",
            Self::Clipboard => "error.clipboard",
            Self::Skipped => "error.skipped",
        }
    }

    fn text(self) -> String {
        t!(self.text_key()).to_string()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum UiMessage {
    #[default]
    None,
    Processing,
    BatchProgress {
        completed: usize,
        total: usize,
    },
    CancelledOpen,
    CancelledSave,
    LoadedImage,
    LoadedMultiple {
        successes: usize,
        failures: usize,
    },
    SavedReady,
    Exported,
    ExportSkipped,
    ExifStripped,
    ExifRead,
    QrGenerated,
    QrDecoded,
    CollageReady,
    SlicesSaved {
        successes: usize,
        total: usize,
    },
    GifReady,
    BatchComplete {
        successes: usize,
        failures: usize,
        total: usize,
    },
    BatchCancelled {
        successes: usize,
        total: usize,
    },
    BatchPresetSaved,
    BatchPresetApplied,
    ImagePasted,
    ImageCopied,
    UndoApplied {
        remaining: usize,
    },
    RedoApplied {
        remaining: usize,
    },
    OriginalRestored,
    AiResultOpened,
    SettingsSaved,
    ConfigReset,
    NeedImage,
    NeedMultipleImages,
    NeedResult,
    NeedExifImage,
    NeedQrImage,
    ClipboardHasNoImage,
    NeedText,
    Busy,
    Error(ErrorKind),
}

impl UiMessage {
    pub fn text(&self) -> SharedString {
        match self {
            Self::None => SharedString::default(),
            Self::Processing => translated("status.processing"),
            Self::BatchProgress { completed, total } => {
                format!("{}: {completed}/{total}", t!("status.batch_progress")).into()
            }
            Self::CancelledOpen => translated("status.cancelled_open"),
            Self::CancelledSave => translated("status.cancelled_save"),
            Self::LoadedImage => translated("status.loaded_image"),
            Self::LoadedMultiple {
                successes,
                failures,
            } => format!(
                "{}: {successes} · {}: {failures}",
                t!("status.successes"),
                t!("status.failures")
            )
            .into(),
            Self::SavedReady => translated("status.ready_to_save"),
            Self::Exported => translated("status.exported"),
            Self::ExportSkipped => translated("status.export_skipped"),
            Self::ExifStripped => translated("status.exif_stripped"),
            Self::ExifRead => translated("status.exif_read"),
            Self::QrGenerated => translated("status.qr_generated"),
            Self::QrDecoded => translated("status.qr_decoded"),
            Self::CollageReady => translated("status.collage_ready"),
            Self::SlicesSaved { successes, total } => {
                format!("{}: {successes}/{total}", t!("status.slices_saved")).into()
            }
            Self::GifReady => translated("status.gif_ready"),
            Self::BatchComplete {
                successes,
                failures,
                total,
            } => format!(
                "{}: {successes} · {}: {failures} · {}: {total}",
                t!("status.successes"),
                t!("status.failures"),
                t!("status.total")
            )
            .into(),
            Self::ImagePasted => translated("status.image_pasted"),
            Self::ImageCopied => translated("status.image_copied"),
            Self::UndoApplied { remaining } => format!(
                "{} · {}: {remaining}",
                t!("status.undo_applied"),
                t!("status.remaining")
            )
            .into(),
            Self::BatchCancelled { successes, total } => format!(
                "{} · {}: {successes}/{total}",
                t!("status.batch_cancelled"),
                t!("status.successes")
            )
            .into(),
            Self::BatchPresetSaved => translated("status.batch_preset_saved"),
            Self::BatchPresetApplied => translated("status.batch_preset_applied"),
            Self::RedoApplied { remaining } => format!(
                "{} · {}: {remaining}",
                t!("status.redo_applied"),
                t!("status.remaining")
            )
            .into(),
            Self::OriginalRestored => translated("status.original_restored"),
            Self::AiResultOpened => translated("status.ai_result_opened"),
            Self::SettingsSaved => translated("status.settings_saved"),
            Self::ConfigReset => translated("status.config_reset"),
            Self::NeedImage => translated("validation.need_image"),
            Self::NeedMultipleImages => translated("validation.need_multiple_images"),
            Self::NeedResult => translated("validation.need_result"),
            Self::NeedExifImage => translated("validation.need_exif_image"),
            Self::NeedQrImage => translated("validation.need_qr_image"),
            Self::ClipboardHasNoImage => translated("validation.clipboard_no_image"),
            Self::NeedText => translated("validation.need_text"),
            Self::Busy => translated("validation.busy"),
            Self::Error(kind) => kind.text().into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BatchItemOutcome {
    Success,
    Failure(ErrorKind),
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchItemResult {
    pub name: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub bytes: Option<usize>,
    pub outcome: BatchItemOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum InfoMessage {
    #[default]
    None,
    Image {
        name: String,
        width: u32,
        height: u32,
    },
    ClipboardImage {
        width: u32,
        height: u32,
    },
    QrContent(String),
    Exif(Option<ExifData>),
    BatchResults(Vec<BatchItemResult>),
}

impl InfoMessage {
    pub fn text(&self) -> SharedString {
        match self {
            Self::None => SharedString::default(),
            Self::Image {
                name,
                width,
                height,
            } => format!("{name} · {width}×{height}").into(),
            Self::ClipboardImage { width, height } => {
                format!("{} · {width}×{height}", t!("status.clipboard_image")).into()
            }
            Self::QrContent(value) => format!("{}: {value}", t!("status.qr_content")).into(),
            Self::Exif(None) => translated("status.no_exif"),
            Self::Exif(Some(data)) => {
                let mut lines = vec![t!("status.exif_source_original").to_string()];
                lines.extend([
                    field("exif.camera_make", data.camera_make.as_deref()),
                    field("exif.camera_model", data.camera_model.as_deref()),
                    field("exif.lens_model", data.lens_model.as_deref()),
                    field("exif.focal_length", data.focal_length.as_deref()),
                    field("exif.aperture", data.aperture.as_deref()),
                    field("exif.shutter_speed", data.shutter_speed.as_deref()),
                    field("exif.iso", data.iso.as_deref()),
                    field("exif.taken_at", data.taken_at.as_deref()),
                ]);
                lines.push(match data.gps {
                    Some(gps) => format!(
                        "{}: {:.6}, {:.6}",
                        t!("exif.gps"),
                        gps.latitude,
                        gps.longitude
                    ),
                    None => format!("{}: —", t!("exif.gps")),
                });
                if data.has_privacy_data() {
                    lines.push(t!("exif.privacy_warning").to_string());
                }
                lines.join("\n").into()
            }
            Self::BatchResults(items) => items
                .iter()
                .map(|item| {
                    let properties = match (item.width, item.height, item.bytes) {
                        (Some(width), Some(height), Some(bytes)) => {
                            format!(" · {width}×{height} · {}", human_bytes(bytes))
                        }
                        _ => String::new(),
                    };
                    let outcome = match item.outcome {
                        BatchItemOutcome::Success => t!("status.item_success").to_string(),
                        BatchItemOutcome::Failure(kind) => kind.text(),
                        BatchItemOutcome::Cancelled => t!("status.item_cancelled").to_string(),
                    };
                    format!("{}{} · {outcome}", item.name, properties)
                })
                .collect::<Vec<_>>()
                .join("\n")
                .into(),
        }
    }
}

fn human_bytes(bytes: usize) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1} MiB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.1} KiB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}

fn field(key: &str, value: Option<&str>) -> String {
    format!("{}: {}", t!(key), value.unwrap_or("—"))
}

fn translated(key: &str) -> SharedString {
    // 所有调用点来自上方枚举的封闭 match；key 不接受外部输入。
    t!(key).to_string().into()
}
