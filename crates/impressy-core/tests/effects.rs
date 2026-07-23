//! 效果 property tests：Property 17（GIF 帧数）、18（美化透明）、28（美化尺寸）、
//! 水印尺寸不变。
mod common;

use common::arb_image;
use image::{Rgba, RgbaImage};
use proptest::prelude::*;
use impressy_core::animation::{self, GifParams, Playback};
use impressy_core::beautify::{self, Background, BeautifyParams};
use impressy_core::watermark::{self, Position};
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
