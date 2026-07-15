//! 几何变换：裁剪、缩放、旋转、宽高比预设。
//!
//! 覆盖 FR-01（图片编辑）、Requirement 6、55.1、55.2。

use crate::error::{CoreError, Result};
use image::RgbaImage;

/// 宽高比，以约分后的整数对表示。
///
/// 用整数对而非浮点数，是为了让裁剪能产生**精确**的比例：
/// [`CropRect::largest_centered`] 取该整数对的整数倍作为宽高，
/// 于是输出比例与预设严格相等，而不只是「误差 0.1% 以内」（Requirement 55.1）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AspectRatio {
    width: u32,
    height: u32,
}

impl AspectRatio {
    /// 1:1 —— 头像方图。
    pub const SQUARE: Self = Self {
        width: 1,
        height: 1,
    };
    /// 4:5 —— Instagram 竖图。
    pub const PORTRAIT_4_5: Self = Self {
        width: 4,
        height: 5,
    };
    /// 9:16 —— 短视频封面 / 手机竖屏。
    pub const STORY_9_16: Self = Self {
        width: 9,
        height: 16,
    };
    /// 16:9 —— 横版视频。
    pub const WIDESCREEN_16_9: Self = Self {
        width: 16,
        height: 9,
    };
    /// 21:9 —— 超宽 banner。
    ///
    /// spec Requirement 6.2 只写了「ultra-wide banner」未给具体数值，此处取
    /// 业界标准的超宽比 21:9。
    pub const ULTRAWIDE_BANNER: Self = Self {
        width: 21,
        height: 9,
    };

    /// Requirement 6.2 要求提供的全部预设。
    pub const PRESETS: [Self; 5] = [
        Self::SQUARE,
        Self::PORTRAIT_4_5,
        Self::STORY_9_16,
        Self::WIDESCREEN_16_9,
        Self::ULTRAWIDE_BANNER,
    ];

    /// 构造自定义比例，自动约分。任一边为 0 时返回错误。
    pub fn new(width: u32, height: u32) -> Result<Self> {
        if width == 0 || height == 0 {
            return Err(CoreError::InvalidArgument(format!(
                "宽高比的两边都必须为正，收到 {width}:{height}"
            )));
        }
        let g = gcd(width, height);
        Ok(Self {
            width: width / g,
            height: height / g,
        })
    }

    /// 约分后的宽。
    pub fn width(self) -> u32 {
        self.width
    }

    /// 约分后的高。
    pub fn height(self) -> u32 {
        self.height
    }

    /// 比值（宽 / 高）。
    pub fn value(self) -> f64 {
        f64::from(self.width) / f64::from(self.height)
    }
}

impl std::fmt::Display for AspectRatio {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.width, self.height)
    }
}

fn gcd(mut a: u32, mut b: u32) -> u32 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a.max(1)
}

/// 像素坐标系下的裁剪矩形。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CropRect {
    /// 左上角 x。
    pub x: u32,
    /// 左上角 y。
    pub y: u32,
    /// 宽。
    pub width: u32,
    /// 高。
    pub height: u32,
}

impl CropRect {
    /// 构造裁剪矩形，校验非零。
    pub fn new(x: u32, y: u32, width: u32, height: u32) -> Result<Self> {
        if width == 0 || height == 0 {
            return Err(CoreError::InvalidArgument(format!(
                "裁剪区域的宽高必须为正，收到 {width}×{height}"
            )));
        }
        Ok(Self {
            x,
            y,
            width,
            height,
        })
    }

    /// 在 `image_width × image_height` 内，求给定比例下面积最大的居中矩形。
    ///
    /// 宽高取 `ratio` 约分整数对的整数倍，因此输出比例与 `ratio` **精确相等**
    /// （Requirement 55.1 只要求 0.1% 以内，这里做到 0）。
    ///
    /// 若图像小到放不下一个该比例的矩形（如 1×1 图求 21:9），返回错误。
    pub fn largest_centered(image_width: u32, image_height: u32, ratio: AspectRatio) -> Result<Self> {
        if image_width == 0 || image_height == 0 {
            return Err(CoreError::InvalidArgument(
                "图像尺寸必须为正".to_string(),
            ));
        }
        let k = (image_width / ratio.width).min(image_height / ratio.height);
        if k == 0 {
            return Err(CoreError::InvalidArgument(format!(
                "{image_width}×{image_height} 的图像放不下 {ratio} 的裁剪区域"
            )));
        }
        let width = k * ratio.width;
        let height = k * ratio.height;
        Ok(Self {
            x: (image_width - width) / 2,
            y: (image_height - height) / 2,
            width,
            height,
        })
    }

    /// 该矩形是否完整落在 `image_width × image_height` 内。
    pub fn fits_within(self, image_width: u32, image_height: u32) -> bool {
        self.x.saturating_add(self.width) <= image_width
            && self.y.saturating_add(self.height) <= image_height
    }
}

