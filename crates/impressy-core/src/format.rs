//! 图像编解码与质量控制。
//!
//! 覆盖 Requirement 38（格式支持）、39（质量控制）、52.1–52.8（编解码往返一致性）。
//!
//! 内部一律以 `RgbaImage` 作为统一表示，编码时按目标格式的能力做必要降级
//! （如 JPEG 不支持 alpha，见 [`EncodeSettings::Jpeg`]）。

use crate::error::{CoreError, Result};
use image::{ImageEncoder, ImageFormat as ImgFormat, RgbaImage};
use std::io::Cursor;

/// 支持导出的图像格式。
///
/// spec §4 约定「所有导出类功能默认支持 PNG / JPG / WebP 三种格式并可调质量」。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    /// PNG。无损，支持透明（Requirement 38.10）。
    Png,
    /// JPEG。有损，**不支持透明**。
    Jpeg,
    /// WebP。可有损可无损，无损模式支持透明（Requirement 38.11）。
    Webp,
}

impl OutputFormat {
    /// 该格式的标准扩展名（不含点）。
    pub fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
            Self::Webp => "webp",
        }
    }

    /// 该格式是否能保留 alpha 通道。
    pub fn supports_transparency(self) -> bool {
        !matches!(self, Self::Jpeg)
    }
}

impl std::fmt::Display for OutputFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Png => "PNG",
            Self::Jpeg => "JPEG",
            Self::Webp => "WebP",
        })
    }
}

/// 有损编码的质量，取值 1–100。
///
/// Requirement 39.6 要求「对所有质量设置，编码器必须应用精确指定的质量值」，
/// 因此本类型只做区间校验，不做任何缩放或映射。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
pub struct Quality(u8);

impl Quality {
    /// 构造质量值。超出 1–100 返回 [`CoreError::InvalidArgument`]。
    pub fn new(value: u8) -> Result<Self> {
        if (1..=100).contains(&value) {
            Ok(Self(value))
        } else {
            Err(CoreError::InvalidArgument(format!(
                "质量必须在 1–100 之间，收到 {value}"
            )))
        }
    }

    /// 取出原始值。
    pub fn get(self) -> u8 {
        self.0
    }
}

impl Default for Quality {
    /// 默认 85 —— 视觉与体积的常用平衡点。
    fn default() -> Self {
        Self(85)
    }
}

impl<'de> serde::Deserialize<'de> for Quality {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let v = u8::deserialize(d)?;
        Self::new(v).map_err(serde::de::Error::custom)
    }
}

/// PNG 压缩级别（Requirement 38.6、39.3）。
///
/// PNG 是无损的，压缩级别只影响文件体积与编码耗时，不影响像素。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PngCompression {
    /// 最快，体积最大。
    Fast,
    /// 默认平衡。
    #[default]
    Default,
    /// 最小体积，最慢。
    Best,
}

impl From<PngCompression> for image::codecs::png::CompressionType {
    fn from(c: PngCompression) -> Self {
        match c {
            PngCompression::Fast => Self::Fast,
            PngCompression::Default => Self::Default,
            PngCompression::Best => Self::Best,
        }
    }
}

/// 编码参数。质量选项与格式绑定，使「给 PNG 设 JPEG 质量」这类错误无法表达。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncodeSettings {
    /// PNG：无损，保留透明（Requirement 38.10）。
    Png {
        /// 压缩级别。
        compression: PngCompression,
    },
    /// JPEG：有损，**不支持透明**。
    ///
    /// 编码时 alpha 通道会被丢弃，图像合成到不透明白色背景上。
    Jpeg {
        /// 质量 1–100（Requirement 38.7）。
        quality: Quality,
    },
    /// WebP 有损：质量 1–100（Requirement 38.8）。
    WebpLossy {
        /// 质量 1–100。
        quality: Quality,
    },
    /// WebP 无损：保留透明（Requirement 38.11），像素往返一致（Requirement 52.8）。
    WebpLossless,
}

impl EncodeSettings {
    /// 该参数对应的输出格式。
    pub fn format(self) -> OutputFormat {
        match self {
            Self::Png { .. } => OutputFormat::Png,
            Self::Jpeg { .. } => OutputFormat::Jpeg,
            Self::WebpLossy { .. } | Self::WebpLossless => OutputFormat::Webp,
        }
    }

    /// 该参数是否为无损编码。无损编码保证解码后像素与输入完全一致。
    pub fn is_lossless(self) -> bool {
        matches!(self, Self::Png { .. } | Self::WebpLossless)
    }
}

/// 从字节解码图像。
///
/// 支持 PNG / JPEG / WebP / GIF / BMP（Requirement 38.1–38.5）。格式由内容嗅探，
/// 不依赖扩展名。
///
/// 无效或损坏的输入返回 [`CoreError::ImageDecode`] 而非 panic（Requirement 57.1）。
pub fn decode(bytes: &[u8]) -> Result<RgbaImage> {
    // WebP 走 libwebp（与编码端同库），保证无损往返逐像素一致（Requirement 52.8）；
    // 跨库（image crate 的 WebP 解码器）对 alpha 不保证无损。其余格式用 image crate。
    if matches!(image::guess_format(bytes), Ok(image::ImageFormat::WebP)) {
        return decode_webp(bytes);
    }
    let reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| CoreError::Io {
            path: None,
            source: e,
        })?;
    let img = reader.decode().map_err(|e| CoreError::ImageDecode {
        path: None,
        source: e,
    })?;
    Ok(img.to_rgba8())
}

/// WebP 解码像素上限，与 image crate 的默认 512MiB 分配上限对齐（RGBA 4 字节/像素）。
pub const MAX_DECODE_PIXELS: u64 = 512 * 1024 * 1024 / 4;

