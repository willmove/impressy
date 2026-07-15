//! # rastery-core —— Rastery 本地图像引擎
//!
//! 本 crate 实现 Rastery 的全部**本地功能**（FR-01~FR-10）：图片进、图片出的确定性
//! 变换。**无 UI、无网络、无全局状态**，因此可以完全 headless 地测试。
//!
//! v1 范围见 `docs/adr/0001-v1-scope-local-only.md`；AI 功能（`rastery-ai`）不属于 v1，
//! 本 crate 不得依赖任何网络或 API Key。
//!
//! ## 模块与需求的对应
//!
//! | 模块 | 功能 | 需求 |
//! | --- | --- | --- |
//! | [`format`] | 编解码、质量控制 | FR-01, Req 38/39/52 |
//! | [`transform`] | 裁剪、缩放、旋转、比例预设 | FR-01, Req 6/55 |
//! | [`collage`] | 拼图拼接 | FR-02, Req 7 |
//! | [`slice`] | 切图 | FR-04, Req 9 |
//! | [`qr`] | 二维码生成与识别 | FR-05, Req 10/52 |
//! | [`exif`] | EXIF 查看与清除 | FR-06, Req 11/55/56 |
//! | [`beautify`] | 截图美化 | FR-07, Req 12/55 |
//! | [`animation`] | GIF 制作 | FR-10, Req 15 |
//! | [`watermark`] | 水印 | FR-03, Req 8 |
//! | [`batch`] | 批量处理 | FR-03, Req 8/54 |
//! | [`config`] | TOML 配置 | Req 53/56 |
//!
//! ## 统一约定
//!
//! - 图像统一以 [`image::RgbaImage`] 表示；编码时按目标格式能力降级。
//! - 所有可能失败的操作返回 [`Result`]，**不 panic**（Requirement 57）。
//! - 参数校验在入口完成，非法参数返回 [`CoreError::InvalidArgument`]。

#![deny(missing_docs)]

pub mod collage;
pub mod error;
pub mod exif;
pub mod format;
pub mod qr;
pub mod slice;
pub mod transform;

pub use error::{CoreError, Result};

/// 本 crate 内部统一的图像类型。
pub use image::RgbaImage;