/// 按矩形裁剪（Requirement 6.9）。
///
/// 矩形越界时返回错误，不做静默钳制——静默钳制会让输出比例偏离用户所选预设。
pub fn crop(img: &RgbaImage, rect: CropRect) -> Result<RgbaImage> {
    if !rect.fits_within(img.width(), img.height()) {
        return Err(CoreError::InvalidArgument(format!(
            "裁剪区域 {}×{} @({},{}) 超出 {}×{} 的图像",
            rect.width,
            rect.height,
            rect.x,
            rect.y,
            img.width(),
            img.height()
        )));
    }
    Ok(image::imageops::crop_imm(img, rect.x, rect.y, rect.width, rect.height).to_image())
}

/// 缩放算法。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ResizeFilter {
    /// 最近邻。最快，适合像素画。
    Nearest,
    /// 双线性。
    Bilinear,
    /// Lanczos3。质量最好，默认。
    #[default]
    Lanczos3,
}

impl From<ResizeFilter> for fast_image_resize::ResizeAlg {
    fn from(f: ResizeFilter) -> Self {
        use fast_image_resize::{FilterType, ResizeAlg};
        match f {
            ResizeFilter::Nearest => ResizeAlg::Nearest,
            ResizeFilter::Bilinear => ResizeAlg::Convolution(FilterType::Bilinear),
            ResizeFilter::Lanczos3 => ResizeAlg::Convolution(FilterType::Lanczos3),
        }
    }
}

/// 缩放到指定尺寸。
///
/// 用 `fast_image_resize`（spec §3.4 指定），它带 SIMD 优化，批量处理时明显快于
/// `image` 自带的缩放。
pub fn resize(
    img: &RgbaImage,
    width: u32,
    height: u32,
    filter: ResizeFilter,
) -> Result<RgbaImage> {
    use fast_image_resize::images::Image as FirImage;
    use fast_image_resize::{PixelType, Resizer};

    if width == 0 || height == 0 {
        return Err(CoreError::InvalidArgument(format!(
            "目标尺寸必须为正，收到 {width}×{height}"
        )));
    }

    let src = FirImage::from_vec_u8(img.width(), img.height(), img.as_raw().clone(), PixelType::U8x4)
        .map_err(|e| CoreError::InvalidArgument(format!("缩放源图无效: {e}")))?;
    let mut dst = FirImage::new(width, height, PixelType::U8x4);
    let mut resizer = Resizer::new();
    let opts = fast_image_resize::ResizeOptions::new().resize_alg(filter.into());
    resizer
        .resize(&src, &mut dst, &opts)
        .map_err(|e| CoreError::InvalidArgument(format!("缩放失败: {e}")))?;

    RgbaImage::from_raw(width, height, dst.into_vec())
        .ok_or_else(|| CoreError::InvalidArgument("缩放结果缓冲区大小不符".to_string()))
}

/// 等比缩放，使图像完整放入 `max_width × max_height`。不放大小于目标的图像。
pub fn resize_to_fit(
    img: &RgbaImage,
    max_width: u32,
    max_height: u32,
    filter: ResizeFilter,
) -> Result<RgbaImage> {
    if max_width == 0 || max_height == 0 {
        return Err(CoreError::InvalidArgument(format!(
            "目标尺寸必须为正，收到 {max_width}×{max_height}"
        )));
    }
    let (w, h) = (img.width(), img.height());
    if w <= max_width && h <= max_height {
        return Ok(img.clone());
    }
    let scale = f64::from(max_width) / f64::from(w);
    let scale = scale.min(f64::from(max_height) / f64::from(h));
    let nw = ((f64::from(w) * scale).round() as u32).max(1);
    let nh = ((f64::from(h) * scale).round() as u32).max(1);
    resize(img, nw, nh, filter)
}

/// 旋转角度。只提供 90° 的整数倍——任意角度旋转需要重采样，会引入插值损失，
/// 而 Requirement 55.2 要求「旋转 360° 后与原图视觉一致」，只有无损旋转能保证这点。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rotation {
    /// 不旋转。
    None,
    /// 顺时针 90°。
    Cw90,
    /// 180°。
    Cw180,
    /// 顺时针 270°（= 逆时针 90°）。
    Cw270,
}

/// 旋转图像。
///
/// 90° 的整数倍旋转是纯像素搬运，无插值，因此旋转四次 90°（共 360°）后与原图
/// **逐像素相等**（Requirement 55.2）。
pub fn rotate(img: &RgbaImage, rotation: Rotation) -> RgbaImage {
    match rotation {
        Rotation::None => img.clone(),
        Rotation::Cw90 => image::imageops::rotate90(img),
        Rotation::Cw180 => image::imageops::rotate180(img),
        Rotation::Cw270 => image::imageops::rotate270(img),
    }
}
