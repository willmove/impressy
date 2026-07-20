//! Deterministic offscreen poster composition.
//!
//! Text shaping belongs to the UI because it needs platform fonts. This module receives already
//! rasterized transparent layers and performs the pixel composition without UI or network state.

use image::{RgbaImage, imageops};

/// A transparent raster layer positioned in output pixels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RasterLayer {
    /// Layer pixels, normally transparent outside the glyphs.
    pub image: RgbaImage,
    /// Left position in output pixels.
    pub x: u32,
    /// Top position in output pixels.
    pub y: u32,
}

/// Composes text or decoration layers over a background.
///
/// Pixels outside the background are clipped by `imageops::overlay`.
pub fn compose(background: &RgbaImage, layers: &[RasterLayer]) -> RgbaImage {
    let mut output = background.clone();
    for layer in layers {
        imageops::overlay(
            &mut output,
            &layer.image,
            i64::from(layer.x),
            i64::from(layer.y),
        );
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    #[test]
    fn composition_alpha_blends_and_clips_layers() {
        let background = RgbaImage::from_pixel(2, 2, Rgba([0, 0, 0, 255]));
        let layer = RasterLayer {
            image: RgbaImage::from_pixel(2, 1, Rgba([255, 255, 255, 128])),
            x: 1,
            y: 1,
        };
        let output = compose(&background, &[layer]);
        assert_eq!(output.dimensions(), (2, 2));
        assert_eq!(output.get_pixel(0, 0), &Rgba([0, 0, 0, 255]));
        assert!(output.get_pixel(1, 1).0[0] > 0);
    }
}
