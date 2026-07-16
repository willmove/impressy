//! 捕获层错误类型。

use thiserror::Error;

/// `rastery-capture` 的统一错误类型。
#[derive(Debug, Error)]
pub enum CaptureError {
    /// 当前环境不支持该系统集成能力（如无头 VM 无显示服务器）。
    ///
    /// 在主开发机（无头云 VM）上，屏幕捕获 / 热键 / 取色一律走这条路——它们必须在
    /// 真机验收（见 ADR-0002）。
    #[error("当前环境不支持该能力：{0}")]
    Unsupported(&'static str),

    /// 指定的显示器不存在。
    #[error("显示器 {0} 不存在")]
    DisplayNotFound(u32),

    /// 请求的捕获区域越出显示器边界。
    #[error("捕获区域越界：{detail}")]
    RegionOutOfBounds {
        /// 具体越界情况。
        detail: String,
    },

    /// 参数非法（如区域宽高为 0、马赛克块大小为 0）。
    #[error("参数非法：{0}")]
    InvalidArgument(String),

    /// 底层平台调用失败。
    #[error("平台调用失败：{0}")]
    Platform(String),
}

/// 捕获层统一 `Result`。
pub type Result<T> = std::result::Result<T, CaptureError>;
