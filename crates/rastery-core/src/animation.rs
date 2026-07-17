//! GIF 制作。
//!
//! 覆盖 FR-10、Requirement 15、Property 17。
//!
//! 用 `gif` crate 编码。帧延迟以 GIF 的 1/100 秒（centisecond）为单位，
//! 输入毫秒按四舍五入换算，最小 1（即 10ms）。

use crate::error::{CoreError, Result};
use crate::transform::{self, ResizeFilter};
use image::RgbaImage;
use std::io::Cursor;

/// 播放模式（Requirement 15.4–15.5）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Playback {
    /// 正序播放。
    #[default]
    Forward,
    /// 倒序播放。
    Reverse,
    /// 乒乓：正序到末尾后倒序回到起点，如此循环（Requirement 15.5）。
    PingPong,
}

/// GIF 编码参数。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GifParams {
    /// 帧延迟（毫秒）。Requirement 15.2 要求 100–800。
    pub frame_delay_ms: u32,
    /// 输出尺寸；`None` 用首帧尺寸（Requirement 15.3）。
    pub dimensions: Option<(u32, u32)>,
    /// 播放模式。
    pub playback: Playback,
}

/// 合成 GIF（Requirement 15.6）。
///
/// 正序模式下输出帧数 == 输入帧数（Property 17）。乒乓模式会追加倒序帧
/// （不含首末重复），帧数接近翻倍。
///
/// 空帧列表或非法延迟返回错误而非 panic（Requirement 57）。
pub fn compose(frames: &[RgbaImage], params: &GifParams) -> Result<Vec<u8>> {
    if frames.is_empty() {
        return Err(CoreError::InvalidArgument("GIF 至少需要一帧".into()));
    }
    if !(100..=800).contains(&params.frame_delay_ms) {
        return Err(CoreError::InvalidArgument(format!(
            "帧延迟必须在 100–800ms 之间，收到 {}",
            params.frame_delay_ms
        )));
    }

    let (tw, th) = params.dimensions.unwrap_or_else(|| frames[0].dimensions());
    if tw == 0 || th == 0 {
        return Err(CoreError::InvalidArgument("GIF 输出尺寸不能为 0".into()));
    }
    // GIF 维度上限为 u16（65535）。
    let (tw16, th16) = (
        u16::try_from(tw).map_err(|_| CoreError::InvalidArgument("GIF 宽度超过 65535".into()))?,
        u16::try_from(th).map_err(|_| CoreError::InvalidArgument("GIF 高度超过 65535".into()))?,
    );

    // 统一缩放到目标尺寸。
    let resized: Vec<RgbaImage> = frames
        .iter()
        .map(|f| {
            if f.dimensions() == (tw, th) {
                Ok(f.clone())
            } else {
                transform::resize(f, tw, th, ResizeFilter::default())
            }
        })
        .collect::<Result<Vec<_>>>()?;

    // 排序。
    let ordered: Vec<RgbaImage> = match params.playback {
        Playback::Forward => resized,
        Playback::Reverse => resized.into_iter().rev().collect(),
        Playback::PingPong => {
            let mut v = resized.clone();
            // 倒序部分跳过最后一帧，避免首末重复。
            for i in (0..resized.len().saturating_sub(1)).rev() {
                v.push(resized[i].clone());
            }
            v
        }
    };

    // ms → centiseconds，四舍五入，最小 1（GIF delay 单位为 1/100 秒）。
    let delay_cs = (((params.frame_delay_ms + 5) / 10) as u16).max(1);

    let mut buf = Cursor::new(Vec::new());
    {
        let mut encoder = gif::Encoder::new(&mut buf, tw16, th16, &[])
            .map_err(|e| CoreError::GifEncode { source: e })?;
        encoder
            .set_repeat(gif::Repeat::Infinite)
            .map_err(|e| CoreError::GifEncode { source: e })?;
        for frame in &ordered {
            let mut rgba = frame.clone().into_raw();
            let mut gf = gif::Frame::from_rgba_speed(tw16, th16, &mut rgba, 10);
            gf.delay = delay_cs;
            encoder
                .write_frame(&gf)
                .map_err(|e| CoreError::GifEncode { source: e })?;
        }
    }
    Ok(buf.into_inner())
}
