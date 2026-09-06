//! v1 功能页的可调参数。
//!
//! 这里不依赖 GPUI，所有步进、模式切换与取值范围都能在 headless 测试中验证。

use image::Rgba;
use impressy_core::animation::{GifParams, Playback};
use impressy_core::beautify::{Background, BeautifyParams, Border, GradientDirection, Shadow};
use impressy_core::collage::{CollageLayout, CollageOptions};
use impressy_core::format::{EncodeSettings, OutputFormat, PngCompression, Quality};
use impressy_core::qr::{ErrorCorrection, QrOptions};
use impressy_core::slice::SliceGrid;
use impressy_core::transform::AspectRatio;
use std::num::NonZeroU32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::enum_variant_names)]
pub(crate) enum ParamAction {
    SetEditRatio(usize),
    SetEditWidth(u32),
    SetEditHeight(u32),
    SetEditResizeMode(ResizeMode),
    SetEditAspectLock(bool),
    SetEditPercentage(u32),
    SetEditLongestSide(u32),
    SetEditPreventEnlarge(bool),
    SetCollageMode(CollageMode),
    SetCollageColumns(u32),
    SetCollageSpacing(u32),
    SetCollageBackground(usize),
    SetBatchResizeEnabled(bool),
    SetBatchResizeMode(ResizeMode),
    SetBatchAspectLock(bool),
    SetBatchPercentage(u32),
    SetBatchLongestSide(u32),
    SetBatchWatermarkEnabled(bool),
    SetBatchResizeFirst(bool),
    SetBatchPreventEnlarge(bool),
    SetBatchFormat(OutputFormat),
    SetBatchPngCompression(PngCompression),
    SetBatchWidth(u32),
    SetBatchHeight(u32),
    SetBatchWatermarkSource(WatermarkSource),
    SetBatchOpacity(u8),
    SetBatchPosition(WatermarkPlacement),
    SetSliceRows(u32),
    SetSliceColumns(u32),
    SetQrSize(u32),
    SetQrCorrection(ErrorCorrection),
    SetBeautifyRadius(u32),
    SetBeautifyPadding(u32),
    SetBeautifyBackground(BeautifyBackground),
    SetBeautifyBorder(u32),
    SetBeautifyShadow(bool),
    SetGifDelay(u32),
    SetGifWidth(u32),
    SetGifHeight(u32),
    SetGifCustomSize(bool),
    SetGifPlayback(Playback),
}

