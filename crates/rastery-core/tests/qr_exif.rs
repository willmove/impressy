//! 二维码与 EXIF property tests：Property 2（QR 往返）、22（QR 重编码幂等）、
//! 25（坏 QR 报错）、4（EXIF 清除）、20（EXIF 幂等）。
mod common;

use common::arb_image;
use img_parts::{Bytes, DynImage, ImageEXIF};
use proptest::prelude::*;
use rastery_core::exif;
use rastery_core::format::{self, EncodeSettings, Quality};
use rastery_core::qr;
use rastery_core::qr::QrOptions;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    // Feature: rastery, Property 2: QR round-trip preserves content
    #[test]
    fn prop_qr_roundtrip(text in "[a-zA-Z0-9 .:/]{1,40}") {
        let img = qr::generate(&text, QrOptions::default()).expect("generate");
        let decoded = qr::decode(&img).expect("decode");
        prop_assert_eq!(decoded, text);
    }
}

// Feature: rastery, Property 22: QR re-encoding is idempotent
#[test]
fn prop_qr_reencode_idempotent() {
    proptest!(|(text in "[a-zA-Z0-9]{1,30}")| {
        let img1 = qr::generate(&text, QrOptions::default()).expect("g1");
        let d1 = qr::decode(&img1).expect("d1");
        let img2 = qr::generate(&d1, QrOptions::default()).expect("g2");
        let d2 = qr::decode(&img2).expect("d2");
        prop_assert_eq!(&d1, &text);
        prop_assert_eq!(&d2, &text);
    });
}

// Feature: rastery, Property 25: Corrupted QR code returns error without crash
proptest! {
    #[test]
    fn prop_corrupted_qr_returns_error(img in arb_image()) {
        let result = qr::decode(&img);
        prop_assert!(result.is_err(), "non-QR image must error, not panic");
    }
}

/// 注入一个最小 EXIF 段（空 IFD0 的 TIFF）到 JPEG 字节中。
fn inject_exif(jpeg: &[u8]) -> Vec<u8> {
    let mut image = DynImage::from_bytes(Bytes::copy_from_slice(jpeg))
        .expect("parse")
        .expect("is jpeg");
    // 小端 TIFF：II / 0x002A / IFD0@8 / 0 条目 / 无下一 IFD。
    let blob = vec![
        0x49, 0x49, 0x2A, 0x00, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];
    image.set_exif(Some(Bytes::from(blob)));
    image.encoder().bytes().to_vec()
}

#[test]
// Feature: rastery, Property 4: EXIF cleaning removes metadata and preserves pixels
fn exif_strip_removes_metadata_and_preserves_pixels() {
    use image::{Rgba, RgbaImage};
    let img = RgbaImage::from_pixel(16, 16, Rgba([200, 100, 50, 255]));
    let jpeg = format::encode(
        &img,
        EncodeSettings::Jpeg {
            quality: Quality::new(90).unwrap(),
        },
    )
    .expect("encode jpeg");

    let with_exif = inject_exif(&jpeg);
    assert!(exif::has_exif(&with_exif).expect("has_exif before"), "injected exif must be present");

    let stripped = exif::strip(&with_exif).expect("strip");
    assert!(
        !exif::has_exif(&stripped).expect("has_exif after"),
        "EXIF (含 GPS/设备字段) 必须被清除"
    );

    // 像素不变：strip 只动容器段，压缩数据原样保留。
    let before = format::decode(&with_exif).expect("decode before");
    let after = format::decode(&stripped).expect("decode after");
    assert_eq!(before.as_raw(), after.as_raw());
}

#[test]
// Feature: rastery, Property 20: EXIF cleaning is idempotent
fn exif_strip_idempotent() {
    use image::{Rgba, RgbaImage};
    let img = RgbaImage::from_pixel(8, 8, Rgba([10, 20, 30, 255]));
    let jpeg = format::encode(
        &img,
        EncodeSettings::Jpeg {
            quality: Quality::new(80).unwrap(),
        },
    )
    .expect("encode");
    let with_exif = inject_exif(&jpeg);

    let once = exif::strip(&with_exif).expect("strip1");
    let twice = exif::strip(&once).expect("strip2");
    assert_eq!(once, twice, "二次清除应与一次清除相同");
}
