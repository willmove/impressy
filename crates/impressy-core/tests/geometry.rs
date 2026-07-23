//! 几何变换 property tests：Property 5（切图块数）、6（拼图含全部图）、
//! 7（网格列数）、14（裁剪比例）、15（360° 旋转恒等）。
mod common;

use common::arb_image;
use image::{Rgba, RgbaImage};
use proptest::prelude::*;
use impressy_core::collage::{self, CollageLayout, CollageOptions};
use impressy_core::slice::{self, SliceGrid};
use impressy_core::transform::{self, AspectRatio, CropRect, Rotation};
use std::num::NonZeroU32;

proptest! {
    // Feature: impressy, Property 15: 360-degree rotation is identity
    #[test]
    fn prop_rotate_360_is_identity(img in arb_image()) {
        let r = transform::rotate(&img, Rotation::Cw90);
        let r = transform::rotate(&r, Rotation::Cw90);
        let r = transform::rotate(&r, Rotation::Cw90);
        let r = transform::rotate(&r, Rotation::Cw90);
        prop_assert_eq!(img.as_raw(), r.as_raw());
    }
}

// Feature: impressy, Property 14: Cropped image matches selected aspect ratio (exactly)
#[test]
fn prop_crop_aspect_ratio_exact() {
    proptest!(|(w in 20u32..64, h in 20u32..64)| {
        let img = RgbaImage::from_pixel(w, h, Rgba([10, 20, 30, 255]));
        for &ratio in &AspectRatio::PRESETS {
            if let Ok(rect) = CropRect::largest_centered(w, h, ratio) {
                let cropped = transform::crop(&img, rect).expect("crop");
                // 比例精确相等（largest_centered 用整数倍，误差为 0）。
                prop_assert_eq!(
                    cropped.width() * ratio.height(),
                    cropped.height() * ratio.width()
                );
            }
        }
    });
}

// Feature: impressy, Property 5: Image slicing produces exactly M×N tiles
#[test]
fn prop_slice_tile_count() {
    proptest!(|(w in 3u32..40, h in 3u32..40, r in 1u32..5, c in 1u32..5)| {
        let img = RgbaImage::from_pixel(w, h, Rgba([1, 2, 3, 255]));
        // 仅当图像足以切出该网格时测试。
        if w >= c && h >= r {
            let grid = SliceGrid::new(r, c).expect("grid");
            let tiles = slice::slice(&img, grid).expect("slice");
            prop_assert_eq!(tiles.len(), (r * c) as usize);
            // 序号唯一且覆盖 0..r*c。
            let mut indices: Vec<u32> = tiles.iter().map(|t| t.index).collect();
            indices.sort_unstable();
            prop_assert_eq!(indices, (0..r * c).collect::<Vec<_>>());
        }
    });
}

// Feature: impressy, Property 6: Collage contains all input images (vertical: N cells stacked)
#[test]
fn prop_collage_contains_all_images() {
    proptest!(|(n in 1u32..6, side in 2u32..12)| {
        let imgs: Vec<RgbaImage> = (0..n).map(|i| {
            RgbaImage::from_pixel(side, side, Rgba([i as u8, 0, 0, 255]))
        }).collect();
        let opts = CollageOptions { spacing: 0, background: Rgba([255, 255, 255, 255]) };
        let out = collage::compose(&imgs, CollageLayout::Vertical, opts).expect("compose");
        // spacing=0、等尺寸图纵向排列 → 高度 = N × 单元格高。
        prop_assert_eq!(out.height(), n * side);
        prop_assert_eq!(out.width(), side);
    });
}

// Feature: impressy, Property 7: Grid collage has specified columns
#[test]
fn prop_grid_columns() {
    proptest!(|(n in 1u32..8, cols in 1u32..4, side in 2u32..10)| {
        let imgs: Vec<RgbaImage> = (0..n).map(|_| {
            RgbaImage::from_pixel(side, side, Rgba([9, 9, 9, 255]))
        }).collect();
        let grid = CollageLayout::Grid { columns: NonZeroU32::new(cols).unwrap() };
        let out = collage::compose(&imgs, grid, CollageOptions::default()).expect("compose");
        let expected_rows = n.div_ceil(cols);
        // spacing=0、等尺寸 → 宽 = cols × side，高 = rows × side。
        prop_assert_eq!(out.width(), cols * side);
        prop_assert_eq!(out.height(), expected_rows * side);
    });
}