/// 裁剪比例选项：五种预设之后是自由比例。
pub(crate) const FREE_RATIO_INDEX: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ParamEffect {
    None,
    CropRatioChanged,
    ResizePreview,
    BeautifyPreview,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ResizeMode {
    #[default]
    Pixels,
    Percentage,
    LongestSide,
}

impl ResizeMode {
    pub fn label_key(self) -> &'static str {
        match self {
            Self::Pixels => "option.resize_pixels",
            Self::Percentage => "option.resize_percentage",
            Self::LongestSide => "option.resize_longest_side",
        }
    }
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct EditParams {
    pub ratio_index: usize,
    pub width: u32,
    pub height: u32,
    pub source_width: u32,
    pub source_height: u32,
    pub aspect_locked: bool,
    pub mode: ResizeMode,
    pub percentage: u32,
    pub longest_side: u32,
    pub prevent_enlarge: bool,
}

impl Default for EditParams {
    fn default() -> Self {
        Self {
            ratio_index: 0,
            width: 1024,
            height: 1024,
            source_width: 1024,
            source_height: 1024,
            aspect_locked: true,
            mode: ResizeMode::Pixels,
            percentage: 100,
            longest_side: 1024,
            prevent_enlarge: true,
        }
    }
}

impl EditParams {
    pub fn ratio(self) -> Option<AspectRatio> {
        AspectRatio::PRESETS.get(self.ratio_index).copied()
    }

    pub fn set_source_dimensions(&mut self, width: u32, height: u32) {
        self.source_width = width.max(1);
        self.source_height = height.max(1);
        self.width = self.source_width;
        self.height = self.source_height;
        self.longest_side = self.source_width.max(self.source_height);
        self.percentage = 100;
    }

    pub fn set_width(&mut self, width: u32) {
        self.width = width.clamp(16, 16_384);
        if self.aspect_locked {
            self.height = proportional_dimension(self.width, self.source_height, self.source_width);
        }
    }

    pub fn set_height(&mut self, height: u32) {
        self.height = height.clamp(16, 16_384);
        if self.aspect_locked {
            self.width = proportional_dimension(self.height, self.source_width, self.source_height);
        }
    }

    pub fn output_dimensions(self) -> (u32, u32) {
        let dimensions = match self.mode {
            ResizeMode::Pixels => (self.width, self.height),
            ResizeMode::Percentage => (
                proportional_dimension(self.percentage, self.source_width, 100),
                proportional_dimension(self.percentage, self.source_height, 100),
            ),
            ResizeMode::LongestSide => {
                if self.source_width >= self.source_height {
                    (
                        self.longest_side,
                        proportional_dimension(
                            self.longest_side,
                            self.source_height,
                            self.source_width,
                        ),
                    )
                } else {
                    (
                        proportional_dimension(
                            self.longest_side,
                            self.source_width,
                            self.source_height,
                        ),
                        self.longest_side,
                    )
                }
            }
        };
        if self.prevent_enlarge
            && dimensions.0 >= self.source_width
            && dimensions.1 >= self.source_height
        {
            (self.source_width, self.source_height)
        } else {
            dimensions
        }
    }
}

fn proportional_dimension(value: u32, numerator: u32, denominator: u32) -> u32 {
    ((u64::from(value) * u64::from(numerator) + u64::from(denominator) / 2)
        / u64::from(denominator.max(1)))
    .clamp(1, 16_384) as u32
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BatchParams {
    pub resize_enabled: bool,
    pub resize_mode: ResizeMode,
    pub aspect_locked: bool,
    pub percentage: u32,
    pub longest_side: u32,
    pub watermark_enabled: bool,
    pub resize_first: bool,
    pub prevent_enlarge: bool,
    pub format: OutputFormat,
    pub quality: Quality,
    pub png_compression: PngCompression,
    pub width: u32,
    pub height: u32,
    pub watermark_source: WatermarkSource,
    pub watermark_opacity_percent: u8,
    pub watermark_placement: WatermarkPlacement,
}

impl Default for BatchParams {
    fn default() -> Self {
        Self {
            resize_enabled: false,
            resize_mode: ResizeMode::Pixels,
            aspect_locked: true,
            percentage: 100,
            longest_side: 1024,
            watermark_enabled: false,
            resize_first: true,
            prevent_enlarge: true,
            format: OutputFormat::Png,
            quality: Quality::default(),
            png_compression: PngCompression::default(),
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
                compression: self.png_compression,
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
    pub foreground: Rgba<u8>,
    pub background: Rgba<u8>,
}

impl Default for QrParams {
    fn default() -> Self {
        Self {
            size: 512,
            correction: ErrorCorrection::Medium,
            foreground: Rgba([0, 0, 0, 255]),
            background: Rgba([255, 255, 255, 255]),
        }
    }
}

impl QrParams {
    pub fn options(self) -> QrOptions {
        QrOptions {
            error_correction: self.correction,
            min_size: self.size,
            foreground: self.foreground,
            background: self.background,
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
            ParamAction::SetEditRatio(index) => {
                self.edit.ratio_index = index.min(FREE_RATIO_INDEX);
                ParamEffect::CropRatioChanged
            }
            ParamAction::SetEditWidth(value) => {
                self.edit.set_width(value);
                ParamEffect::ResizePreview
            }
            ParamAction::SetEditHeight(value) => {
                self.edit.set_height(value);
                ParamEffect::ResizePreview
            }
            ParamAction::SetEditResizeMode(mode) => {
                self.edit.mode = mode;
                ParamEffect::ResizePreview
            }
            ParamAction::SetEditAspectLock(locked) => {
                self.edit.aspect_locked = locked;
                ParamEffect::ResizePreview
            }
            ParamAction::SetEditPercentage(value) => {
                self.edit.percentage = value.clamp(1, 1_000);
                ParamEffect::ResizePreview
            }
            ParamAction::SetEditLongestSide(value) => {
                self.edit.longest_side = value.clamp(16, 16_384);
                ParamEffect::ResizePreview
            }
            ParamAction::SetEditPreventEnlarge(enabled) => {
                self.edit.prevent_enlarge = enabled;
                ParamEffect::ResizePreview
            }
            ParamAction::SetCollageMode(mode) => {
                self.collage.mode = mode;
                ParamEffect::None
            }
            ParamAction::SetCollageColumns(value) => {
                self.collage.columns = value.clamp(1, 20);
                ParamEffect::None
            }
            ParamAction::SetCollageSpacing(value) => {
                self.collage.spacing = value.clamp(0, 256);
                ParamEffect::None
            }
            ParamAction::SetCollageBackground(index) => {
                self.collage.background_index = index.min(3);
                ParamEffect::None
            }
            ParamAction::SetBatchResizeEnabled(enabled) => {
                self.batch.resize_enabled = enabled;
                ParamEffect::None
            }
            ParamAction::SetBatchResizeMode(mode) => {
                self.batch.resize_mode = mode;
                ParamEffect::None
            }
            ParamAction::SetBatchAspectLock(locked) => {
                self.batch.aspect_locked = locked;
                ParamEffect::None
            }
            ParamAction::SetBatchPercentage(value) => {
                self.batch.percentage = value.clamp(1, 800);
                ParamEffect::None
            }
            ParamAction::SetBatchLongestSide(value) => {
                self.batch.longest_side = value.clamp(16, 16_384);
                ParamEffect::None
            }
            ParamAction::SetBatchWatermarkEnabled(enabled) => {
                self.batch.watermark_enabled = enabled;
                ParamEffect::None
            }
            ParamAction::SetBatchResizeFirst(resize_first) => {
                self.batch.resize_first = resize_first;
                ParamEffect::None
            }
            ParamAction::SetBatchPreventEnlarge(enabled) => {
                self.batch.prevent_enlarge = enabled;
                ParamEffect::None
            }
            ParamAction::SetBatchFormat(format) => {
                self.batch.format = format;
                ParamEffect::None
            }
            ParamAction::SetBatchPngCompression(compression) => {
                self.batch.png_compression = compression;
                ParamEffect::None
            }
            ParamAction::SetBatchWidth(value) => {
                self.batch.width = value.clamp(16, 16_384);
                ParamEffect::None
            }
            ParamAction::SetBatchHeight(value) => {
                self.batch.height = value.clamp(16, 16_384);
                ParamEffect::None
            }
            ParamAction::SetBatchWatermarkSource(source) => {
                self.batch.watermark_source = source;
                ParamEffect::None
            }
            ParamAction::SetBatchOpacity(value) => {
                self.batch.watermark_opacity_percent = value.clamp(5, 100);
                ParamEffect::None
            }
            ParamAction::SetBatchPosition(placement) => {
                self.batch.watermark_placement = placement;
                ParamEffect::None
            }
            ParamAction::SetSliceRows(value) => {
                self.slice.rows = value.clamp(1, 100);
                ParamEffect::None
            }
            ParamAction::SetSliceColumns(value) => {
                self.slice.columns = value.clamp(1, 100);
                ParamEffect::None
            }
            ParamAction::SetQrSize(value) => {
                self.qr.size = value.clamp(128, 4096);
                ParamEffect::None
            }
            ParamAction::SetQrCorrection(correction) => {
                self.qr.correction = correction;
                ParamEffect::None
            }
            ParamAction::SetBeautifyRadius(value) => {
                self.beautify.radius = value.clamp(0, 512);
                ParamEffect::BeautifyPreview
            }
            ParamAction::SetBeautifyPadding(value) => {
                self.beautify.padding = value.clamp(0, 1024);
                ParamEffect::BeautifyPreview
            }
            ParamAction::SetBeautifyBackground(background) => {
                self.beautify.background = background;
                ParamEffect::BeautifyPreview
            }
            ParamAction::SetBeautifyBorder(value) => {
                self.beautify.border_width = value.clamp(0, 32);
                ParamEffect::BeautifyPreview
            }
            ParamAction::SetBeautifyShadow(enabled) => {
                self.beautify.shadow = enabled;
                ParamEffect::BeautifyPreview
            }
            ParamAction::SetGifDelay(value) => {
                self.gif.delay_ms = value.clamp(100, 800);
                ParamEffect::None
            }
            ParamAction::SetGifWidth(value) => {
                self.gif.width = value.clamp(16, 65_535);
                ParamEffect::None
            }
            ParamAction::SetGifHeight(value) => {
                self.gif.height = value.clamp(16, 65_535);
                ParamEffect::None
            }
            ParamAction::SetGifCustomSize(custom) => {
                self.gif.custom_size = custom;
                ParamEffect::None
            }
            ParamAction::SetGifPlayback(playback) => {
                self.gif.playback = playback;
                ParamEffect::None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numeric_controls_stay_inside_supported_ranges() {
        let mut params = FeatureParams::default();
        params.apply(ParamAction::SetGifDelay(0));
        params.apply(ParamAction::SetSliceRows(0));
        params.apply(ParamAction::SetBatchOpacity(0));
        params.apply(ParamAction::SetBatchPercentage(0));
        params.apply(ParamAction::SetBatchLongestSide(0));
        assert_eq!(params.gif.delay_ms, 100);
        assert_eq!(params.slice.rows, 1);
        assert_eq!(params.batch.watermark_opacity_percent, 5);
        assert_eq!(params.batch.percentage, 1);
        assert_eq!(params.batch.longest_side, 16);

        params.apply(ParamAction::SetGifDelay(9_000));
        params.apply(ParamAction::SetSliceRows(9_000));
        params.apply(ParamAction::SetBatchOpacity(255));
        assert_eq!(params.gif.delay_ms, 800);
        assert_eq!(params.slice.rows, 100);
        assert_eq!(params.batch.watermark_opacity_percent, 100);
    }

    #[test]
    fn enum_and_ratio_choices_set_exactly() {
        let mut params = FeatureParams::default();
        params.apply(ParamAction::SetBatchWatermarkEnabled(true));
        params.apply(ParamAction::SetCollageMode(CollageMode::Grid));
        params.apply(ParamAction::SetGifPlayback(Playback::PingPong));
        params.apply(ParamAction::SetBatchPngCompression(PngCompression::Best));
        params.apply(ParamAction::SetEditRatio(FREE_RATIO_INDEX));
        assert!(params.batch.watermark_enabled);
        assert_eq!(params.collage.mode, CollageMode::Grid);
        assert_eq!(params.gif.playback, Playback::PingPong);
        assert_eq!(params.batch.png_compression, PngCompression::Best);
        assert_eq!(params.edit.ratio(), None);

        params.apply(ParamAction::SetEditRatio(0));
        assert_eq!(params.edit.ratio(), Some(AspectRatio::SQUARE));
    }

    #[test]
    fn batch_steps_and_output_settings_are_independent() {
        let mut params = FeatureParams::default();
        params.apply(ParamAction::SetBatchFormat(OutputFormat::Png));
        params.apply(ParamAction::SetBatchResizeEnabled(true));
        params.apply(ParamAction::SetBatchWatermarkEnabled(true));
        params.apply(ParamAction::SetBatchResizeFirst(false));
        params.apply(ParamAction::SetBatchResizeMode(ResizeMode::LongestSide));
        params.apply(ParamAction::SetBatchLongestSide(2048));
        params.apply(ParamAction::SetBatchPercentage(75));
        assert_eq!(params.batch.format, OutputFormat::Png);
        assert!(params.batch.resize_enabled);
        assert!(params.batch.watermark_enabled);
        assert!(!params.batch.resize_first);
        assert_eq!(params.batch.resize_mode, ResizeMode::LongestSide);
        assert_eq!(params.batch.longest_side, 2048);
        assert_eq!(params.batch.percentage, 75);
    }

    #[test]
    fn qr_options_preserve_independent_custom_colors() {
        let params = QrParams {
            foreground: Rgba([12, 34, 56, 255]),
            background: Rgba([210, 220, 230, 255]),
            ..QrParams::default()
        };
        let options = params.options();
        assert_eq!(options.foreground, params.foreground);
        assert_eq!(options.background, params.background);
    }

    #[test]
    fn resize_defaults_to_locked_source_dimensions_and_supports_relative_modes() {
        let mut edit = EditParams::default();
        edit.set_source_dimensions(1_920, 1_080);
        assert_eq!(edit.output_dimensions(), (1_920, 1_080));

        edit.set_width(960);
        assert_eq!(edit.output_dimensions(), (960, 540));

        edit.mode = ResizeMode::Percentage;
        edit.percentage = 25;
        assert_eq!(edit.output_dimensions(), (480, 270));

        edit.mode = ResizeMode::LongestSide;
        edit.longest_side = 800;
        assert_eq!(edit.output_dimensions(), (800, 450));
    }

    #[test]
    fn prevent_enlarge_keeps_source_dimensions() {
        let mut edit = EditParams::default();
        edit.set_source_dimensions(640, 480);
        edit.mode = ResizeMode::LongestSide;
        edit.longest_side = 2_000;
        edit.prevent_enlarge = true;
        assert_eq!(edit.output_dimensions(), (640, 480));
    }
}
