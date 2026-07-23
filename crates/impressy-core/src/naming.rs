//! 批量处理与切图的跨平台输出命名。
//!
//! 文件名只由源文件 stem、稳定顺序号和格式组成，不拼接目录字符串。
//! 顺序号保证同一批任务内名称唯一（Requirement 41、Property 19）。

use crate::format::OutputFormat;
use crate::slice::Tile;

/// 清除 Windows、macOS 与 Linux 文件名中不安全或不可移植的字符。
pub fn sanitize_stem(stem: &str) -> String {
    let sanitized = stem
        .chars()
        .map(|character| {
            if character.is_control()
                || matches!(
                    character,
                    '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'
                )
            {
                '_'
            } else {
                character
            }
        })
        .collect::<String>();
    let sanitized = sanitized.trim_matches([' ', '.']);
    if sanitized.is_empty() {
        "image".to_string()
    } else {
        sanitized.to_string()
    }
}

/// 生成带 1-based 顺序号的批量输出文件名。
pub fn batch_filename(source_stem: &str, index: usize, format: OutputFormat) -> String {
    format!(
        "{}-impressy-{}.{}",
        sanitize_stem(source_stem),
        index + 1,
        format.extension()
    )
}

/// 生成同时包含顺序号与网格位置的切图文件名。
pub fn tile_filename(tile: &Tile) -> String {
    format!("tile{}.png", tile.filename_suffix())
}

/// Timestamp-based AI output name (Requirement 41.5).
pub fn ai_filename(created_at_unix_ms: u64, index: usize, extension: &str) -> String {
    let extension = extension
        .trim_start_matches('.')
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_lowercase();
    let extension = if extension.is_empty() {
        "png"
    } else {
        &extension
    };
    format!("impressy-ai-{created_at_unix_ms}-{index:02}.{extension}")
}
