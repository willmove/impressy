//! 批处理 property tests：Property 8（数量守恒）、9（顺序）、10（格式转换保尺寸）、
//! 11（压缩保尺寸）、12（缩放保数量）、13（水印保数量）。
mod common;

use common::arb_image;
use image::{Rgba, RgbaImage};
use proptest::prelude::*;
use impressy_core::batch::{self, BatchOp};
use impressy_core::format::{self, EncodeSettings, OutputFormat, Quality};
use impressy_core::transform::ResizeFilter;
use impressy_core::watermark::Position;

fn arb_images() -> impl Strategy<Value = Vec<RgbaImage>> {
    proptest::collection::vec(arb_image(), 1..=6)
}

proptest! {
    // Feature: impressy, Property 8 + 9 + 10: format conversion preserves count, order, dims
    #[test]
    fn prop_batch_convert(imgs in arb_images()) {
        let n = imgs.len();
        let op = BatchOp::Convert {
            format: OutputFormat::Jpeg,
            settings: EncodeSettings::Jpeg { quality: Quality::new(75).unwrap() },
        };
        let res = batch::process(&imgs, &op);
        // Property 8: 数量守恒。
        prop_assert_eq!(res.total(), n);
        prop_assert!(res.failures.is_empty(), "valid images must not fail");
        // Property 9 + 10: 顺序保持、尺寸不变。
        for (k, (idx, out)) in res.successes.iter().enumerate() {
            prop_assert_eq!(*idx, k);
            let d = format::decode(&out.bytes).expect("decode output");
            prop_assert_eq!(d.dimensions(), imgs[k].dimensions());
        }
    }

    // Feature: impressy, Property 11: compression preserves dimensions
    #[test]
    fn prop_batch_compression_preserves_dimensions(imgs in arb_images(), quality in 1u8..=100) {
        let op = BatchOp::Convert {
            format: OutputFormat::Jpeg,
            settings: EncodeSettings::Jpeg {
                quality: Quality::new(quality).expect("generated quality is valid"),
            },
        };
        let res = batch::process(&imgs, &op);
        prop_assert!(res.failures.is_empty(), "valid images must not fail");
        for (index, output) in &res.successes {
            let decoded = format::decode(&output.bytes).expect("decode compressed output");
            prop_assert_eq!(decoded.dimensions(), imgs[*index].dimensions());
        }
    }

    // Feature: impressy, Property 12: resize preserves count
    #[test]
    fn prop_batch_resize_count(imgs in arb_images()) {
        let n = imgs.len();
        let op = BatchOp::Resize { width: 8, height: 8, filter: ResizeFilter::Nearest };
        let res = batch::process(&imgs, &op);
        prop_assert_eq!(res.successes.len(), n);
        prop_assert_eq!(res.total(), n);
    }

    // Feature: impressy, Property 13: watermark preserves count
    #[test]
    fn prop_batch_watermark_count(imgs in arb_images()) {
        let n = imgs.len();
        let mark = RgbaImage::from_pixel(2, 2, Rgba([255, 255, 255, 200]));
        let op = BatchOp::Watermark {
            overlay: mark,
            opacity: 0.5,
            position: Position::Tiled { spacing: 2 },
        };
        let res = batch::process(&imgs, &op);
        prop_assert_eq!(res.successes.len(), n);
        prop_assert_eq!(res.total(), n);
    }
}

// Feature: impressy, Property 8 (failure path): failures counted, total preserved
#[test]
fn batch_failure_path_preserves_count() {
    let imgs = vec![RgbaImage::from_pixel(4, 4, Rgba([1, 1, 1, 255])); 3];
    // 0×0 是非法目标尺寸，每张图都会失败。
    let op = BatchOp::Resize {
        width: 0,
        height: 0,
        filter: ResizeFilter::default(),
    };
    let res = batch::process(&imgs, &op);
    assert_eq!(res.total(), 3, "失败项也计入总数");
    assert_eq!(res.failures.len(), 3);
    assert!(res.successes.is_empty());
}
