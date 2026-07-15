//! 编解码 property tests：Property 1（无损往返）、21（PNG 幂等）、24（坏文件报错）。
mod common;

use common::arb_image;
use proptest::prelude::*;
use rastery_core::format::{self, EncodeSettings, PngCompression};

proptest! {
    // Feature: rastery, Property 1: Lossless PNG round-trip preserves pixels
    #[test]
    fn prop_png_roundtrip_preserves_pixels(img in arb_image()) {
        let bytes = format::encode(&img, EncodeSettings::Png { compression: PngCompression::Default }).expect("encode");
        let decoded = format::decode(&bytes).expect("decode");
        prop_assert_eq!(img.as_raw(), decoded.as_raw());
    }

    // Feature: rastery, Property 1: Lossless WebP round-trip preserves pixels
    //
    // libwebp 无损保留全部**可见**像素（含半透明）。但对其 alpha=0 的完全透明像素，
    // libwebp 会清零其 RGB（视觉不可见，且所有 WebP 工具均如此）。故本性质要求：
    // 非透明像素逐字节一致；透明像素仅要求 alpha 仍为 0。
    #[test]
    fn prop_webp_lossless_roundtrip_preserves_pixels(img in arb_image()) {
        let bytes = format::encode(&img, EncodeSettings::WebpLossless).expect("encode");
        let decoded = format::decode(&bytes).expect("decode");
        prop_assert_eq!(img.dimensions(), decoded.dimensions());
        for (a, b) in img.pixels().zip(decoded.pixels()) {
            if a.0[3] == 0 {
                prop_assert_eq!(b.0[3], 0, "透明像素必须保持透明");
            } else {
                prop_assert_eq!(a, b, "非透明像素必须逐字节一致");
            }
        }
    }

    // Feature: rastery, Property 21: PNG-to-PNG conversion is idempotent
    #[test]
    fn prop_png_to_png_idempotent(img in arb_image()) {
        let once = format::encode(&img, EncodeSettings::Png { compression: PngCompression::Default }).expect("encode");
        let twice = format::encode(
            &format::decode(&once).expect("decode"),
            EncodeSettings::Png { compression: PngCompression::Default },
        ).expect("encode2");
        let d1 = format::decode(&once).expect("d1");
        let d2 = format::decode(&twice).expect("d2");
        prop_assert_eq!(d1.as_raw(), d2.as_raw());
    }
}

// Feature: rastery, Property 24: Invalid image file returns error without crash
#[test]
fn invalid_bytes_return_error_without_crash() {
    let result = format::decode(b"definitely not an image");
    assert!(result.is_err(), "garbage bytes must error, not panic");
}
