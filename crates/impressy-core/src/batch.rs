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

/// 单项处理产物。
#[derive(Debug, Clone)]
pub struct BatchOutput {
    /// 编码后的字节。
    pub bytes: Vec<u8>,
    /// 实际输出的格式。
    pub format: OutputFormat,
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

/// 对单张图应用操作。
fn apply_one(img: &RgbaImage, op: &BatchOp) -> Result<BatchOutput> {
    match op {
        BatchOp::Convert { format, settings } => {
            let bytes = crate::format::encode(img, *settings)?;
            Ok(BatchOutput {
                bytes,
                format: *format,
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
            })
        }
    }
}
