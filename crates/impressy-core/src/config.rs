//! 配置管理：TOML 序列化 / 反序列化。
//!
//! 覆盖 Requirement 32（配置项）、53（往返一致性）、56.4（幂等性）、57.6（无效 TOML 报错）。
//!
//! 本模块只负责**数据模型与编解码**，不触及文件系统——配置文件路径解析、落盘与
//! 损坏回退（Requirement 53.5）由 UI 层（`impressy-app`）负责。这样配置编解码可被
//! property test 完全自动验证（Property 3、23、26）。

use crate::error::{CoreError, Result};
use crate::format::{OutputFormat, PngCompression, Quality};
use std::path::PathBuf;

/// 界面语言（Requirement 35）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum Language {
    /// 简体中文（默认）。
    #[default]
    ZhCn,
    /// English。
    En,
}

impl Language {
    /// `rust-i18n` / `gpui-component` 使用的 locale 标识。
    pub const fn locale(self) -> &'static str {
        match self {
            Self::ZhCn => "zh-CN",
            Self::En => "en",
        }
    }
}

/// Non-secret local AI generation history (Requirement 32.7).
///
/// Prompts and API keys are intentionally absent. Paths remain local and can be cleared from the
/// settings page.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GenerationHistoryRecord {
    /// Request completion time as Unix milliseconds.
    pub created_at_unix_ms: u64,
    /// Stable provider identifier; never contains credentials.
    pub provider: String,
    /// Stable feature identifier.
    pub feature: String,
    /// Number of images returned by the provider.
    pub output_count: usize,
    /// Local paths chosen by the user when saving this result set.
    #[serde(default)]
    pub saved_paths: Vec<PathBuf>,
}

/// 应用配置（Requirement 32）。
///
/// API Key 永远不属于该模型；它们只存入系统凭据管理器（Requirement 60.1–60.4）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AppConfig {
    /// 默认导出格式（Requirement 32.4）。
    pub default_export_format: OutputFormat,
    /// 默认导出质量（Requirement 32.5）。
    pub default_export_quality: Quality,
    /// PNG 压缩档位（Requirement 39.3）。
    ///
    /// `serde(default)` 保证升级前的配置文件仍可读取。
    #[serde(default)]
    pub default_png_compression: PngCompression,
    /// 界面语言。
    pub language: Language,
    /// 最近使用的输出目录（Requirement 32.10）。
    pub last_output_dir: Option<PathBuf>,
    /// v2 默认 Provider 的稳定标识。使用字符串避免本地 core 依赖网络 crate。
    #[serde(default = "default_provider")]
    pub default_provider: String,
    /// 本地生成历史；不含提示词与密钥。
    #[serde(default)]
    pub generation_history: Vec<GenerationHistoryRecord>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            default_export_format: OutputFormat::Png,
            default_export_quality: Quality::default(),
            default_png_compression: PngCompression::default(),
            language: Language::default(),
            last_output_dir: None,
            default_provider: default_provider(),
            generation_history: Vec::new(),
        }
    }
}

fn default_provider() -> String {
    "seedream".to_string()
}

/// 序列化为 TOML 文本（Requirement 32.1、53.1）。
pub fn to_toml(config: &AppConfig) -> Result<String> {
    toml::to_string_pretty(config).map_err(|e| CoreError::ConfigSerialize { source: e })
}

/// 从 TOML 文本解析（Requirement 32.1、53.2、57.6）。
///
/// 无效 TOML 语法返回 [`CoreError::ConfigParse`] 而非 panic（Requirement 57.6）。
pub fn from_toml(toml_str: &str) -> Result<AppConfig> {
    toml::from_str(toml_str).map_err(|e| CoreError::ConfigParse { source: e })
}
