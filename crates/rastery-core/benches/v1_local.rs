use std::hint::black_box;
use std::time::{Duration, Instant};

use image::{Rgba, RgbaImage};
use rastery_core::batch::{self, BatchOp};
use rastery_core::beautify::{self, Background, BeautifyParams, Border, GradientDirection, Shadow};
use rastery_core::transform::{self, ResizeFilter};

fn patterned_image(width: u32, height: u32) -> RgbaImage {
    RgbaImage::from_fn(width, height, |x, y| {
        Rgba([
            (x.wrapping_mul(17).wrapping_add(y.wrapping_mul(3)) % 256) as u8,
            (x.wrapping_mul(5).wrapping_add(y.wrapping_mul(11)) % 256) as u8,
            (x.wrapping_add(y.wrapping_mul(7)) % 256) as u8,
            255,
        ])
    })
}

fn measure<T>(name: &str, mut operation: impl FnMut() -> T) -> Duration {
    let started = Instant::now();
    black_box(operation());
    let elapsed = started.elapsed();
    println!("{name:<34} {:>10.2} ms", elapsed.as_secs_f64() * 1_000.0);
    elapsed
}

fn main() {
    println!("Rastery v1 core performance probe (release profile, single sample)");
    println!("-----------------------------------------------------------------");

    let large = patterned_image(4_000, 3_000);
    measure("12 MP Lanczos3 resize to 1920x1440", || {
        transform::resize(black_box(&large), 1_920, 1_440, ResizeFilter::Lanczos3)
            .expect("resize probe must succeed")
    });

    let preview = patterned_image(1_920, 1_080);
    let params = BeautifyParams {
        corner_radius: 28,
        inner_padding: 36,
        background: Background::Gradient {
            start: Rgba([48, 74, 188, 255]),
            end: Rgba([199, 73, 143, 255]),
            direction: GradientDirection::Diagonal,
        },
        border: Some(Border {
            width: 2,
            color: Rgba([255, 255, 255, 190]),
        }),
        shadow: Some(Shadow {
            offset: (0, 12),
            blur_sigma: 8.0,
            color: Rgba([0, 0, 0, 110]),
        }),
    };
    measure("1080p beautify preview", || {
        beautify::beautify(black_box(&preview), black_box(&params))
            .expect("beautify probe must succeed")
    });

    let batch_inputs: Vec<_> = (0..100).map(|_| patterned_image(256, 256)).collect();
    let operation = BatchOp::Resize {
        width: 128,
        height: 128,
        filter: ResizeFilter::Bilinear,
    };
    let elapsed = measure("100-image resize and PNG encode", || {
        let result = batch::process(black_box(&batch_inputs), black_box(&operation));
        assert_eq!(result.successes.len(), 100);
        assert!(result.failures.is_empty());
        result
    });
    println!(
        "{:<34} {:>10.2} images/s",
        "batch throughput",
        100.0 / elapsed.as_secs_f64()
    );

    println!("-----------------------------------------------------------------");
    println!("These figures are diagnostic only; desktop acceptance owns UI targets.");
}
