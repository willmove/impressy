//! 批量处理（FR-03，Requirement 8、54）。
//!
//! 把同一操作应用到一批图像。**单项失败不影响其余**（Requirement 8.7、8.8）——
//! 失败项单独标记，成功项继续。
//!
//! **设计取舍：输入是已解码的 [`RgbaImage`]，输出是编码后的字节。** 文件 I/O（读取、
//! 落盘、命名 Requirement 41）由 UI 层负责；core 提供纯函数，使批处理不变量
//! （Property 8–13）可被 property test 全自动验证。

use crate::error::{CoreError, Result};
use crate::format::{self, EncodeSettings, OutputFormat, PngCompression};
use crate::transform::{self, ResizeFilter};
use crate::watermark;
use image::RgbaImage;

/// 批量操作（Requirement 8.1 的四种模式）。
///
/// 「格式转换」与「压缩」在已解码像素层面是同一操作——都是「按目标格式与质量重编码」。
/// UI 的「压缩」模式映射为 [`BatchOp::Convert`] 并传入原图格式 + 新质量；core 不必
/// 区分二者，因为解码后原格式已丢失。
#[derive(Debug)]
pub enum BatchOp {
    /// 格式转换 / 压缩（Requirement 8.1、54.1、54.2）。
    Convert {
        /// 目标格式。
        format: OutputFormat,
        /// 编码参数（含质量）。
        settings: EncodeSettings,
    },
    /// 缩放（Requirement 8.1、54.3）。
    Resize {
        /// 目标宽。
        width: u32,
        /// 目标高。
        height: u32,
        /// 缩放算法。
        filter: ResizeFilter,
    },
    /// 加水印（Requirement 8.1、54.4）。
    Watermark {
        /// 水印图像（带 alpha）。
        overlay: RgbaImage,
        /// 不透明度 0.0–1.0。
        opacity: f32,
        /// 位置。
        position: watermark::Position,
    },
}

/// One ordered pixel-processing step in a batch pipeline (Requirement 8.11–8.13).
#[derive(Debug, Clone)]
pub enum BatchStep {
    /// Resize to the displayed dimensions. When `prevent_enlarge` is enabled, inputs that
    /// already fit inside the target bounds pass through unchanged.
    Resize {
        /// Per-input dimension rule.
        mode: BatchResizeMode,
        /// Resampling filter.
        filter: ResizeFilter,
        /// Keep an input unchanged when it already fits inside both target dimensions.
        prevent_enlarge: bool,
    },
    /// Apply an image watermark after every preceding step.
    Watermark {
        /// Watermark pixels, including alpha.
        overlay: RgbaImage,
        /// Additional opacity multiplier in the inclusive range 0–1.
        opacity: f32,
        /// Watermark placement rule.
        position: watermark::Position,
    },
}

/// Dimension rule resolved independently for every input image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatchResizeMode {
    /// Use exact dimensions, optionally fitting each image inside the box without distortion.
    Pixels {
        /// Maximum or exact output width.
        width: u32,
        /// Maximum or exact output height.
        height: u32,
        /// Fit inside the width/height box while preserving the input ratio.
        preserve_aspect: bool,
    },
    /// Scale both dimensions by an integer percentage.
    Percentage(u32),
    /// Scale proportionally so the longest side equals the supplied pixel count.
    LongestSide(u32),
}

/// Ordered processing steps plus the independent output encoding settings.
#[derive(Debug, Clone)]
pub struct BatchPipeline {
    /// Pixel-processing steps in execution order.
    pub steps: Vec<BatchStep>,
    /// Encoding settings applied once after the final processing step.
    pub output: EncodeSettings,
}

/// 单项处理产物。
#[derive(Debug, Clone)]
pub struct BatchOutput {
    /// 编码后的字节。
    pub bytes: Vec<u8>,
    /// 实际输出的格式。
    pub format: OutputFormat,
    /// Final pixel width after every processing step.
    pub width: u32,
    /// Final pixel height after every processing step.
    pub height: u32,
}

/// 失败项（Requirement 8.7）。
#[derive(Debug)]
pub struct BatchFailure {
    /// 在输入列表中的下标。
    pub index: usize,
    /// 失败原因。
    pub error: CoreError,
}

/// 批处理结果（Requirement 8.8）。
#[derive(Debug, Default)]
pub struct BatchResult {
    /// 成功项，按输入顺序排列，附原始下标。
    pub successes: Vec<(usize, BatchOutput)>,
    /// 失败项。
    pub failures: Vec<BatchFailure>,
}

impl BatchResult {
    /// 成功数 + 失败数（恒等于输入数，Requirement 54.6）。
    pub fn total(&self) -> usize {
        self.successes.len() + self.failures.len()
    }
}

