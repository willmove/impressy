//! 图像工作区：把 v1 功能页的「打开 → 处理 → 预览 → 保存」流程接到 `rastery-core`。
//!
//! 文件选择用 `rfd` 的**阻塞**原生对话框（Linux 走 xdg-portal，Win/macOS 走系统原生），
//! 编解码与所有图像变换全部委托 `rastery-core`——本模块不含任何图像算法，只做编排。
//!
//! **环境约束（ADR-0002）**：无头 VM 上可编译，但对话框弹出、预览渲染、实际读写效果
//! 须在真机验收。核心变换的正确性由 `rastery-core` 的测试保证，本模块只负责把它们串起来。

use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpui::{RenderImage, SharedString};
use image::{Frame, RgbaImage};

use rastery_core::animation::{self, GifParams, Playback};
use rastery_core::batch::{self, BatchOp};
use rastery_core::beautify::{self, Background, BeautifyParams};
use rastery_core::collage::{self, CollageLayout, CollageOptions};
use rastery_core::format::{self, EncodeSettings, OutputFormat, PngCompression};
use rastery_core::slice::{self, SliceGrid};
use rastery_core::transform::{self, AspectRatio, CropRect, Rotation};
use rastery_core::{exif, qr};

/// 支持打开的图片扩展名。
const IMAGE_EXTS: &[&str] = &["png", "jpg", "jpeg", "webp", "bmp", "gif"];

/// 单个功能页共享的图像工作区状态。
#[derive(Default)]
pub struct Workspace {
    /// 已载入的输入图（单图操作用第 0 张，拼图 / GIF / 批量用全部）。
    images: Vec<RgbaImage>,
    /// 第一张输入文件的原始字节，供 EXIF 容器层清除（像素不变）使用。
    source_bytes: Option<Vec<u8>>,
    /// 处理结果图（可保存 / 预览）。
    result_image: Option<RgbaImage>,
    /// 处理结果的编码字节（GIF 等直接产出字节的操作）。
    result_bytes: Option<(Vec<u8>, &'static str)>,
    /// 当前预览（GPUI 的 BGRA 图）。
    preview: Option<Arc<RenderImage>>,
    /// 面向用户的状态 / 错误文案（Requirement 36）。
    pub status: SharedString,
    /// 图像信息（尺寸等）。
    pub info: SharedString,
}

impl Workspace {
    /// 当前预览图（供 `img()` 渲染）。
    pub fn preview(&self) -> Option<Arc<RenderImage>> {
        self.preview.clone()
    }

    /// 打开单张图片。
    pub fn open_single(&mut self) {
        let Some(path) = pick_open_file() else {
            self.status = "已取消打开".into();
            return;
        };
        match load_image(&path) {
            Ok((img, bytes)) => {
                self.info = format!("{} · {}×{}", file_name(&path), img.width(), img.height()).into();
                self.set_preview(&img);
                self.source_bytes = Some(bytes);
                self.images = vec![img];
                self.result_image = None;
                self.result_bytes = None;
                self.status = "已载入图片".into();
            }
            Err(e) => self.status = format!("打开失败：{e}").into(),
        }
    }

    /// 打开多张图片（拼图 / GIF / 批量）。
    pub fn open_multiple(&mut self) {
        let Some(paths) = pick_open_files() else {
            self.status = "已取消打开".into();
            return;
        };
        let mut images = Vec::new();
        let mut failures = 0usize;
        for p in &paths {
            match load_image(p) {
                Ok((img, _)) => images.push(img),
                Err(_) => failures += 1,
            }
        }
        if images.is_empty() {
            self.status = "没有可用图片".into();
            return;
        }
        self.set_preview(&images[0]);
        self.info = format!("已载入 {} 张（{} 张失败）", images.len(), failures).into();
        self.source_bytes = None;
        self.images = images;
        self.result_image = None;
        self.result_bytes = None;
        self.status = "已载入多张图片".into();
    }

