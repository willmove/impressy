//! 错误类型。
//!
//! Requirement 57 要求所有错误条件下返回错误而非 panic，且系统保持可继续运行的有效状态。
//! 因此本 crate 的公开 API 一律返回 `Result`，不 panic、不 unwrap。

use std::path::PathBuf;

/// `rastery-core` 的统一错误类型。
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    /// 图像解码失败（文件损坏、格式不支持、不是图片）。Requirement 57.1。
    #[error("无法解码图像{}: {source}", path_suffix(.path))]
    ImageDecode {
        /// 出错的文件路径（若来自内存则为 None）。
        path: Option<PathBuf>,
        /// 底层错误。
        #[source]
        source: image::ImageError,
    },

    /// 图像编码失败。
    #[error("无法编码为 {format}: {source}")]
    ImageEncode {
        /// 目标格式。
        format: crate::format::OutputFormat,
        /// 底层错误。
        #[source]
        source: image::ImageError,
    },

    /// WebP 编解码失败。`webp` crate 的错误不实现 `std::error::Error`，故降级为字符串。
    #[error("WebP {operation}失败: {detail}")]
    Webp {
        /// "编码" 或 "解码"。
        operation: &'static str,
        /// 底层错误描述。
        detail: String,
    },

    /// 磁盘空间不足。Requirement 57.5。
    #[error("磁盘空间不足，无法写入 {path}")]
    InsufficientDiskSpace {
        /// 目标路径。
        path: PathBuf,
    },

    /// 其他 I/O 错误。
    #[error("I/O 错误{}: {source}", path_suffix(.path))]
    Io {
        /// 相关路径。
        path: Option<PathBuf>,
        /// 底层错误。
        #[source]
        source: std::io::Error,
    },

    /// 二维码生成失败。
    #[error("二维码生成失败: {source}")]
    QrEncode {
        /// 底层错误。
        #[source]
        source: qrcode::types::QrError,
    },

    /// 二维码解码失败（图中无二维码或已损坏）。Requirement 57.2。
    #[error("二维码解码失败: {detail}")]
    QrDecode {
        /// 失败原因。
        detail: String,
    },

    /// EXIF 处理失败。
    #[error("EXIF 处理失败: {detail}")]
    Exif {
        /// 失败原因。
        detail: String,
    },

    /// GIF 编码失败。
    #[error("GIF 编码失败: {source}")]
    GifEncode {
        /// 底层错误。
        #[source]
        source: gif::EncodingError,
    },

    /// 配置解析失败（TOML 语法错误）。Requirement 57.6。
    #[error("配置解析失败: {source}")]
    ConfigParse {
        /// 底层错误。
        #[source]
        source: toml::de::Error,
    },

    /// 配置序列化失败。
    #[error("配置序列化失败: {source}")]
    ConfigSerialize {
        /// 底层错误。
        #[source]
        source: toml::ser::Error,
    },

    /// 调用方传入了无效参数。这类错误在参数校验阶段就被挡下，不会进入处理流程。
    #[error("无效参数: {0}")]
    InvalidArgument(String),
}

fn path_suffix(path: &Option<PathBuf>) -> String {
    match path {
        Some(p) => format!(" {}", p.display()),
        None => String::new(),
    }
}

/// `rastery-core` 的 `Result` 别名。
pub type Result<T> = std::result::Result<T, CoreError>;