/// 处理一批图像（Requirement 8.8、54）。
///
/// 对每张图独立应用操作；单项失败收集到 `failures`，不中断其余（Requirement 8.8）。
/// 成功项保持输入顺序（Requirement 54.5）。
///
/// 缩放与水印的结果以无损 PNG 返回；若需特定格式，调用方可在 UI 层对产物二次编码。
pub fn process(images: &[RgbaImage], op: &BatchOp) -> BatchResult {
    let mut result = BatchResult::default();
    for (index, img) in images.iter().enumerate() {
        match apply_one(img, op) {
            Ok(output) => result.successes.push((index, output)),
            Err(error) => result.failures.push(BatchFailure { index, error }),
        }
    }
    result
}

/// Process every input through all enabled steps in display order, then encode once using the
/// selected output settings. A failure remains isolated to its input item.
pub fn process_pipeline(images: &[RgbaImage], pipeline: &BatchPipeline) -> BatchResult {
    let mut result = BatchResult::default();
    for (index, input) in images.iter().enumerate() {
        match apply_pipeline_one(input, pipeline) {
            Ok(output) => result.successes.push((index, output)),
            Err(error) => result.failures.push(BatchFailure { index, error }),
        }
    }
    result
}

fn apply_pipeline_one(input: &RgbaImage, pipeline: &BatchPipeline) -> Result<BatchOutput> {
    let mut image = input.clone();
    for step in &pipeline.steps {
        image = match step {
            BatchStep::Resize {
                mode,
                filter,
                prevent_enlarge,
            } => {
                let (width, height) = resolve_resize_dimensions(&image, *mode)?;
                if *prevent_enlarge && width >= image.width() && height >= image.height() {
                    image
                } else {
                    transform::resize(&image, width, height, *filter)?
                }
            }
            BatchStep::Watermark {
                overlay,
                opacity,
                position,
            } => watermark::apply(&image, overlay, *opacity, *position)?,
        };
    }

    let bytes = format::encode(&image, pipeline.output)?;
    Ok(BatchOutput {
        bytes,
        format: pipeline.output.format(),
        width: image.width(),
        height: image.height(),
    })
}

fn resolve_resize_dimensions(image: &RgbaImage, mode: BatchResizeMode) -> Result<(u32, u32)> {
    let source_width = image.width();
    let source_height = image.height();
    let scaled = |value: u32, numerator: u32, denominator: u32| {
        ((u64::from(value) * u64::from(numerator) + u64::from(denominator) / 2)
            / u64::from(denominator.max(1)))
        .clamp(1, u64::from(u32::MAX)) as u32
    };
    let dimensions = match mode {
        BatchResizeMode::Pixels {
            width,
            height,
            preserve_aspect: false,
        } => (width, height),
        BatchResizeMode::Pixels {
            width,
            height,
            preserve_aspect: true,
        } => {
            if width == 0 || height == 0 {
                (width, height)
            } else if u64::from(width) * u64::from(source_height)
                <= u64::from(height) * u64::from(source_width)
            {
                (width, scaled(width, source_height, source_width))
            } else {
                (scaled(height, source_width, source_height), height)
            }
        }
        BatchResizeMode::Percentage(percent) => (
            scaled(source_width, percent, 100),
            scaled(source_height, percent, 100),
        ),
        BatchResizeMode::LongestSide(longest) if source_width >= source_height => {
            (longest, scaled(longest, source_height, source_width))
        }
        BatchResizeMode::LongestSide(longest) => {
            (scaled(longest, source_width, source_height), longest)
        }
    };
    if dimensions.0 == 0 || dimensions.1 == 0 {
        return Err(CoreError::InvalidArgument(
            "batch resize dimensions must be non-zero".into(),
        ));
    }
    Ok(dimensions)
}

/// 对单张图应用操作。
fn apply_one(img: &RgbaImage, op: &BatchOp) -> Result<BatchOutput> {
    match op {
        BatchOp::Convert { format, settings } => {
            let bytes = crate::format::encode(img, *settings)?;
            Ok(BatchOutput {
                bytes,
                format: *format,
                width: img.width(),
                height: img.height(),
            })
        }
        BatchOp::Resize {
            width,
            height,
            filter,
        } => {
            let resized = transform::resize(img, *width, *height, *filter)?;
            let bytes = format::encode(
                &resized,
                EncodeSettings::Png {
                    compression: PngCompression::default(),
                },
            )?;
            Ok(BatchOutput {
                bytes,
                format: OutputFormat::Png,
                width: resized.width(),
                height: resized.height(),
            })
        }
        BatchOp::Watermark {
            overlay,
            opacity,
            position,
        } => {
            let marked = watermark::apply(img, overlay, *opacity, *position)?;
            let bytes = format::encode(
                &marked,
                EncodeSettings::Png {
                    compression: PngCompression::default(),
                },
            )?;
            Ok(BatchOutput {
                bytes,
                format: OutputFormat::Png,
                width: marked.width(),
                height: marked.height(),
            })
        }
    }
}