/// 用 libwebp 解码 WebP。
///
/// libwebp 的 `WebPDecodeRGBA/RGB` 不受 image crate `Limits::max_alloc` 约束，
/// 因此解码前先用 [`webp::BitstreamFeatures`]（`WebPGetFeatures`，只解析位流头、
/// 不分配像素）校验声明尺寸，超出 [`MAX_DECODE_PIXELS`] 直接拒绝（Requirement 57）。
fn decode_webp(bytes: &[u8]) -> Result<RgbaImage> {
    let features = webp::BitstreamFeatures::new(bytes).ok_or_else(|| CoreError::Webp {
        operation: "解码",
        detail: "无法解析 WebP 位流头".to_string(),
    })?;
    let (width, height) = (features.width(), features.height());
    if u64::from(width) * u64::from(height) > MAX_DECODE_PIXELS {
        return Err(CoreError::ImageTooLarge {
            width,
            height,
            max_pixels: MAX_DECODE_PIXELS,
        });
    }
    let img = webp::Decoder::new(bytes)
        .decode()
        .ok_or_else(|| CoreError::Webp {
            operation: "解码",
            detail: "无法解码 WebP".to_string(),
        })?;
    let (w, h) = (img.width(), img.height());
    if img.is_alpha() {
        RgbaImage::from_raw(w, h, img.to_vec()).ok_or_else(|| CoreError::Webp {
            operation: "解码",
            detail: format!("{w}×{h} RGBA 缓冲区与尺寸不符"),
        })
    } else {
        // 无 alpha 的 WebP：补全不透明 alpha 后转为 RGBA。
        let rgb = image::RgbImage::from_raw(w, h, img.to_vec()).ok_or_else(|| CoreError::Webp {
            operation: "解码",
            detail: format!("{w}×{h} RGB 缓冲区与尺寸不符"),
        })?;
        Ok(image::DynamicImage::ImageRgb8(rgb).to_rgba8())
    }
}

/// 把图像编码为字节。
///
/// Requirement 39.6：质量值原样传给编码器，不做任何映射。
pub fn encode(img: &RgbaImage, settings: EncodeSettings) -> Result<Vec<u8>> {
    match settings {
        EncodeSettings::Png { compression } => encode_png(img, compression),
        EncodeSettings::Jpeg { quality } => encode_jpeg(img, quality),
        EncodeSettings::WebpLossy { quality } => encode_webp_lossy(img, quality),
        EncodeSettings::WebpLossless => encode_webp_lossless(img),
    }
}

fn encode_png(img: &RgbaImage, compression: PngCompression) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    image::codecs::png::PngEncoder::new_with_quality(
        &mut out,
        compression.into(),
        image::codecs::png::FilterType::Adaptive,
    )
    .write_image(
        img.as_raw(),
        img.width(),
        img.height(),
        image::ExtendedColorType::Rgba8,
    )
    .map_err(|source| CoreError::ImageEncode {
        format: OutputFormat::Png,
        source,
    })?;
    Ok(out)
}

fn encode_jpeg(img: &RgbaImage, quality: Quality) -> Result<Vec<u8>> {
    // JPEG 无 alpha 通道，先合成到白色背景。
    let rgb = flatten_onto_white(img);
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality.get())
        .write_image(
            rgb.as_raw(),
            rgb.width(),
            rgb.height(),
            image::ExtendedColorType::Rgb8,
        )
        .map_err(|source| CoreError::ImageEncode {
            format: OutputFormat::Jpeg,
            source,
        })?;
    Ok(out)
}

fn encode_webp_lossy(img: &RgbaImage, quality: Quality) -> Result<Vec<u8>> {
    let encoder = webp::Encoder::from_rgba(img.as_raw(), img.width(), img.height());
    // webp crate 的 quality 是 f32，语义与 1–100 一致，直接透传（Requirement 39.6）。
    let mem = encoder.encode(f32::from(quality.get()));
    Ok(mem.to_vec())
}

fn encode_webp_lossless(img: &RgbaImage) -> Result<Vec<u8>> {
    let encoder = webp::Encoder::from_rgba(img.as_raw(), img.width(), img.height());
    let mem = encoder.encode_lossless();
    Ok(mem.to_vec())
}

/// 把 RGBA 合成到不透明白色背景，得到 RGB。用于 JPEG 这类无 alpha 的格式。
fn flatten_onto_white(img: &RgbaImage) -> image::RgbImage {
    let mut out = image::RgbImage::new(img.width(), img.height());
    for (x, y, px) in img.enumerate_pixels() {
        let [r, g, b, a] = px.0;
        let a = f32::from(a) / 255.0;
        let blend = |c: u8| -> u8 {
            let v = f32::from(c) * a + 255.0 * (1.0 - a);
            v.round().clamp(0.0, 255.0) as u8
        };
        out.put_pixel(x, y, image::Rgb([blend(r), blend(g), blend(b)]));
    }
    out
}

/// 按扩展名推断输出格式。无法识别时返回 `None`。
pub fn format_from_extension(ext: &str) -> Option<OutputFormat> {
    match ext.to_ascii_lowercase().as_str() {
        "png" => Some(OutputFormat::Png),
        "jpg" | "jpeg" => Some(OutputFormat::Jpeg),
        "webp" => Some(OutputFormat::Webp),
        _ => None,
    }
}

/// 判断字节流是否是本引擎能读取的图像格式（Requirement 38.1–38.5）。
pub fn is_supported_input(bytes: &[u8]) -> bool {
    image::guess_format(bytes).is_ok_and(|f| {
        matches!(
            f,
            ImgFormat::Png | ImgFormat::Jpeg | ImgFormat::WebP | ImgFormat::Gif | ImgFormat::Bmp
        )
    })
}