    /// 保存当前结果。优先保存编码字节（GIF 等），否则把结果图编码为 PNG。
    pub fn save_result(&mut self) {
        if let Some((bytes, ext)) = &self.result_bytes {
            match pick_save_file(ext).map(|p| std::fs::write(&p, bytes)) {
                Some(Ok(())) => self.status = "已保存".into(),
                Some(Err(e)) => self.status = format!("保存失败：{e}").into(),
                None => self.status = "已取消保存".into(),
            }
            return;
        }
        let Some(img) = &self.result_image else {
            self.status = "没有可保存的结果，请先执行操作".into();
            return;
        };
        match format::encode(img, EncodeSettings::Png { compression: PngCompression::Default }) {
            Ok(bytes) => match pick_save_file("png").map(|p| std::fs::write(&p, &bytes)) {
                Some(Ok(())) => self.status = "已保存 PNG".into(),
                Some(Err(e)) => self.status = format!("保存失败：{e}").into(),
                None => self.status = "已取消保存".into(),
            },
            Err(e) => self.status = format!("编码失败：{e}").into(),
        }
    }

    /// 旋转 90°（FR-01）。
    pub fn rotate90(&mut self) {
        self.with_first(|img| Ok(transform::rotate(img, Rotation::Cw90)));
    }

    /// 裁剪为 1:1 居中最大区域（FR-01）。
    pub fn crop_square(&mut self) {
        self.with_first(|img| {
            let rect = CropRect::largest_centered(img.width(), img.height(), AspectRatio::SQUARE)
                .map_err(|e| e.to_string())?;
            transform::crop(img, rect).map_err(|e| e.to_string())
        });
    }

    /// 清除 EXIF（FR-06，容器层，像素不变）。
    pub fn strip_exif(&mut self) {
        let Some(bytes) = &self.source_bytes else {
            self.status = "请先打开一张带 EXIF 的图片".into();
            return;
        };
        match exif::strip(bytes) {
            Ok(clean) => {
                // 解码干净字节用于预览，并把字节留作保存（保持像素与格式）。
                if let Ok(img) = format::decode(&clean) {
                    self.set_preview(&img);
                }
                let ext = detect_ext(&clean);
                self.result_bytes = Some((clean, ext));
                self.result_image = None;
                self.status = "已清除 EXIF".into();
            }
            Err(e) => self.status = format!("清除失败：{e}").into(),
        }
    }

    /// 默认参数截图美化（FR-07）。
    pub fn beautify_default(&mut self) {
        self.with_first(|img| {
            let params = BeautifyParams {
                corner_radius: 24,
                inner_padding: 48,
                background: Background::Solid(image::Rgba([240, 240, 245, 255])),
                border: None,
                shadow: None,
            };
            beautify::beautify(img, &params).map_err(|e| e.to_string())
        });
    }

    /// 识别图中的二维码（FR-05），结果显示为文本。
    pub fn decode_qr(&mut self) {
        let Some(img) = self.images.first() else {
            self.status = "请先打开二维码图片".into();
            return;
        };
        match qr::decode(img) {
            Ok(text) => {
                self.info = format!("识别结果：{text}").into();
                self.status = "识别成功".into();
            }
            Err(e) => self.status = format!("未识别到二维码：{e}").into(),
        }
    }

    /// 纵向拼接已载入的多张图（FR-02）。
    pub fn collage_vertical(&mut self) {
        if self.images.is_empty() {
            self.status = "请先打开多张图片".into();
            return;
        }
        match collage::compose(&self.images, CollageLayout::Vertical, CollageOptions::default()) {
            Ok(out) => {
                self.set_preview(&out);
                self.result_image = Some(out);
                self.result_bytes = None;
                self.status = "已纵向拼接".into();
            }
            Err(e) => self.status = format!("拼接失败：{e}").into(),
        }
    }

    /// 九宫格切图（FR-04），保存到所选目录。
    pub fn slice_3x3(&mut self) {
        let Some(img) = self.images.first() else {
            self.status = "请先打开图片".into();
            return;
        };
        let tiles = match slice::slice(img, SliceGrid::three_by_three()) {
            Ok(t) => t,
            Err(e) => {
                self.status = format!("切图失败：{e}").into();
                return;
            }
        };
        let Some(dir) = pick_folder() else {
            self.status = "已取消保存".into();
            return;
        };
        let mut ok = 0usize;
        for tile in &tiles {
            let Ok(bytes) =
                format::encode(&tile.image, EncodeSettings::Png { compression: PngCompression::Default })
            else {
                continue;
            };
            let path = dir.join(format!("tile{}.png", tile.filename_suffix()));
            if std::fs::write(&path, &bytes).is_ok() {
                ok += 1;
            }
        }
        self.status = format!("已保存 {ok}/{} 块到所选目录", tiles.len()).into();
    }

