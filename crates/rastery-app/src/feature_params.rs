//! v1 功能页的可调参数。
//!
//! 这里不依赖 GPUI，所有步进、模式切换与取值范围都能在 headless 测试中验证。

use image::Rgba;
use rastery_core::animation::{GifParams, Playback};
use rastery_core::beautify::{Background, BeautifyParams, Border, GradientDirection, Shadow};
use rastery_core::collage::{CollageLayout, CollageOptions};
use rastery_core::format::{EncodeSettings, OutputFormat, PngCompression, Quality};
use rastery_core::qr::{ErrorCorrection, QrOptions};
use rastery_core::slice::SliceGrid;
use rastery_core::transform::AspectRatio;
use std::num::NonZeroU32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ParamAction {
    EditRatioNext,
    EditWidth(i32),
    EditHeight(i32),
    CollageLayoutNext,
    CollageColumns(i32),
    CollageSpacing(i32),
    CollageBackgroundNext,
    BatchModeNext,
    BatchFormatNext,
    BatchQuality(i16),
    BatchWidth(i32),
    BatchHeight(i32),
    BatchWatermarkSourceNext,
    BatchOpacity(i16),
    BatchPositionNext,
    SliceRows(i32),
    SliceColumns(i32),
    QrSize(i32),
    QrCorrectionNext,
    QrPaletteNext,
    BeautifyRadius(i32),
    BeautifyPadding(i32),
    BeautifyBackgroundNext,
    BeautifyBorder(i32),
    BeautifyShadowToggle,
    GifDelay(i32),
    GifWidth(i32),
    GifHeight(i32),
    GifSizeModeToggle,
    GifPlaybackNext,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ParamEffect {
    None,
    CropRatioChanged,
    BeautifyPreview,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum CollageMode {
    #[default]
    Vertical,
    Horizontal,
    Grid,
}

impl CollageMode {
    pub fn label_key(self) -> &'static str {
        match self {
            Self::Vertical => "option.vertical",
            Self::Horizontal => "option.horizontal",
            Self::Grid => "option.grid",
        }
    }

    fn next(self) -> Self {
        match self {
            Self::Vertical => Self::Horizontal,
            Self::Horizontal => Self::Grid,
            Self::Grid => Self::Vertical,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum BatchMode {
    #[default]
    Convert,
    Compress,
    Resize,
    Watermark,
}

impl BatchMode {
    pub fn label_key(self) -> &'static str {
        match self {
            Self::Convert => "option.convert",
            Self::Compress => "option.compress",
            Self::Resize => "option.resize",
            Self::Watermark => "option.watermark",
        }
    }

    fn next(self) -> Self {
        match self {
            Self::Convert => Self::Compress,
            Self::Compress => Self::Resize,
            Self::Resize => Self::Watermark,
            Self::Watermark => Self::Convert,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum WatermarkSource {
    #[default]
    Text,
    Image,
}

impl WatermarkSource {
    pub fn label_key(self) -> &'static str {
        match self {
            Self::Text => "option.text_watermark",
            Self::Image => "option.image_watermark",
        }
    }

    fn next(self) -> Self {
        match self {
            Self::Text => Self::Image,
            Self::Image => Self::Text,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum WatermarkPlacement {
    #[default]
    BottomRight,
    Tiled,
}

impl WatermarkPlacement {
    pub fn label_key(self) -> &'static str {
        match self {
            Self::BottomRight => "option.bottom_right",
            Self::Tiled => "option.tiled",
        }
    }

    fn next(self) -> Self {
        match self {
            Self::BottomRight => Self::Tiled,
            Self::Tiled => Self::BottomRight,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum BeautifyBackground {
    #[default]
    Gradient,
    Solid,
    Transparent,
}

impl BeautifyBackground {
    pub fn label_key(self) -> &'static str {
        match self {
            Self::Gradient => "option.gradient",
            Self::Solid => "option.solid",
            Self::Transparent => "option.transparent",
        }
    }

    fn next(self) -> Self {
        match self {
            Self::Gradient => Self::Solid,
            Self::Solid => Self::Transparent,
            Self::Transparent => Self::Gradient,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct EditParams {
    pub ratio_index: usize,
    pub width: u32,
    pub height: u32,
}

impl Default for EditParams {
    fn default() -> Self {
        Self {
            ratio_index: 0,
            width: 1024,
            height: 1024,
        }
    }
}

impl EditParams {
    pub fn ratio(self) -> AspectRatio {
        AspectRatio::PRESETS[self.ratio_index]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CollageParams {
    pub mode: CollageMode,
    pub columns: u32,
    pub spacing: u32,
    pub background_index: usize,
}

impl Default for CollageParams {
    fn default() -> Self {
        Self {
            mode: CollageMode::Vertical,
            columns: 3,
            spacing: 0,
            background_index: 0,
        }
    }
}

impl CollageParams {
    pub fn layout(self) -> CollageLayout {
        match self.mode {
            CollageMode::Vertical => CollageLayout::Vertical,
            CollageMode::Horizontal => CollageLayout::Horizontal,
            CollageMode::Grid => CollageLayout::Grid {
                columns: NonZeroU32::new(self.columns).expect("collage columns are clamped to 1+"),
            },
        }
    }

    pub fn options(self) -> CollageOptions {
        const COLORS: [Rgba<u8>; 4] = [
            Rgba([255, 255, 255, 255]),
            Rgba([24, 24, 27, 255]),
            Rgba([228, 228, 231, 255]),
            Rgba([0, 0, 0, 0]),
        ];
        CollageOptions {
            spacing: self.spacing,
            background: COLORS[self.background_index],
        }
    }

    pub fn background_label_key(self) -> &'static str {
        match self.background_index {
            0 => "option.white",
            1 => "option.black",
            2 => "option.gray",
            _ => "option.transparent",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BatchParams {
    pub mode: BatchMode,
    pub format: OutputFormat,
    pub quality: Quality,
    pub width: u32,
    pub height: u32,
    pub watermark_source: WatermarkSource,
    pub watermark_opacity_percent: u8,
    pub watermark_placement: WatermarkPlacement,
}

impl Default for BatchParams {
    fn default() -> Self {
        Self {
            mode: BatchMode::Convert,
            format: OutputFormat::Png,
            quality: Quality::default(),
            width: 1024,
            height: 1024,
            watermark_source: WatermarkSource::Text,
            watermark_opacity_percent: 50,
            watermark_placement: WatermarkPlacement::BottomRight,
        }
    }
}

impl BatchParams {
    pub fn encode_settings(self) -> EncodeSettings {
        match self.format {
            OutputFormat::Png => EncodeSettings::Png {
                compression: match self.quality.get() {
                    1..=33 => PngCompression::Fast,
                    34..=66 => PngCompression::Default,
                    _ => PngCompression::Best,
                },
            },
            OutputFormat::Jpeg => EncodeSettings::Jpeg {
                quality: self.quality,
            },
            OutputFormat::Webp => EncodeSettings::WebpLossy {
                quality: self.quality,
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SliceParams {
    pub rows: u32,
    pub columns: u32,
}

impl Default for SliceParams {
    fn default() -> Self {
        Self {
            rows: 3,
            columns: 3,
        }
    }
}

impl SliceParams {
    pub fn grid(self) -> SliceGrid {
        SliceGrid::new(self.rows, self.columns).expect("slice rows and columns are clamped to 1+")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct QrParams {
    pub size: u32,
    pub correction: ErrorCorrection,
    pub palette_index: usize,
}

impl Default for QrParams {
    fn default() -> Self {
        Self {
            size: 512,
            correction: ErrorCorrection::Medium,
            palette_index: 0,
        }
    }
}

impl QrParams {
    pub fn options(self) -> QrOptions {
        let (foreground, background) = match self.palette_index {
            0 => (Rgba([0, 0, 0, 255]), Rgba([255, 255, 255, 255])),
            1 => (Rgba([30, 64, 175, 255]), Rgba([239, 246, 255, 255])),
            _ => (Rgba([255, 255, 255, 255]), Rgba([24, 24, 27, 255])),
        };
        QrOptions {
            error_correction: self.correction,
            min_size: self.size,
            foreground,
            background,
        }
    }

    pub fn correction_label_key(self) -> &'static str {
        match self.correction {
            ErrorCorrection::Low => "option.correction_low",
            ErrorCorrection::Medium => "option.correction_medium",
            ErrorCorrection::Quartile => "option.correction_quartile",
            ErrorCorrection::High => "option.correction_high",
        }
    }

    pub fn palette_label_key(self) -> &'static str {
        match self.palette_index {
            0 => "option.qr_black_white",
            1 => "option.qr_blue",
            _ => "option.qr_inverted",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BeautifyUiParams {
    pub radius: u32,
    pub padding: u32,
    pub background: BeautifyBackground,
    pub border_width: u32,
    pub shadow: bool,
}

impl Default for BeautifyUiParams {
    fn default() -> Self {
        Self {
            radius: 24,
            padding: 48,
            background: BeautifyBackground::Gradient,
            border_width: 0,
            shadow: true,
        }
    }
}

impl BeautifyUiParams {
    pub fn core(self) -> BeautifyParams {
        let background = match self.background {
            BeautifyBackground::Gradient => Background::Gradient {
                start: Rgba([219, 234, 254, 255]),
                end: Rgba([233, 213, 255, 255]),
                direction: GradientDirection::Diagonal,
            },
            BeautifyBackground::Solid => Background::Solid(Rgba([244, 244, 245, 255])),
            BeautifyBackground::Transparent => Background::Transparent,
        };
        BeautifyParams {
            corner_radius: self.radius,
            inner_padding: self.padding,
            background,
            border: (self.border_width > 0).then_some(Border {
                width: self.border_width,
                color: Rgba([39, 39, 42, 180]),
            }),
            shadow: self.shadow.then_some(Shadow {
                offset: (0, 12),
                blur_sigma: 12.0,
                color: Rgba([0, 0, 0, 96]),
            }),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GifUiParams {
    pub delay_ms: u32,
    pub custom_size: bool,
    pub width: u32,
    pub height: u32,
    pub playback: Playback,
}

impl Default for GifUiParams {
    fn default() -> Self {
        Self {
            delay_ms: 200,
            custom_size: false,
            width: 640,
            height: 640,
            playback: Playback::Forward,
        }
    }
}

impl GifUiParams {
    pub fn core(self) -> GifParams {
        GifParams {
            frame_delay_ms: self.delay_ms,
            dimensions: self.custom_size.then_some((self.width, self.height)),
            playback: self.playback,
        }
    }

    pub fn playback_label_key(self) -> &'static str {
        match self.playback {
            Playback::Forward => "option.forward",
            Playback::Reverse => "option.reverse",
            Playback::PingPong => "option.ping_pong",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct FeatureParams {
    pub edit: EditParams,
    pub collage: CollageParams,
    pub batch: BatchParams,
    pub slice: SliceParams,
    pub qr: QrParams,
    pub beautify: BeautifyUiParams,
    pub gif: GifUiParams,
}

impl FeatureParams {
    pub fn apply(&mut self, action: ParamAction) -> ParamEffect {
        match action {
            ParamAction::EditRatioNext => {
                self.edit.ratio_index = (self.edit.ratio_index + 1) % AspectRatio::PRESETS.len();
                ParamEffect::CropRatioChanged
            }
            ParamAction::EditWidth(delta) => {
                self.edit.width = stepped(self.edit.width, delta, 64, 16, 16_384);
                ParamEffect::None
            }
            ParamAction::EditHeight(delta) => {
                self.edit.height = stepped(self.edit.height, delta, 64, 16, 16_384);
                ParamEffect::None
            }
            ParamAction::CollageLayoutNext => {
                self.collage.mode = self.collage.mode.next();
                ParamEffect::None
            }
            ParamAction::CollageColumns(delta) => {
                self.collage.columns = stepped(self.collage.columns, delta, 1, 1, 20);
                ParamEffect::None
            }
            ParamAction::CollageSpacing(delta) => {
                self.collage.spacing = stepped(self.collage.spacing, delta, 4, 0, 256);
                ParamEffect::None
            }
            ParamAction::CollageBackgroundNext => {
                self.collage.background_index = (self.collage.background_index + 1) % 4;
                ParamEffect::None
            }
            ParamAction::BatchModeNext => {
                self.batch.mode = self.batch.mode.next();
                ParamEffect::None
            }
            ParamAction::BatchFormatNext => {
                self.batch.format = next_format(self.batch.format);
                ParamEffect::None
            }
            ParamAction::BatchQuality(delta) => {
                let value = (i16::from(self.batch.quality.get()) + delta).clamp(1, 100) as u8;
                self.batch.quality = Quality::new(value).expect("quality is clamped to 1..=100");
                ParamEffect::None
            }
            ParamAction::BatchWidth(delta) => {
                self.batch.width = stepped(self.batch.width, delta, 64, 16, 16_384);
                ParamEffect::None
            }
            ParamAction::BatchHeight(delta) => {
                self.batch.height = stepped(self.batch.height, delta, 64, 16, 16_384);
                ParamEffect::None
            }
            ParamAction::BatchWatermarkSourceNext => {
                self.batch.watermark_source = self.batch.watermark_source.next();
                ParamEffect::None
            }
            ParamAction::BatchOpacity(delta) => {
                let next = (i16::from(self.batch.watermark_opacity_percent) + delta).clamp(5, 100);
                self.batch.watermark_opacity_percent = next as u8;
                ParamEffect::None
            }
            ParamAction::BatchPositionNext => {
                self.batch.watermark_placement = self.batch.watermark_placement.next();
                ParamEffect::None
            }
            ParamAction::SliceRows(delta) => {
                self.slice.rows = stepped(self.slice.rows, delta, 1, 1, 100);
                ParamEffect::None
            }
            ParamAction::SliceColumns(delta) => {
                self.slice.columns = stepped(self.slice.columns, delta, 1, 1, 100);
                ParamEffect::None
            }
            ParamAction::QrSize(delta) => {
                self.qr.size = stepped(self.qr.size, delta, 64, 128, 4096);
                ParamEffect::None
            }
            ParamAction::QrCorrectionNext => {
                self.qr.correction = match self.qr.correction {
                    ErrorCorrection::Low => ErrorCorrection::Medium,
                    ErrorCorrection::Medium => ErrorCorrection::Quartile,
                    ErrorCorrection::Quartile => ErrorCorrection::High,
                    ErrorCorrection::High => ErrorCorrection::Low,
                };
                ParamEffect::None
            }
            ParamAction::QrPaletteNext => {
                self.qr.palette_index = (self.qr.palette_index + 1) % 3;
                ParamEffect::None
            }
            ParamAction::BeautifyRadius(delta) => {
                self.beautify.radius = stepped(self.beautify.radius, delta, 4, 0, 512);
                ParamEffect::BeautifyPreview
            }
            ParamAction::BeautifyPadding(delta) => {
                self.beautify.padding = stepped(self.beautify.padding, delta, 8, 0, 1024);
                ParamEffect::BeautifyPreview
            }
            ParamAction::BeautifyBackgroundNext => {
                self.beautify.background = self.beautify.background.next();
                ParamEffect::BeautifyPreview
            }
            ParamAction::BeautifyBorder(delta) => {
                self.beautify.border_width = stepped(self.beautify.border_width, delta, 1, 0, 32);
                ParamEffect::BeautifyPreview
            }
            ParamAction::BeautifyShadowToggle => {
                self.beautify.shadow = !self.beautify.shadow;
                ParamEffect::BeautifyPreview
            }
            ParamAction::GifDelay(delta) => {
                self.gif.delay_ms = stepped(self.gif.delay_ms, delta, 50, 100, 800);
                ParamEffect::None
            }
            ParamAction::GifWidth(delta) => {
                self.gif.width = stepped(self.gif.width, delta, 64, 16, 65_535);
                ParamEffect::None
            }
            ParamAction::GifHeight(delta) => {
                self.gif.height = stepped(self.gif.height, delta, 64, 16, 65_535);
                ParamEffect::None
            }
            ParamAction::GifSizeModeToggle => {
                self.gif.custom_size = !self.gif.custom_size;
                ParamEffect::None
            }
            ParamAction::GifPlaybackNext => {
                self.gif.playback = match self.gif.playback {
                    Playback::Forward => Playback::Reverse,
                    Playback::Reverse => Playback::PingPong,
                    Playback::PingPong => Playback::Forward,
                };
                ParamEffect::None
            }
        }
    }
}

fn stepped(value: u32, direction: i32, step: u32, min: u32, max: u32) -> u32 {
    let delta = i64::from(direction.signum()) * i64::from(step);
    (i64::from(value) + delta).clamp(i64::from(min), i64::from(max)) as u32
}

fn next_format(format: OutputFormat) -> OutputFormat {
    match format {
        OutputFormat::Png => OutputFormat::Jpeg,
        OutputFormat::Jpeg => OutputFormat::Webp,
        OutputFormat::Webp => OutputFormat::Png,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numeric_controls_stay_inside_supported_ranges() {
        let mut params = FeatureParams::default();
        for _ in 0..1_000 {
            params.apply(ParamAction::GifDelay(-1));
            params.apply(ParamAction::SliceRows(-1));
            params.apply(ParamAction::BatchOpacity(-1));
        }
        assert_eq!(params.gif.delay_ms, 100);
        assert_eq!(params.slice.rows, 1);
        assert_eq!(params.batch.watermark_opacity_percent, 5);

        for _ in 0..1_000 {
            params.apply(ParamAction::GifDelay(1));
            params.apply(ParamAction::SliceRows(1));
            params.apply(ParamAction::BatchOpacity(1));
        }
        assert_eq!(params.gif.delay_ms, 800);
        assert_eq!(params.slice.rows, 100);
        assert_eq!(params.batch.watermark_opacity_percent, 100);
    }

    #[test]
    fn all_mode_cycles_return_to_the_start() {
        let mut params = FeatureParams::default();
        for _ in 0..4 {
            params.apply(ParamAction::BatchModeNext);
        }
        assert_eq!(params.batch.mode, BatchMode::Convert);
        for _ in 0..3 {
            params.apply(ParamAction::CollageLayoutNext);
            params.apply(ParamAction::GifPlaybackNext);
        }
        assert_eq!(params.collage.mode, CollageMode::Vertical);
        assert_eq!(params.gif.playback, Playback::Forward);
    }
}
