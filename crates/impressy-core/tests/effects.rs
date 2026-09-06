//! 效果 property tests：Property 17（GIF 帧数）、18（美化透明）、28（美化尺寸）、
//! 水印尺寸不变。
mod common;

use common::arb_image;
use image::{Rgba, RgbaImage};
use impressy_core::animation::{self, GifParams, Playback};
use impressy_core::beautify::{self, Background, BeautifyParams, Border};
use impressy_core::watermark::{self, Position};
use proptest::prelude::*;
use std::io::Cursor;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    // Feature: impressy, Property 17: GIF frame count matches input
    #[test]
    fn prop_gif_frame_count(n in 1u32..5, side in 4u32..12) {
        let frames: Vec<RgbaImage> = (0..n)
            .map(|i| RgbaImage::from_pixel(side, side, Rgba([i as u8 * 30, 0, 0, 255])))
            .collect();
        let bytes = animation::compose(
            &frames,
            &GifParams { frame_delay_ms: 100, dimensions: None, playback: Playback::Forward },
        )
        .expect("compose");
        let dec = image::codecs::gif::GifDecoder::new(Cursor::new(bytes.as_slice())).expect("decode");
        let count = image::AnimationDecoder::into_frames(dec).count();
        prop_assert_eq!(count, n as usize);
    }
}

// Feature: impressy, Property 18: Beautify transparent bg produces true transparency
#[test]
fn prop_beautify_transparent_bg_has_transparency() {
    proptest!(|(side in 8u32..30, radius in 1u32..8, pad in 0u32..6)| {
        let img = RgbaImage::from_pixel(side, side, Rgba([100, 150, 200, 255]));
        let params = BeautifyParams {
            corner_radius: radius,
            inner_padding: pad,
            background: Background::Transparent,
            border: None,
            shadow: None,
        };
        let out = beautify::beautify(&img, &params).expect("beautify");
        // 画布左上角是背景，透明背景下必须为真透明。
        prop_assert_eq!(out.get_pixel(0, 0).0[3], 0);
    });
}

#[test]
fn ping_pong_frame_count_matches_the_looping_preview_sequence() {
    let frames = (0..4)
        .map(|value| RgbaImage::from_pixel(4, 4, Rgba([value, 0, 0, 255])))
        .collect::<Vec<_>>();
    let bytes = animation::compose(
        &frames,
        &GifParams {
            frame_delay_ms: 100,
            dimensions: None,
            playback: Playback::PingPong,
        },
    )
    .expect("compose ping-pong GIF");
    let decoder =
        image::codecs::gif::GifDecoder::new(Cursor::new(bytes.as_slice())).expect("decode GIF");

    assert_eq!(image::AnimationDecoder::into_frames(decoder).count(), 7);
}

// Feature: impressy, Property 28: Beautification padding increases dimensions predictably
#[test]
fn prop_beautify_padding_dims() {
    proptest!(|(w in 6u32..40, h in 6u32..40, r in 0u32..10, p in 0u32..10)| {
        let img = RgbaImage::from_pixel(w, h, Rgba([9, 9, 9, 255]));
        let params = BeautifyParams {
            corner_radius: r,
            inner_padding: p,
            background: Background::Transparent,
            border: None,
            shadow: None,
        };
        let out = beautify::beautify(&img, &params).expect("beautify");
        let (ow, oh) = out.dimensions();
        // 无阴影时尺寸增量恰为 2×padding。
        prop_assert_eq!(ow, w + 2 * p);
        prop_assert_eq!(oh, h + 2 * p);
        // Property 28：增量 ≤ 2(R+P)。
        prop_assert!((ow - w) as i64 <= 2 * (r as i64 + p as i64));
        prop_assert!((oh - h) as i64 <= 2 * (r as i64 + p as i64));
    });
}

proptest! {
    // 水印不改变图幅（Requirement 54.4 的尺寸侧）。
    #[test]
    fn prop_watermark_preserves_dims(img in arb_image()) {
        let mark = RgbaImage::from_pixel(3, 3, Rgba([255, 255, 255, 180]));
        let out = watermark::apply(&img, &mark, 0.8, Position::BottomRight { margin: 1 }).expect("wm");
        prop_assert_eq!(out.dimensions(), img.dimensions());
    }
}

// 描边宽度超过内容半边长时被钳制，内容像素不被描边色整体覆盖
// （Requirement 57：极端参数返回合理结果而非错误输出）。
#[test]
fn border_wider_than_half_the_content_is_clamped() {
    let content = Rgba([10, 20, 30, 255]);
    let border = Rgba([200, 0, 0, 255]);
    let img = RgbaImage::from_pixel(8, 8, content);
    let params = BeautifyParams {
        corner_radius: 0,
        inner_padding: 0,
        background: Background::Solid(Rgba([255, 255, 255, 255])),
        border: Some(Border {
            width: 32,
            color: border,
        }),
        shadow: None,
    };
    let out = beautify::beautify(&img, &params).expect("beautify");
    // 钳制后描边宽 3：边缘像素是描边色，中心内容像素仍是原图颜色。
    assert_eq!(out.get_pixel(0, 0), &border);
    assert_eq!(out.get_pixel(4, 4), &content);
}

// 1px 内容无法容纳描边：描边被跳过，内容保持不变。
#[test]
fn border_on_single_pixel_content_is_skipped() {
    let content = Rgba([10, 20, 30, 255]);
    let img = RgbaImage::from_pixel(1, 1, content);
    let params = BeautifyParams {
        corner_radius: 0,
        inner_padding: 0,
        background: Background::Solid(Rgba([255, 255, 255, 255])),
        border: Some(Border {
            width: 8,
            color: Rgba([200, 0, 0, 255]),
        }),
        shadow: None,
    };
    let out = beautify::beautify(&img, &params).expect("beautify");
    assert_eq!(out.get_pixel(0, 0), &content);
}