    /// 把已载入的多张图合成 GIF（FR-10）。
    pub fn make_gif(&mut self) {
        if self.images.is_empty() {
            self.status = "请先打开多张图片".into();
            return;
        }
        let params = GifParams {
            frame_delay_ms: 200,
            dimensions: None,
            playback: Playback::Forward,
        };
        match animation::compose(&self.images, &params) {
            Ok(bytes) => {
                if let Some(first) = self.images.first() {
                    let f = first.clone();
                    self.set_preview(&f);
                }
                self.result_bytes = Some((bytes, "gif"));
                self.result_image = None;
                self.status = "已合成 GIF，可保存".into();
            }
            Err(e) => self.status = format!("合成失败：{e}").into(),
        }
    }

    /// 批量转 PNG（FR-03），保存到所选目录。
    pub fn batch_to_png(&mut self) {
        if self.images.is_empty() {
            self.status = "请先打开多张图片".into();
            return;
        }
        let op = BatchOp::Convert {
            format: OutputFormat::Png,
            settings: EncodeSettings::Png { compression: PngCompression::Default },
        };
        let result = batch::process(&self.images, &op);
        let Some(dir) = pick_folder() else {
            self.status = "已取消保存".into();
            return;
        };
        let mut ok = 0usize;
        for (index, output) in &result.successes {
            let path = dir.join(format!("{index}.png"));
            if std::fs::write(&path, &output.bytes).is_ok() {
                ok += 1;
            }
        }
        self.status =
            format!("成功 {ok}，失败 {} 项（共 {}）", result.failures.len(), result.total()).into();
    }

    /// 对第一张输入图套用一个产出 [`RgbaImage`] 的操作，统一处理错误与预览。
    fn with_first(&mut self, op: impl FnOnce(&RgbaImage) -> Result<RgbaImage, String>) {
        let Some(img) = self.images.first() else {
            self.status = "请先打开一张图片".into();
            return;
        };
        match op(img) {
            Ok(out) => {
                self.set_preview(&out);
                self.result_image = Some(out);
                self.result_bytes = None;
                self.status = "已处理，可保存".into();
            }
            Err(e) => self.status = format!("处理失败：{e}").into(),
        }
    }

    /// 用给定图更新预览。
    fn set_preview(&mut self, img: &RgbaImage) {
        self.preview = Some(to_render_image(img));
    }
}

/// 把 `rastery-core` 的 RGBA 图转成 GPUI 的 [`RenderImage`]（BGRA 序）。
pub(crate) fn to_render_image(img: &RgbaImage) -> Arc<RenderImage> {
    let mut bgra = img.clone();
    for px in bgra.pixels_mut() {
        px.0.swap(0, 2); // RGBA → BGRA
    }
    Arc::new(RenderImage::new(vec![Frame::new(bgra)]))
}

/// 读取并解码一张图，返回 (解码图, 原始字节)。
fn load_image(path: &Path) -> Result<(RgbaImage, Vec<u8>), String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let img = format::decode(&bytes).map_err(|e| e.to_string())?;
    Ok((img, bytes))
}

/// 按魔数猜测清除 EXIF 后应使用的扩展名。
fn detect_ext(bytes: &[u8]) -> &'static str {
    match image::guess_format(bytes) {
        Ok(image::ImageFormat::Png) => "png",
        Ok(image::ImageFormat::WebP) => "webp",
        _ => "jpg",
    }
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "图片".to_string())
}

fn pick_open_file() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("图片", IMAGE_EXTS)
        .pick_file()
}

fn pick_open_files() -> Option<Vec<PathBuf>> {
    rfd::FileDialog::new()
        .add_filter("图片", IMAGE_EXTS)
        .pick_files()
}

fn pick_save_file(ext: &str) -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter(ext, &[ext])
        .set_file_name(format!("rastery-output.{ext}"))
        .save_file()
}

fn pick_folder() -> Option<PathBuf> {
    rfd::FileDialog::new().pick_folder()
}
