//! `annotate` 模块的 property test。
//!
//! 这是 `rastery-capture` 里唯一能在无头 VM 上自动验证的部分（截图/热键/取色只能真机）。
//! 覆盖标注合成的关键不变量：尺寸不变、空图层恒等、马赛克块内一致、画笔确实着色。

use image::{Rgba, RgbaImage};
use proptest::prelude::*;
use rastery_capture::annotate::{AnnotationLayer, Point, Region};

/// 造一张渐变测试图，避免纯色导致的平凡通过。
fn gradient_image(w: u32, h: u32) -> RgbaImage {
    RgbaImage::from_fn(w, h, |x, y| {
        Rgba([(x % 256) as u8, (y % 256) as u8, ((x + y) % 256) as u8, 255])
    })
}

proptest! {
    /// 空图层是恒等操作：像素分毫不动。
    #[test]
    fn empty_layer_is_identity(w in 1u32..64, h in 1u32..64) {
        let mut img = gradient_image(w, h);
        let original = img.clone();
        AnnotationLayer::new().render(&mut img);
        prop_assert_eq!(img, original);
    }

    /// 渲染不改变图幅（画笔）。
    #[test]
    fn brush_preserves_dimensions(
        w in 4u32..64, h in 4u32..64,
        x0 in 0i32..64, y0 in 0i32..64,
        x1 in 0i32..64, y1 in 0i32..64,
        width in 1u32..12,
    ) {
        let mut img = gradient_image(w, h);
        let mut layer = AnnotationLayer::new();
        layer.add_brush_stroke(
            vec![Point::new(x0, y0), Point::new(x1, y1)],
            width,
            Rgba([255, 0, 0, 255]),
        );
        layer.render(&mut img);
        prop_assert_eq!(img.dimensions(), (w, h));
    }

    /// 渲染不改变图幅（马赛克），即使区域越界。
    #[test]
    fn mosaic_preserves_dimensions(
        w in 4u32..64, h in 4u32..64,
        rx in 0u32..80, ry in 0u32..80,
        rw in 1u32..80, rh in 1u32..80,
        block in 1u32..16,
    ) {
        let mut img = gradient_image(w, h);
        let mut layer = AnnotationLayer::new();
        layer.add_mosaic_region(Region { x: rx, y: ry, width: rw, height: rh }, block).unwrap();
        layer.render(&mut img);
        prop_assert_eq!(img.dimensions(), (w, h));
    }

    /// 不透明画笔盖在某点后，该点像素变为画笔色。
    #[test]
    fn opaque_brush_paints_point(
        w in 8u32..64, h in 8u32..64,
        cx in 2u32..6, cy in 2u32..6,
    ) {
        let mut img = gradient_image(w, h);
        let color = Rgba([200, 30, 40, 255]);
        let mut layer = AnnotationLayer::new();
        // 单点笔画，粗细 1：至少覆盖中心像素。
        layer.add_brush_stroke(vec![Point::new(cx as i32, cy as i32)], 1, color);
        layer.render(&mut img);
        prop_assert_eq!(*img.get_pixel(cx, cy), color);
    }
}

/// block_size = 0 必须报错而非 panic（除零保护）。
#[test]
fn mosaic_zero_block_errors() {
    let mut layer = AnnotationLayer::new();
    let err = layer.add_mosaic_region(Region { x: 0, y: 0, width: 4, height: 4 }, 0);
    assert!(err.is_err());
}

/// 单块覆盖整图时，整图变为同一平均色（块内一致性的极限情形）。
#[test]
fn mosaic_single_block_uniform() {
    let mut img = gradient_image(8, 8);
    let mut layer = AnnotationLayer::new();
    layer
        .add_mosaic_region(Region { x: 0, y: 0, width: 8, height: 8 }, 8)
        .unwrap();
    layer.render(&mut img);
    let first = *img.get_pixel(0, 0);
    for px in img.pixels() {
        assert_eq!(*px, first, "整图单块马赛克后应处处相等");
    }
}
