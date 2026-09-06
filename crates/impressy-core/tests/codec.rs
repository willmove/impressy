//! 编解码 property tests：Property 1（无损往返）、21（PNG 幂等）、24（坏文件报错）。
mod common;

use common::arb_image;
use proptest::prelude::*;
use impressy_core::format::{self, EncodeSettings, PngCompression};

proptest! {
    // Feature: impressy, Property 1: Lossless PNG round-trip preserves pixels
    #[test]
    fn prop_png_roundtrip_preserves_pixels(img in arb_image()) {
        let bytes = format::encode(&img, EncodeSettings::Png { compression: PngCompression::Default }).expect("encode");
        let decoded = format::decode(&bytes).expect("decode");
        prop_assert_eq!(img.as_raw(), decoded.as_raw());
    }

    // Feature: impressy, Property 1: Lossless WebP round-trip preserves pixels
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

    // Feature: impressy, Property 21: PNG-to-PNG conversion is idempotent
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

// Feature: impressy, Property 24: Invalid image file returns error without crash
#[test]
fn invalid_bytes_return_error_without_crash() {
    let result = format::decode(b"definitely not an image");
    assert!(result.is_err(), "garbage bytes must error, not panic");
}

// 声明超大画布的 WebP 必须在分配内存前被拒绝（Requirement 57 防御：
// libwebp 解码绕过 image crate 的分配上限，恶意文件可触发 OOM）。
//
// libwebp 的 WebPGetFeatures 对 VP8X 画布自带 2^26 像素上限，但 VP8L 的
// 14bit 尺寸（最大 16384×16384 = 2^28 像素）没有面积检查——正是本预检
// 需要挡住的路径。
#[test]
fn oversized_webp_canvas_is_rejected_before_decoding() {
    // 手工构造最小 VP8L 头（5 字节载荷：签名 0x2F + 14bit 宽高各 16384）。
    // WebPGetFeatures 只解析头、不解码载荷，因此 5 字节即可通过解析。
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&17u32.to_le_bytes()); // 文件剩余部分长度
    bytes.extend_from_slice(b"WEBP");
    bytes.extend_from_slice(b"VP8L");
    bytes.extend_from_slice(&5u32.to_le_bytes()); // chunk payload 长度
    bytes.push(0x2F); // VP8L 签名
    bytes.extend_from_slice(&0x0FFF_FFFFu32.to_le_bytes()); // 宽-1/高-1 各 14 bit

    let result = format::decode(&bytes);
    assert!(
        matches!(result, Err(impressy_core::CoreError::ImageTooLarge { .. })),
        "oversized WebP canvas must be rejected before pixel allocation, got: {result:?}"
    );
}
