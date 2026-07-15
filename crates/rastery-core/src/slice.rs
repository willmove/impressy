//! 切图：把一张图按网格切成多块。
//!
//! 覆盖 FR-04、Requirement 9、55.4、41.4。

use crate::error::{CoreError, Result};
use image::RgbaImage;
use std::num::NonZeroU32;

/// 切分网格（Requirement 9.2–9.4）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SliceGrid {
    /// 行数。
    pub rows: NonZeroU32,
    /// 列数。
    pub columns: NonZeroU32,
}

impl SliceGrid {
    /// 九宫格 3×3 预设（Requirement 9.2）。
    pub fn three_by_three() -> Self {
        // 3 是非零常量，unwrap 不可能失败；用 expect 保持 `unsafe_code = "forbid"`。
        Self {
            rows: NonZeroU32::new(3).expect("3 非零"),
            columns: NonZeroU32::new(3).expect("3 非零"),
        }
    }

    /// 自定义行列数（Requirement 9.3、9.4）。
    pub fn new(rows: u32, columns: u32) -> Result<Self> {
        let rows = NonZeroU32::new(rows)
            .ok_or_else(|| CoreError::InvalidArgument("行数必须为正".to_string()))?;
        let columns = NonZeroU32::new(columns)
            .ok_or_else(|| CoreError::InvalidArgument("列数必须为正".to_string()))?;
        Ok(Self { rows, columns })
    }

    /// 该网格会切出多少块。
    pub fn tile_count(self) -> u32 {
        self.rows.get() * self.columns.get()
    }
}

/// 一块切片及其网格位置。
#[derive(Debug, Clone)]
pub struct Tile {
    /// 行号，从 0 开始。
    pub row: u32,
    /// 列号，从 0 开始。
    pub column: u32,
    /// 序号，从 0 开始，按行优先。用于 Requirement 9.6 的顺序编号。
    pub index: u32,
    /// 图像数据。
    pub image: RgbaImage,
}

impl Tile {
    /// 建议的文件名后缀，形如 `_1_r0c0`（Requirement 41.4 要求带网格位置标识）。
    ///
    /// 同时含顺序号与行列号：顺序号满足 Requirement 9.6「顺序编号」，
    /// 行列号满足 Requirement 41.4「网格位置标识」。
    pub fn filename_suffix(&self) -> String {
        format!("_{}_r{}c{}", self.index + 1, self.row, self.column)
    }
}

/// 按网格切分图像（Requirement 9.5）。
///
/// **保证切出恰好 `rows × columns` 块**（Requirement 55.4）。图像尺寸不能整除时，
/// 余数分摊给靠前的行/列（每块最多多 1 像素），因此所有块拼回去正好是原图，
/// 不丢像素、不重叠。
///
/// 若图像太小以致某行或某列会得到 0 像素（如 2×2 图切 3×3），返回错误——
/// 返回空白块会让用户以为切图成功了。
pub fn slice(img: &RgbaImage, grid: SliceGrid) -> Result<Vec<Tile>> {
    let (rows, cols) = (grid.rows.get(), grid.columns.get());
    if img.width() < cols || img.height() < rows {
        return Err(CoreError::InvalidArgument(format!(
            "{}×{} 的图像切不出 {}行×{}列（每块至少 1 像素）",
            img.width(),
            img.height(),
            rows,
            cols
        )));
    }

    let mut tiles = Vec::with_capacity(grid.tile_count() as usize);
    for row in 0..rows {
        let (y, h) = span(img.height(), rows, row);
        for column in 0..cols {
            let (x, w) = span(img.width(), cols, column);
            let image = image::imageops::crop_imm(img, x, y, w, h).to_image();
            tiles.push(Tile {
                row,
                column,
                index: row * cols + column,
                image,
            });
        }
    }
    Ok(tiles)
}

/// 把 `total` 像素分成 `divisions` 份，返回第 `index` 份的 (起点, 长度)。
///
/// 余数分给靠前的份，保证各份首尾相接、总和等于 `total`。
fn span(total: u32, divisions: u32, index: u32) -> (u32, u32) {
    let base = total / divisions;
    let remainder = total % divisions;
    // 前 `remainder` 份各多 1 像素
    let start = index * base + index.min(remainder);
    let len = base + u32::from(index < remainder);
    (start, len)
}
