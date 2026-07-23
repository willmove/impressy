//! 测试公共辅助：proptest 图像生成策略。
use image::RgbaImage;
use proptest::prelude::*;

/// 生成小型随机 RGBA 图像（最大 24×24），用于 property test。
/// 小尺寸保证 100+ 次迭代快速完成。
pub fn arb_image() -> impl Strategy<Value = RgbaImage> {
    (1u32..=24u32, 1u32..=24u32).prop_flat_map(|(w, h)| {
        let n = w as usize * h as usize * 4;
        proptest::collection::vec(any::<u8>(), n).prop_map(move |data| {
            RgbaImage::from_raw(w, h, data).expect("dimensions match buffer length")
        })
    })
}
