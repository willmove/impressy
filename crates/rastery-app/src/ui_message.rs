//! 结构化 UI 消息。
//!
//! 状态只保存语义和数据，不保存已翻译字符串；因此运行时切换语言后，已有状态也会
//! 立即按新 locale 重绘（Requirement 35、36）。

use gpui::SharedString;
use rastery_core::exif::ExifData;
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
    Saved,
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
    ImagePasted,
    ImageCopied,
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
            Self::Saved => translated("status.saved"),
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
            Self::Error(kind) => match kind {
                ErrorKind::UnsupportedFormat => translated("error.unsupported_format"),
                ErrorKind::CorruptedImage => translated("error.corrupted_image"),
                ErrorKind::Encode => translated("error.encode"),
                ErrorKind::Operation => translated("error.operation"),
                ErrorKind::Exif => translated("error.exif"),
                ErrorKind::Qr => translated("error.qr"),
                ErrorKind::DiskFull => translated("error.disk_full"),
                ErrorKind::Permission => translated("error.permission"),
                ErrorKind::Io => translated("error.io"),
                ErrorKind::Config => translated("error.config"),
                ErrorKind::Clipboard => translated("error.clipboard"),
            },
        }
    }
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
    BatchFailures(Vec<String>),
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
                let mut lines = vec![
                    field("exif.camera_make", data.camera_make.as_deref()),
                    field("exif.camera_model", data.camera_model.as_deref()),
                    field("exif.lens_model", data.lens_model.as_deref()),
                    field("exif.focal_length", data.focal_length.as_deref()),
                    field("exif.aperture", data.aperture.as_deref()),
                    field("exif.shutter_speed", data.shutter_speed.as_deref()),
                    field("exif.iso", data.iso.as_deref()),
                    field("exif.taken_at", data.taken_at.as_deref()),
                ];
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
            Self::BatchFailures(names) => {
                format!("{}: {}", t!("status.failed_items"), names.join(", ")).into()
            }
        }
    }
}

fn field(key: &str, value: Option<&str>) -> String {
    format!("{}: {}", t!(key), value.unwrap_or("—"))
}

fn translated(key: &str) -> SharedString {
    // 所有调用点来自上方枚举的封闭 match；key 不接受外部输入。
    t!(key).to_string().into()
}
