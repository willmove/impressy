//! 二维码生成与识别。
//!
//! 覆盖 FR-05、Requirement 10、52.9–52.12、57.2。
//!
//! 生成用 `qrcode`，识别用 `rxing`（spec §3.4 指定）。

use crate::error::{CoreError, Result};
use image::{Rgba, RgbaImage};

/// 纠错级别（Requirement 10.4）。
///
/// 级别越高，二维码能容忍的破损越多，但同样内容需要的模块数也越多。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ErrorCorrection {
    /// 低：可容忍约 7% 破损。
    Low,
    /// 中：约 15%。默认。
    #[default]
    Medium,
    /// 四分位：约 25%。
    Quartile,
    /// 高：约 30%。
    High,
}

impl From<ErrorCorrection> for qrcode::EcLevel {
    fn from(e: ErrorCorrection) -> Self {
        match e {
            ErrorCorrection::Low => Self::L,
            ErrorCorrection::Medium => Self::M,
            ErrorCorrection::Quartile => Self::Q,
            ErrorCorrection::High => Self::H,
        }
    }
}

/// 二维码生成选项。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QrOptions {
    /// 纠错级别（Requirement 10.4）。
    pub error_correction: ErrorCorrection,
    /// 输出图像的最小边长，单位像素（Requirement 10.3）。
    ///
    /// 实际尺寸会向上取整到模块大小的整数倍，因此可能略大于此值——
    /// 非整数倍的模块会产生锯齿，导致识别率下降。
    pub min_size: u32,
    /// 前景色，即深色模块的颜色（Requirement 10.5）。
    pub foreground: Rgba<u8>,
    /// 背景色，即浅色模块与静区的颜色（Requirement 10.6）。
    pub background: Rgba<u8>,
}

impl Default for QrOptions {
    fn default() -> Self {
        Self {
            error_correction: ErrorCorrection::default(),
            min_size: 512,
            foreground: Rgba([0, 0, 0, 255]),
            background: Rgba([255, 255, 255, 255]),
        }
    }
}

/// 生成二维码（Requirement 10.1、10.2）。
///
/// 输出始终带静区（quiet zone）——没有静区的二维码在多数扫码器上识别不了，
/// 这会让 Requirement 10.9「主流扫码工具能正确识别」不成立。
///
/// 内容过长（超出二维码容量上限）时返回 [`CoreError::QrEncode`]。
pub fn generate(data: &str, options: QrOptions) -> Result<RgbaImage> {
    let code = qrcode::QrCode::with_error_correction_level(
        data.as_bytes(),
        options.error_correction.into(),
    )
    .map_err(|source| CoreError::QrEncode { source })?;

    let img = code
        .render::<Rgba<u8>>()
        .min_dimensions(options.min_size, options.min_size)
        .dark_color(options.foreground)
        .light_color(options.background)
        .quiet_zone(true)
        .build();

    Ok(img)
}

/// 识别图像中的二维码（Requirement 10.7）。
///
/// 图中没有二维码、或二维码已损坏到无法纠错时，返回 [`CoreError::QrDecode`]
/// 而非 panic（Requirement 57.2）。
pub fn decode(img: &RgbaImage) -> Result<String> {
    let dynamic = image::DynamicImage::ImageRgba8(img.clone());
    let result = rxing::helpers::detect_in_image(dynamic, Some(rxing::BarcodeFormat::QR_CODE))
        .map_err(|e| CoreError::QrDecode {
            detail: e.to_string(),
        })?;
    Ok(result.getText().to_string())
}
