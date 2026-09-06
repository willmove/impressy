//! 批处理 property tests：Property 8（数量守恒）、9（顺序）、10（格式转换保尺寸）、
//! 11（压缩保尺寸）、12（缩放保数量）、13（水印保数量）。
mod common;

use common::arb_image;
use image::{Rgba, RgbaImage};
use impressy_core::batch::{self, BatchOp, BatchPipeline, BatchResizeMode, BatchStep};
use impressy_core::format::{self, EncodeSettings, OutputFormat, Quality};
use impressy_core::transform::ResizeFilter;
use impressy_core::watermark::Position;
use proptest::prelude::*;

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

    #[test]
    fn prop_relative_batch_resize_resolves_per_input(
        width in 2u32..64,
        height in 2u32..64,
        percentage in 1u32..=200,
        longest in 2u32..=96,
    ) {
        let input = RgbaImage::from_pixel(width, height, Rgba([1, 2, 3, 255]));
        let run = |mode| {
            batch::process_pipeline(
                std::slice::from_ref(&input),
                &BatchPipeline {
                    steps: vec![BatchStep::Resize {
                        mode,
                        filter: ResizeFilter::Nearest,
                        prevent_enlarge: false,
                    }],
                    output: EncodeSettings::Png { compression: Default::default() },
                },
            )
        };

        let percentage_result = run(BatchResizeMode::Percentage(percentage));
        let output = &percentage_result.successes[0].1;
        let expected = |value| {
            ((u64::from(value) * u64::from(percentage) + 50) / 100).max(1) as u32
        };
        prop_assert_eq!((output.width, output.height), (expected(width), expected(height)));

        let longest_result = run(BatchResizeMode::LongestSide(longest));
        let output = &longest_result.successes[0].1;
        prop_assert_eq!(output.width.max(output.height), longest);
        if width >= height {
            prop_assert_eq!(output.width, longest);
        } else {
            prop_assert_eq!(output.height, longest);
        }
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

#[test]
fn ordered_pipeline_applies_every_step_before_output_encoding() {
    let input = RgbaImage::from_pixel(8, 4, Rgba([10, 20, 30, 255]));
    let watermark = RgbaImage::from_pixel(2, 2, Rgba([250, 5, 5, 255]));
    let pipeline = BatchPipeline {
        steps: vec![
            BatchStep::Resize {
                mode: BatchResizeMode::Pixels {
                    width: 4,
                    height: 2,
                    preserve_aspect: true,
                },
                filter: ResizeFilter::Nearest,
                prevent_enlarge: true,
            },
            BatchStep::Watermark {
                overlay: watermark,
                opacity: 1.0,
                position: Position::BottomRight { margin: 0 },
            },
        ],
        output: EncodeSettings::Jpeg {
            quality: Quality::new(90).expect("quality"),
        },
    };

    let result = batch::process_pipeline(std::slice::from_ref(&input), &pipeline);
    let output = &result.successes[0].1;
    let decoded = format::decode(&output.bytes).expect("decode pipeline output");

    assert_eq!(output.format, OutputFormat::Jpeg);
    assert_eq!(decoded.dimensions(), (4, 2));
}

#[test]
fn changing_pipeline_step_order_changes_the_pixel_result() {
    let input = RgbaImage::from_pixel(8, 4, Rgba([0, 0, 0, 255]));
    let watermark = RgbaImage::from_pixel(2, 2, Rgba([255, 0, 0, 255]));
    let resize = BatchStep::Resize {
        mode: BatchResizeMode::Pixels {
            width: 4,
            height: 2,
            preserve_aspect: true,
        },
        filter: ResizeFilter::Nearest,
        prevent_enlarge: false,
    };
    let watermark = BatchStep::Watermark {
        overlay: watermark,
        opacity: 1.0,
        position: Position::BottomRight { margin: 0 },
    };
    let output = EncodeSettings::Png {
        compression: Default::default(),
    };
    let run = |steps| {
        let result = batch::process_pipeline(
            std::slice::from_ref(&input),
            &BatchPipeline { steps, output },
        );
        format::decode(&result.successes[0].1.bytes).expect("decode pipeline output")
    };

    let resize_then_watermark = run(vec![resize.clone(), watermark.clone()]);
    let watermark_then_resize = run(vec![watermark, resize]);

    assert_ne!(resize_then_watermark, watermark_then_resize);
}

#[test]
fn pipeline_prevent_enlarge_keeps_smaller_input_dimensions() {
    let input = RgbaImage::from_pixel(8, 4, Rgba([10, 20, 30, 255]));
    let pipeline = BatchPipeline {
        steps: vec![BatchStep::Resize {
            mode: BatchResizeMode::Pixels {
                width: 80,
                height: 40,
                preserve_aspect: true,
            },
            filter: ResizeFilter::Nearest,
            prevent_enlarge: true,
        }],
        output: EncodeSettings::Png {
            compression: Default::default(),
        },
    };

    let result = batch::process_pipeline(std::slice::from_ref(&input), &pipeline);
    let decoded = format::decode(&result.successes[0].1.bytes).expect("decode pipeline output");
    assert_eq!(decoded.dimensions(), input.dimensions());
}

#[test]
fn pipeline_longest_side_and_percentage_preserve_each_input_ratio() {
    let inputs = vec![
        RgbaImage::from_pixel(400, 200, Rgba([1, 2, 3, 255])),
        RgbaImage::from_pixel(100, 300, Rgba([1, 2, 3, 255])),
    ];
    let longest = batch::process_pipeline(
        &inputs,
        &BatchPipeline {
            steps: vec![BatchStep::Resize {
                mode: BatchResizeMode::LongestSide(100),
                filter: ResizeFilter::Nearest,
                prevent_enlarge: false,
            }],
            output: EncodeSettings::Png {
                compression: Default::default(),
            },
        },
    );
    assert_eq!(
        longest
            .successes
            .iter()
            .map(|(_, output)| (output.width, output.height))
            .collect::<Vec<_>>(),
        vec![(100, 50), (33, 100)]
    );

    let percentage = batch::process_pipeline(
        &inputs,
        &BatchPipeline {
            steps: vec![BatchStep::Resize {
                mode: BatchResizeMode::Percentage(50),
                filter: ResizeFilter::Nearest,
                prevent_enlarge: false,
            }],
            output: EncodeSettings::Png {
                compression: Default::default(),
            },
        },
    );
    assert_eq!(
        percentage
            .successes
            .iter()
            .map(|(_, output)| (output.width, output.height))
            .collect::<Vec<_>>(),
        vec![(200, 100), (50, 150)]
    );
}
