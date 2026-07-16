//! # rastery-capture —— Rastery 系统集成层
//!
//! 覆盖 FR-08（屏幕截图 + 标注）、FR-09（屏幕取色）、全局热键、剪贴板。
//!
//! ## 本机可验证性（重要）
//!
//! 主开发机是无头云 VM。本 crate 分两类代码：
//!
//! | 模块 | 性质 | 本机可验证？ |
//! | --- | --- | --- |
//! | [`annotate`] | 纯像素合成（画笔 / 马赛克） | ✅ 可 headless 测试 |
//! | [`device`] | 平台特定（截图 / 热键 / 取色 / 多屏 DPI） | ❌ 只能真机验收 |
//! | [`clipboard`] | 平台特定（系统剪贴板） | ❌ 只能真机验收 |
//!
//! 因此 [`annotate`] 做实并配 property test；[`device`] / [`clipboard`] 只给出 trait 与
//! 无头安全占位（[`device::NullCaptureDevice`]），真实平台实现在真机分支补齐（ADR-0002）。

#![deny(missing_docs)]

pub mod annotate;
pub mod clipboard;
pub mod device;
pub mod error;

pub use error::{CaptureError, Result};
