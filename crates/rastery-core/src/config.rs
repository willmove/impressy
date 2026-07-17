//! 配置管理：TOML 序列化 / 反序列化。
//!
//! 覆盖 Requirement 32（配置项）、53（往返一致性）、56.4（幂等性）、57.6（无效 TOML 报错）。
//!
//! 本模块只负责**数据模型与编解码**，不触及文件系统——配置文件路径解析、落盘与
//! 损坏回退（Requirement 53.5）由 UI 层（`rastery-app`）负责。这样配置编解码可被
//! property test 完全自动验证（Property 3、23、26）。

use crate::error::{CoreError, Result};
use crate::format::{OutputFormat, Quality};
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

/// 应用配置（Requirement 32）。
///
/// **只含 v1 字段**：不含 Provider 选择与 API Key——前者属 v2，后者属凭据管理器
/// （Requirement 60.1–60.4 禁止明文落盘）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AppConfig {
    /// 默认导出格式（Requirement 32.4）。
    pub default_export_format: OutputFormat,
    /// 默认导出质量（Requirement 32.5）。
    pub default_export_quality: Quality,
    /// 界面语言。
    pub language: Language,
    /// 最近使用的输出目录（Requirement 32.10）。
    pub last_output_dir: Option<PathBuf>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            default_export_format: OutputFormat::Png,
            default_export_quality: Quality::default(),
            language: Language::default(),
            last_output_dir: None,
        }
    }
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
