//! 图像工作区与后台任务。
//!
//! 文件对话框在 UI 线程上只负责选择路径；文件读取、编解码和所有图像变换封装为
//! [`WorkspaceJob`]，由 `AppShell` 投递到 GPUI background executor。完成后只在主线程
//! 应用状态和刷新预览（Requirement 3、37）。

use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpui::{RenderImage, SharedString};
use image::{Frame, RgbaImage};

use rastery_core::animation::{self, GifParams};
use rastery_core::batch::{self, BatchOp};
use rastery_core::beautify::{self, BeautifyParams};
use rastery_core::collage::{self, CollageLayout, CollageOptions};
use rastery_core::format::{self, EncodeSettings, OutputFormat, PngCompression};
use rastery_core::naming;
use rastery_core::qr::QrOptions;
use rastery_core::slice::{self, SliceGrid};
use rastery_core::transform::{self, CropRect, ResizeFilter, Rotation};
use rastery_core::watermark;
use rastery_core::{CoreError, exif, qr};
use rust_i18n::t;

use crate::feature_params::{WatermarkPlacement, WatermarkSource};
use crate::text_watermark;
use crate::ui_message::{BatchItemFailure, ErrorKind, InfoMessage, UiMessage};

const IMAGE_EXTS: &[&str] = &["png", "jpg", "jpeg", "webp", "bmp", "gif"];

pub(crate) fn is_supported_image_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            IMAGE_EXTS
                .iter()
                .any(|candidate| extension.eq_ignore_ascii_case(candidate))
        })
}

/// 当前功能页可触发的工作区命令。
#[derive(Debug, Clone)]
pub(crate) enum WorkspaceCommand {
    OpenSingle,
    OpenMultiple,
    SaveResult,
    Transform(TransformOperation),
    ReadExif,
    StripExif,
    GenerateQr {
        text: String,
        options: QrOptions,
    },
    DecodeQr,
    Collage {
        layout: CollageLayout,
        options: CollageOptions,
    },
    Slice(SliceGrid),
    MakeGif(GifParams),
    Batch(BatchRequest),
}

pub(crate) enum WorkspaceCommandRoute {
    OpenImages { multiple: bool },
    SaveResult,
    OutputDirectory(OutputDirectoryCommand),
    Direct(WorkspaceOperation),
}

pub(crate) enum OutputDirectoryCommand {
    Slice(SliceGrid),
    Batch(BatchRequest),
}

pub(crate) enum WorkspaceOperation {
    Transform(TransformOperation),
    ReadExif,
    StripExif,
    GenerateQr {
        text: String,
        options: QrOptions,
    },
    DecodeQr,
    Collage {
        layout: CollageLayout,
        options: CollageOptions,
    },
    MakeGif(GifParams),
}

impl WorkspaceCommand {
    pub fn route(self) -> WorkspaceCommandRoute {
        match self {
            Self::OpenSingle => WorkspaceCommandRoute::OpenImages { multiple: false },
            Self::OpenMultiple => WorkspaceCommandRoute::OpenImages { multiple: true },
            Self::SaveResult => WorkspaceCommandRoute::SaveResult,
            Self::Slice(grid) => {
                WorkspaceCommandRoute::OutputDirectory(OutputDirectoryCommand::Slice(grid))
            }
            Self::Batch(request) => {
                WorkspaceCommandRoute::OutputDirectory(OutputDirectoryCommand::Batch(request))
            }
            Self::Transform(operation) => {
                WorkspaceCommandRoute::Direct(WorkspaceOperation::Transform(operation))
            }
            Self::ReadExif => WorkspaceCommandRoute::Direct(WorkspaceOperation::ReadExif),
            Self::StripExif => WorkspaceCommandRoute::Direct(WorkspaceOperation::StripExif),
            Self::GenerateQr { text, options } => {
                WorkspaceCommandRoute::Direct(WorkspaceOperation::GenerateQr { text, options })
            }
            Self::DecodeQr => WorkspaceCommandRoute::Direct(WorkspaceOperation::DecodeQr),
            Self::Collage { layout, options } => {
                WorkspaceCommandRoute::Direct(WorkspaceOperation::Collage { layout, options })
            }
            Self::MakeGif(params) => {
                WorkspaceCommandRoute::Direct(WorkspaceOperation::MakeGif(params))
            }
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) enum BatchRequest {
    Convert {
        format: OutputFormat,
        settings: EncodeSettings,
    },
    Resize {
        width: u32,
        height: u32,
    },
    Watermark {
        source: WatermarkSource,
        text: String,
        opacity: f32,
        placement: WatermarkPlacement,
    },
}

/// 单个功能页共享的图像工作区状态。
#[derive(Default)]
pub(crate) struct Workspace {
    images: Arc<Vec<RgbaImage>>,
    source_names: Arc<Vec<String>>,
    source_bytes: Option<Arc<Vec<u8>>>,
    result_image: Option<Arc<RgbaImage>>,
    result_bytes: Option<Arc<EncodedResult>>,
    preview: Option<Arc<RenderImage>>,
    preview_dimensions: Option<(u32, u32)>,
    status: UiMessage,
    info: InfoMessage,
    busy: bool,
}

#[derive(Clone)]
pub(crate) enum ImageSource {
    Result(Arc<RgbaImage>),
    Input {
        images: Arc<Vec<RgbaImage>>,
        index: usize,
    },
}

impl ImageSource {
    pub fn image(&self) -> &RgbaImage {
        match self {
            Self::Result(image) => image,
            Self::Input { images, index } => &images[*index],
        }
    }
}

#[derive(Debug)]
pub(crate) struct EncodedResult {
    bytes: Vec<u8>,
    extension: &'static str,
}

impl Workspace {
    pub fn preview(&self) -> Option<Arc<RenderImage>> {
        self.preview.clone()
    }

    pub fn preview_size(&self, max_width: f32, max_height: f32) -> Option<(f32, f32)> {
        let (width, height) = self.preview_dimensions?;
        let scale = (max_width / width as f32)
            .min(max_height / height as f32)
            .min(1.0);
        Some((width as f32 * scale, height as f32 * scale))
    }

    pub fn current_dimensions(&self) -> Option<(u32, u32)> {
        self.result_image
            .as_deref()
            .map(RgbaImage::dimensions)
            .or_else(|| self.images.first().map(RgbaImage::dimensions))
    }

    pub fn has_image(&self) -> bool {
        self.current_dimensions().is_some()
    }

    pub fn image_for_estimate(&self) -> Option<ImageSource> {
        self.current_image_source()
    }

    pub fn status_text(&self) -> SharedString {
        self.status.text()
    }

    pub fn info_text(&self) -> SharedString {
        self.info.text()
    }

    pub fn is_busy(&self) -> bool {
        self.busy
    }

    pub fn set_status(&mut self, status: UiMessage) {
        self.status = status;
    }

    /// 在主线程完成输入校验，返回可安全发送到后台的纯任务。
    ///
    /// 需要系统路径选择器的命令必须先由 `AppShell` 异步解析为具体路径，再调用下方
    /// 对应的 `prepare_*` 方法。这里不会启动原生对话框。
    pub fn prepare(&mut self, operation: WorkspaceOperation) -> Option<WorkspaceJob> {
        if self.busy {
            self.status = UiMessage::Busy;
            return None;
        }

        let job = match operation {
            WorkspaceOperation::Transform(operation) => {
                let image = if matches!(operation, TransformOperation::Beautify(_)) {
                    self.source_image_or_status(UiMessage::NeedImage)?
                } else {
                    self.current_image_or_status(UiMessage::NeedImage)?
                };
                WorkspaceJob::Transform { image, operation }
            }
            WorkspaceOperation::ReadExif => {
                let Some(bytes) = &self.source_bytes else {
                    self.status = UiMessage::NeedExifImage;
                    return None;
                };
                WorkspaceJob::ReadExif {
                    bytes: Arc::clone(bytes),
                }
            }
            WorkspaceOperation::StripExif => {
                let Some(bytes) = &self.source_bytes else {
                    self.status = UiMessage::NeedExifImage;
                    return None;
                };
                WorkspaceJob::StripExif {
                    bytes: Arc::clone(bytes),
                }
            }
            WorkspaceOperation::GenerateQr { text, options } => {
                WorkspaceJob::GenerateQr { text, options }
            }
            WorkspaceOperation::DecodeQr => WorkspaceJob::DecodeQr {
                images: self.first_images_with(UiMessage::NeedQrImage)?,
            },
            WorkspaceOperation::Collage { layout, options } => WorkspaceJob::Collage {
                images: self.all_images()?,
                layout,
                options,
            },
            WorkspaceOperation::MakeGif(params) => WorkspaceJob::Gif {
                images: self.all_images()?,
                params,
            },
        };

        self.start_job(job)
    }

    /// 从系统剪贴板取得编码字节后，准备后台解码任务。
    pub fn prepare_paste(&mut self, bytes: Vec<u8>) -> Option<WorkspaceJob> {
        if self.busy {
            self.status = UiMessage::Busy;
            return None;
        }
        self.busy = true;
        self.status = UiMessage::Processing;
        Some(WorkspaceJob::LoadClipboard { bytes })
    }

    /// 准备由操作系统拖放进来的路径；`multiple` 决定是否保留全部有效图片。
    pub fn prepare_paths(&mut self, paths: Vec<PathBuf>, multiple: bool) -> Option<WorkspaceJob> {
        if self.busy {
            self.status = UiMessage::Busy;
            return None;
        }
        let paths = paths
            .into_iter()
            .filter(|path| is_supported_image_path(path))
            .collect::<Vec<_>>();
        if paths.is_empty() {
            self.status = UiMessage::NeedImage;
            return None;
        }
        self.start_job(WorkspaceJob::LoadPaths {
            paths,
            single: !multiple,
        })
    }

    /// 用异步保存 prompt 返回的具体路径准备保存任务。
    pub fn prepare_save_path(
        &mut self,
        path: PathBuf,
        default_export: EncodeSettings,
    ) -> Option<WorkspaceJob> {
        if self.busy {
            self.status = UiMessage::Busy;
            return None;
        }
        let source = if let Some(result) = &self.result_bytes {
            SaveSource::Bytes {
                bytes: Arc::clone(result),
            }
        } else if let Some(image) = self.current_image_source() {
            SaveSource::Image {
                image,
                settings: default_export,
            }
        } else {
            self.status = UiMessage::NeedResult;
            return None;
        };
        self.start_job(WorkspaceJob::Save { source, path })
    }

    /// 用异步目录 prompt 返回的具体路径准备切图任务。
    pub fn prepare_slice_directory(
        &mut self,
        grid: SliceGrid,
        directory: PathBuf,
    ) -> Option<WorkspaceJob> {
        if self.busy {
            self.status = UiMessage::Busy;
            return None;
        }
        let images = self.first_images()?;
        self.start_job(WorkspaceJob::Slice {
            images,
            grid,
            directory,
        })
    }

    /// 用异步 prompt 返回的输出目录和可选图片水印路径准备批处理任务。
    pub fn prepare_batch_paths(
        &mut self,
        request: BatchRequest,
        directory: PathBuf,
        watermark_path: Option<PathBuf>,
    ) -> Option<WorkspaceJob> {
        if self.busy {
            self.status = UiMessage::Busy;
            return None;
        }
        let images = self.all_images()?;
        let names = Arc::clone(&self.source_names);
        let operation = match request {
            BatchRequest::Convert { format, settings } => {
                BatchOperation::Ready(BatchOp::Convert { format, settings })
            }
            BatchRequest::Resize { width, height } => BatchOperation::Ready(BatchOp::Resize {
                width,
                height,
                filter: ResizeFilter::default(),
            }),
            BatchRequest::Watermark {
                source,
                text,
                opacity,
                placement,
            } => {
                let position = match placement {
                    WatermarkPlacement::BottomRight => {
                        watermark::Position::BottomRight { margin: 24 }
                    }
                    WatermarkPlacement::Tiled => watermark::Position::Tiled { spacing: 64 },
                };
                match source {
                    WatermarkSource::Text => BatchOperation::TextWatermark {
                        text,
                        opacity,
                        position,
                    },
                    WatermarkSource::Image => {
                        let Some(path) =
                            watermark_path.filter(|path| is_supported_image_path(path))
                        else {
                            self.status = UiMessage::NeedImage;
                            return None;
                        };
                        BatchOperation::ImageWatermark {
                            path,
                            opacity,
                            position,
                        }
                    }
                }
            }
        };
        self.start_job(WorkspaceJob::Batch {
            images,
            names,
            operation,
            directory,
        })
    }

    /// 当前结果建议使用的保存文件名。
    pub fn suggested_save_name(&self, default_export: EncodeSettings) -> String {
        let extension = self
            .result_bytes
            .as_deref()
            .map(|result| result.extension)
            .unwrap_or_else(|| default_export.format().extension());
        t!("dialog.output_filename", extension = extension).to_string()
    }

    /// 准备把当前结果编码后复制到系统剪贴板。
    pub fn prepare_copy(&mut self) -> Option<WorkspaceJob> {
        if self.busy {
            self.status = UiMessage::Busy;
            return None;
        }
        let job = if let Some(result) = &self.result_bytes {
            WorkspaceJob::CopyBytes {
                bytes: Arc::clone(result),
            }
        } else if let Some(image) = self.current_image_source() {
            WorkspaceJob::CopyImage { image }
        } else {
            self.status = UiMessage::NeedResult;
            return None;
        };
        self.busy = true;
        self.status = UiMessage::Processing;
        Some(job)
    }

    /// 把后台任务结果应用到 UI 状态，返回需要由 App context 执行的副作用。
    pub fn apply(&mut self, outcome: WorkspaceOutcome) -> WorkspaceEffects {
        self.busy = false;
        let mut effects = WorkspaceEffects::default();
        match outcome {
            WorkspaceOutcome::Loaded {
                images,
                source_names,
                source_bytes,
                info,
                status,
            } => {
                if let Some(first) = images.first() {
                    self.preview = Some(to_render_image(first));
                    self.preview_dimensions = Some(first.dimensions());
                }
                self.images = Arc::new(images);
                self.source_names = Arc::new(source_names);
                self.source_bytes = source_bytes.map(Arc::new);
                self.result_image = None;
                self.result_bytes = None;
                self.info = info;
                self.status = status;
            }
            WorkspaceOutcome::Image { image, status } => {
                let image = Arc::new(image);
                self.preview = Some(to_render_image(&image));
                self.preview_dimensions = Some(image.dimensions());
                self.result_image = Some(image);
                self.result_bytes = None;
                self.status = status;
            }
            WorkspaceOutcome::Bytes {
                bytes,
                extension,
                preview,
                status,
            } => {
                if let Some(preview) = preview {
                    self.preview = Some(to_render_image(&preview));
                    self.preview_dimensions = Some(preview.dimensions());
                }
                self.result_bytes = Some(Arc::new(EncodedResult { bytes, extension }));
                self.result_image = None;
                self.status = status;
            }
            WorkspaceOutcome::Info { info, status } => {
                self.info = info;
                self.status = status;
            }
            WorkspaceOutcome::Written {
                status,
                directory,
                info,
            } => {
                self.status = status;
                if let Some(info) = info {
                    self.info = info;
                }
                effects.last_output_dir = Some(directory);
            }
            WorkspaceOutcome::Clipboard {
                bytes,
                format,
                status,
            } => {
                self.status = status;
                effects.clipboard = Some(ClipboardPayload { bytes, format });
            }
            WorkspaceOutcome::Failed { kind, detail } => {
                log::error!("workspace operation failed: {detail}");
                self.status = UiMessage::Error(kind);
            }
        }
        effects
    }

    fn first_images(&mut self) -> Option<Arc<Vec<RgbaImage>>> {
        self.first_images_with(UiMessage::NeedImage)
    }

    fn first_images_with(&mut self, missing: UiMessage) -> Option<Arc<Vec<RgbaImage>>> {
        if self.images.is_empty() {
            self.status = missing;
            return None;
        }
        Some(Arc::clone(&self.images))
    }

    fn all_images(&mut self) -> Option<Arc<Vec<RgbaImage>>> {
        if self.images.is_empty() {
            self.status = UiMessage::NeedMultipleImages;
            return None;
        }
        Some(Arc::clone(&self.images))
    }

    fn current_image_source(&self) -> Option<ImageSource> {
        self.result_image
            .as_ref()
            .map(|image| ImageSource::Result(Arc::clone(image)))
            .or_else(|| {
                (!self.images.is_empty()).then(|| ImageSource::Input {
                    images: Arc::clone(&self.images),
                    index: 0,
                })
            })
    }

    fn current_image_or_status(&mut self, missing: UiMessage) -> Option<ImageSource> {
        let image = self.current_image_source();
        if image.is_none() {
            self.status = missing;
        }
        image
    }

    fn source_image_or_status(&mut self, missing: UiMessage) -> Option<ImageSource> {
        let image = (!self.images.is_empty()).then(|| ImageSource::Input {
            images: Arc::clone(&self.images),
            index: 0,
        });
        if image.is_none() {
            self.status = missing;
        }
        image
    }

    fn start_job(&mut self, job: WorkspaceJob) -> Option<WorkspaceJob> {
        self.busy = true;
        self.status = UiMessage::Processing;
        Some(job)
    }
}

/// 可在线程池执行的工作区任务。
pub(crate) enum WorkspaceJob {
    LoadPaths {
        paths: Vec<PathBuf>,
        single: bool,
    },
    LoadClipboard {
        bytes: Vec<u8>,
    },
    Save {
        source: SaveSource,
        path: PathBuf,
    },
    Transform {
        image: ImageSource,
        operation: TransformOperation,
    },
    ReadExif {
        bytes: Arc<Vec<u8>>,
    },
    StripExif {
        bytes: Arc<Vec<u8>>,
    },
    GenerateQr {
        text: String,
        options: QrOptions,
    },
    DecodeQr {
        images: Arc<Vec<RgbaImage>>,
    },
    Collage {
        images: Arc<Vec<RgbaImage>>,
        layout: CollageLayout,
        options: CollageOptions,
    },
    Slice {
        images: Arc<Vec<RgbaImage>>,
        grid: SliceGrid,
        directory: PathBuf,
    },
    Gif {
        images: Arc<Vec<RgbaImage>>,
        params: GifParams,
    },
    Batch {
        images: Arc<Vec<RgbaImage>>,
        names: Arc<Vec<String>>,
        operation: BatchOperation,
        directory: PathBuf,
    },
    CopyImage {
        image: ImageSource,
    },
    CopyBytes {
        bytes: Arc<EncodedResult>,
    },
}

pub(crate) enum SaveSource {
    Image {
        image: ImageSource,
        settings: EncodeSettings,
    },
    Bytes {
        bytes: Arc<EncodedResult>,
    },
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum TransformOperation {
    Rotate(Rotation),
    Crop(CropRect),
    Resize { width: u32, height: u32 },
    Beautify(BeautifyParams),
}

pub(crate) enum BatchOperation {
    Ready(BatchOp),
    TextWatermark {
        text: String,
        opacity: f32,
        position: watermark::Position,
    },
    ImageWatermark {
        path: PathBuf,
        opacity: f32,
        position: watermark::Position,
    },
}

impl WorkspaceJob {
    /// 执行任务；批处理每完成一项便回报一次进度。
    pub fn run_with_progress(
        self,
        mut report_progress: impl FnMut(usize, usize),
    ) -> WorkspaceOutcome {
        match self {
            Self::LoadPaths { paths, single } => load_paths(paths, single),
            Self::LoadClipboard { bytes } => match decode_bytes(&bytes) {
                Ok(image) => WorkspaceOutcome::Loaded {
                    info: InfoMessage::ClipboardImage {
                        width: image.width(),
                        height: image.height(),
                    },
                    images: vec![image],
                    source_names: vec!["clipboard".to_string()],
                    source_bytes: Some(bytes),
                    status: UiMessage::ImagePasted,
                },
                Err((kind, detail)) => WorkspaceOutcome::Failed { kind, detail },
            },
            Self::Save { source, path } => {
                let bytes = match source {
                    SaveSource::Image { image, settings } => {
                        match format::encode(image.image(), settings) {
                            Ok(bytes) => bytes,
                            Err(error) => return failure(ErrorKind::Encode, error),
                        }
                    }
                    SaveSource::Bytes { bytes } => bytes.bytes.clone(),
                };
                match std::fs::write(&path, bytes) {
                    Ok(()) => WorkspaceOutcome::Written {
                        status: UiMessage::Saved,
                        directory: parent_or_path(&path),
                        info: None,
                    },
                    Err(error) => io_failure(&path, error),
                }
            }
            Self::Transform { image, operation } => {
                let result = match operation {
                    TransformOperation::Rotate(rotation) => {
                        Ok(transform::rotate(image.image(), rotation))
                    }
                    TransformOperation::Crop(rect) => transform::crop(image.image(), rect),
                    TransformOperation::Resize { width, height } => {
                        transform::resize(image.image(), width, height, ResizeFilter::default())
                    }
                    TransformOperation::Beautify(params) => {
                        beautify::beautify(image.image(), &params)
                    }
                };
                match result {
                    Ok(image) => WorkspaceOutcome::Image {
                        image,
                        status: UiMessage::SavedReady,
                    },
                    Err(error) => failure(ErrorKind::Operation, error),
                }
            }
            Self::ReadExif { bytes } => match exif::read(&bytes) {
                Ok(data) => WorkspaceOutcome::Info {
                    info: InfoMessage::Exif(data),
                    status: UiMessage::ExifRead,
                },
                Err(error) => failure(ErrorKind::Exif, error),
            },
            Self::StripExif { bytes } => match exif::strip(&bytes) {
                Ok(clean) => {
                    let preview = format::decode(&clean).ok();
                    WorkspaceOutcome::Bytes {
                        extension: detect_ext(&clean),
                        bytes: clean,
                        preview,
                        status: UiMessage::ExifStripped,
                    }
                }
                Err(error) => failure(ErrorKind::Exif, error),
            },
            Self::GenerateQr { text, options } => match qr::generate(&text, options) {
                Ok(image) => WorkspaceOutcome::Image {
                    image,
                    status: UiMessage::QrGenerated,
                },
                Err(error) => failure(ErrorKind::Qr, error),
            },
            Self::DecodeQr { images } => match qr::decode(&images[0]) {
                Ok(text) => WorkspaceOutcome::Info {
                    info: InfoMessage::QrContent(text),
                    status: UiMessage::QrDecoded,
                },
                Err(error) => failure(ErrorKind::Qr, error),
            },
            Self::Collage {
                images,
                layout,
                options,
            } => match collage::compose(&images, layout, options) {
                Ok(image) => WorkspaceOutcome::Image {
                    image,
                    status: UiMessage::CollageReady,
                },
                Err(error) => failure(ErrorKind::Operation, error),
            },
            Self::Slice {
                images,
                grid,
                directory,
            } => match slice::slice(&images[0], grid) {
                Ok(tiles) => {
                    let total = tiles.len();
                    let mut successes = 0;
                    for tile in tiles {
                        let bytes = match format::encode(
                            &tile.image,
                            EncodeSettings::Png {
                                compression: PngCompression::Default,
                            },
                        ) {
                            Ok(bytes) => bytes,
                            Err(error) => {
                                log::error!("slice tile encode failed: {error}");
                                continue;
                            }
                        };
                        let path = directory.join(naming::tile_filename(&tile));
                        match std::fs::write(&path, bytes) {
                            Ok(()) => successes += 1,
                            Err(error) => log::error!(
                                "slice tile write failed for {}: {error}",
                                path.display()
                            ),
                        }
                    }
                    WorkspaceOutcome::Written {
                        status: UiMessage::SlicesSaved { successes, total },
                        directory,
                        info: None,
                    }
                }
                Err(error) => failure(ErrorKind::Operation, error),
            },
            Self::Gif { images, params } => match animation::compose(&images, &params) {
                Ok(bytes) => WorkspaceOutcome::Bytes {
                    bytes,
                    extension: "gif",
                    preview: images.first().cloned(),
                    status: UiMessage::GifReady,
                },
                Err(error) => failure(ErrorKind::Operation, error),
            },
            Self::Batch {
                images,
                names,
                operation,
                directory,
            } => {
                let operation = match prepare_batch_operation(operation) {
                    Ok(operation) => operation,
                    Err((kind, detail)) => return WorkspaceOutcome::Failed { kind, detail },
                };
                let total = images.len();
                report_progress(0, total);
                let mut successes = 0;
                let mut failures = 0;
                let mut failed_items = Vec::new();
                for (index, image) in images.iter().enumerate() {
                    let mut result = batch::process(std::slice::from_ref(image), &operation);
                    let Some((_, output)) = result.successes.pop() else {
                        failures += 1;
                        let name = batch_item_name(&names, index);
                        if let Some(failure) = result.failures.pop() {
                            failed_items
                                .push(BatchItemFailure::new(name, core_error_kind(&failure.error)));
                            log::error!("batch item {} failed: {}", index + 1, failure.error);
                        } else {
                            failed_items.push(BatchItemFailure::new(name, ErrorKind::Operation));
                        }
                        report_progress(index + 1, total);
                        continue;
                    };
                    let source_stem = names
                        .get(index)
                        .map(String::as_str)
                        .filter(|name| !name.is_empty())
                        .unwrap_or("image");
                    let path =
                        directory.join(naming::batch_filename(source_stem, index, output.format));
                    match std::fs::write(&path, output.bytes) {
                        Ok(()) => successes += 1,
                        Err(error) => {
                            failures += 1;
                            failed_items.push(BatchItemFailure::new(
                                batch_item_name(&names, index),
                                classify_io(&error),
                            ));
                            log::error!(
                                "batch output write failed for {}: {error}",
                                path.display()
                            );
                        }
                    }
                    report_progress(index + 1, total);
                }
                WorkspaceOutcome::Written {
                    status: UiMessage::BatchComplete {
                        successes,
                        failures,
                        total,
                    },
                    directory,
                    info: (!failed_items.is_empty())
                        .then_some(InfoMessage::BatchFailures(failed_items)),
                }
            }
            Self::CopyImage { image } => match format::encode(
                image.image(),
                EncodeSettings::Png {
                    compression: PngCompression::Default,
                },
            ) {
                Ok(bytes) => WorkspaceOutcome::Clipboard {
                    bytes,
                    format: ClipboardFormat::Png,
                    status: UiMessage::ImageCopied,
                },
                Err(error) => failure(ErrorKind::Clipboard, error),
            },
            Self::CopyBytes { bytes } => WorkspaceOutcome::Clipboard {
                bytes: bytes.bytes.clone(),
                format: ClipboardFormat::from_extension(bytes.extension),
                status: UiMessage::ImageCopied,
            },
        }
    }
}

/// 后台任务的完成结果。
pub(crate) enum WorkspaceOutcome {
    Loaded {
        images: Vec<RgbaImage>,
        source_names: Vec<String>,
        source_bytes: Option<Vec<u8>>,
        info: InfoMessage,
        status: UiMessage,
    },
    Image {
        image: RgbaImage,
        status: UiMessage,
    },
    Bytes {
        bytes: Vec<u8>,
        extension: &'static str,
        preview: Option<RgbaImage>,
        status: UiMessage,
    },
    Info {
        info: InfoMessage,
        status: UiMessage,
    },
    Written {
        status: UiMessage,
        directory: PathBuf,
        info: Option<InfoMessage>,
    },
    Clipboard {
        bytes: Vec<u8>,
        format: ClipboardFormat,
        status: UiMessage,
    },
    Failed {
        kind: ErrorKind,
        detail: String,
    },
}

#[derive(Default)]
pub(crate) struct WorkspaceEffects {
    pub last_output_dir: Option<PathBuf>,
    pub clipboard: Option<ClipboardPayload>,
}

pub(crate) struct ClipboardPayload {
    pub bytes: Vec<u8>,
    pub format: ClipboardFormat,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum ClipboardFormat {
    Png,
    Jpeg,
    Webp,
    Gif,
    Bmp,
}

impl ClipboardFormat {
    fn from_extension(extension: &str) -> Self {
        match extension {
            "jpg" | "jpeg" => Self::Jpeg,
            "webp" => Self::Webp,
            "gif" => Self::Gif,
            "bmp" => Self::Bmp,
            _ => Self::Png,
        }
    }
}

fn prepare_batch_operation(operation: BatchOperation) -> Result<BatchOp, (ErrorKind, String)> {
    match operation {
        BatchOperation::Ready(operation) => Ok(operation),
        BatchOperation::TextWatermark {
            text,
            opacity,
            position,
        } => text_watermark::rasterize(&text)
            .map(|overlay| BatchOp::Watermark {
                overlay,
                opacity,
                position,
            })
            .map_err(|detail| (ErrorKind::Operation, detail)),
        BatchOperation::ImageWatermark {
            path,
            opacity,
            position,
        } => {
            let bytes = std::fs::read(&path)
                .map_err(|error| (classify_io(&error), format!("{}: {error}", path.display())))?;
            let overlay = decode_bytes(&bytes)?;
            Ok(BatchOp::Watermark {
                overlay,
                opacity,
                position,
            })
        }
    }
}

fn batch_item_name(names: &[String], index: usize) -> String {
    names
        .get(index)
        .filter(|name| !name.is_empty())
        .cloned()
        .unwrap_or_else(|| format!("#{}", index + 1))
}

fn load_paths(paths: Vec<PathBuf>, single: bool) -> WorkspaceOutcome {
    let mut images = Vec::new();
    let mut source_names = Vec::new();
    let mut first_bytes = None;
    let mut failures = 0;
    let mut first_path = None;
    let mut last_failure = None;

    for path in &paths {
        match std::fs::read(path) {
            Ok(bytes) => match decode_bytes(&bytes) {
                Ok(image) => {
                    if images.is_empty() {
                        first_path = Some(path.clone());
                        first_bytes = Some(bytes);
                    }
                    source_names.push(
                        path.file_stem()
                            .map(|name| name.to_string_lossy().into_owned())
                            .unwrap_or_else(|| "image".to_string()),
                    );
                    images.push(image);
                    if single {
                        break;
                    }
                }
                Err(error) => {
                    failures += 1;
                    last_failure = Some(error);
                }
            },
            Err(error) => {
                failures += 1;
                last_failure = Some((classify_io(&error), error.to_string()));
            }
        }
    }

    let Some(first) = images.first() else {
        let (kind, detail) = last_failure.unwrap_or((
            ErrorKind::CorruptedImage,
            "no image could be loaded".to_string(),
        ));
        return WorkspaceOutcome::Failed { kind, detail };
    };

    let info = if single {
        InfoMessage::Image {
            name: first_path
                .as_deref()
                .and_then(Path::file_name)
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default(),
            width: first.width(),
            height: first.height(),
        }
    } else {
        InfoMessage::None
    };
    let successes = images.len();
    WorkspaceOutcome::Loaded {
        images,
        source_names,
        source_bytes: if single { first_bytes } else { None },
        info,
        status: if single {
            UiMessage::LoadedImage
        } else {
            UiMessage::LoadedMultiple {
                successes,
                failures,
            }
        },
    }
}

fn decode_bytes(bytes: &[u8]) -> Result<RgbaImage, (ErrorKind, String)> {
    if !format::is_supported_input(bytes) {
        return Err((
            ErrorKind::UnsupportedFormat,
            "input magic does not match a supported format".to_string(),
        ));
    }
    format::decode(bytes).map_err(|error| (ErrorKind::CorruptedImage, error.to_string()))
}

fn failure(kind: ErrorKind, error: impl std::fmt::Display) -> WorkspaceOutcome {
    WorkspaceOutcome::Failed {
        kind,
        detail: error.to_string(),
    }
}

fn io_failure(path: &Path, error: std::io::Error) -> WorkspaceOutcome {
    WorkspaceOutcome::Failed {
        kind: classify_io(&error),
        detail: format!("{}: {error}", path.display()),
    }
}

fn classify_io(error: &std::io::Error) -> ErrorKind {
    if error.kind() == std::io::ErrorKind::PermissionDenied {
        ErrorKind::Permission
    } else if error.kind() == std::io::ErrorKind::StorageFull
        || matches!(error.raw_os_error(), Some(28 | 112))
    {
        ErrorKind::DiskFull
    } else {
        ErrorKind::Io
    }
}

fn core_error_kind(error: &CoreError) -> ErrorKind {
    match error {
        CoreError::ImageEncode { .. } | CoreError::Webp { .. } => ErrorKind::Encode,
        CoreError::InsufficientDiskSpace { .. } => ErrorKind::DiskFull,
        CoreError::Io { source, .. } => classify_io(source),
        _ => ErrorKind::Operation,
    }
}

fn parent_or_path(path: &Path) -> PathBuf {
    path.parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| path.to_path_buf())
}

fn detect_ext(bytes: &[u8]) -> &'static str {
    match image::guess_format(bytes) {
        Ok(image::ImageFormat::Png) => "png",
        Ok(image::ImageFormat::WebP) => "webp",
        _ => "jpg",
    }
}

/// 把 `rastery-core` 的 RGBA 图转成 GPUI 的 [`RenderImage`]（BGRA 序）。
pub(crate) fn to_render_image(img: &RgbaImage) -> Arc<RenderImage> {
    let mut bgra = img.clone();
    for pixel in bgra.pixels_mut() {
        pixel.0.swap(0, 2);
    }
    Arc::new(RenderImage::new(vec![Frame::new(bgra)]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    #[test]
    fn filename_sanitization_keeps_outputs_inside_the_selected_directory() {
        assert_eq!(naming::sanitize_stem("hero:cover?.png"), "hero_cover_.png");
        assert_eq!(naming::sanitize_stem("..."), "image");
        assert_eq!(naming::sanitize_stem("封面"), "封面");
    }

    #[test]
    fn supported_image_path_filter_is_case_insensitive() {
        assert!(is_supported_image_path(Path::new("sample.PNG")));
        assert!(is_supported_image_path(Path::new("sample.WebP")));
        assert!(!is_supported_image_path(Path::new("notes.txt")));
        assert!(!is_supported_image_path(Path::new("no-extension")));
    }

    #[test]
    fn async_picker_paths_keep_all_supported_collage_inputs() {
        let mut workspace = Workspace::default();
        let job = workspace
            .prepare_paths(
                vec![
                    PathBuf::from("first.PNG"),
                    PathBuf::from("notes.txt"),
                    PathBuf::from("second.webp"),
                ],
                true,
            )
            .expect("supported paths should create a load job");

        match job {
            WorkspaceJob::LoadPaths { paths, single } => {
                assert!(!single);
                assert_eq!(
                    paths,
                    vec![PathBuf::from("first.PNG"), PathBuf::from("second.webp")]
                );
            }
            _ => panic!("image picker paths should create a load-paths job"),
        }
    }

    #[test]
    fn async_picker_with_no_supported_images_keeps_workspace_reusable() {
        let mut workspace = Workspace::default();

        assert!(
            workspace
                .prepare_paths(vec![PathBuf::from("notes.txt")], true)
                .is_none()
        );
        assert_eq!(workspace.status, UiMessage::NeedImage);
        assert!(!workspace.is_busy());
        assert!(
            workspace
                .prepare_paths(vec![PathBuf::from("photo.png")], false)
                .is_some()
        );
    }

    #[test]
    fn cancelled_and_failed_prompts_leave_workspace_ready_for_the_next_command() {
        for status in [UiMessage::CancelledOpen, UiMessage::Error(ErrorKind::Io)] {
            let mut workspace = Workspace::default();
            workspace.set_status(status);

            assert!(!workspace.is_busy());
            assert!(
                workspace
                    .prepare_paths(vec![PathBuf::from("photo.png")], false)
                    .is_some()
            );
        }
    }

    #[test]
    fn concrete_save_path_prepares_save_job_without_opening_a_dialog() {
        let mut workspace = workspace_with_images(1);
        let path = PathBuf::from("result.png");

        let job = workspace
            .prepare_save_path(
                path.clone(),
                EncodeSettings::Png {
                    compression: PngCompression::Default,
                },
            )
            .expect("a selected path should prepare the save job");

        assert!(matches!(job, WorkspaceJob::Save { path: job_path, .. } if job_path == path));
        assert!(workspace.is_busy());
    }

    #[test]
    fn concrete_directory_prepares_slice_job_without_opening_a_dialog() {
        let mut workspace = workspace_with_images(1);
        let directory = PathBuf::from("tiles");
        let grid = SliceGrid::new(2, 3).expect("valid slice grid");

        let job = workspace
            .prepare_slice_directory(grid, directory.clone())
            .expect("a selected directory should prepare the slice job");

        assert!(matches!(
            job,
            WorkspaceJob::Slice {
                grid: job_grid,
                directory: job_directory,
                ..
            } if job_grid == grid && job_directory == directory
        ));
    }

    #[test]
    fn concrete_paths_prepare_image_watermark_batch_without_dialogs() {
        let mut workspace = workspace_with_images(2);
        let directory = PathBuf::from("batch-output");
        let watermark = PathBuf::from("watermark.png");
        let request = BatchRequest::Watermark {
            source: WatermarkSource::Image,
            text: String::new(),
            opacity: 0.5,
            placement: WatermarkPlacement::BottomRight,
        };

        let job = workspace
            .prepare_batch_paths(request, directory.clone(), Some(watermark.clone()))
            .expect("selected output and watermark paths should prepare the batch job");

        assert!(matches!(
            job,
            WorkspaceJob::Batch {
                operation: BatchOperation::ImageWatermark { path, .. },
                directory: job_directory,
                ..
            } if path == watermark && job_directory == directory
        ));
    }

    #[test]
    fn concrete_path_preparation_respects_busy_state() {
        let mut workspace = workspace_with_images(1);
        workspace.busy = true;

        assert!(
            workspace
                .prepare_save_path(
                    PathBuf::from("result.png"),
                    EncodeSettings::Png {
                        compression: PngCompression::Default,
                    },
                )
                .is_none()
        );
        assert_eq!(workspace.status, UiMessage::Busy);
    }

    fn workspace_with_images(count: usize) -> Workspace {
        Workspace {
            images: Arc::new(
                (0..count)
                    .map(|_| RgbaImage::from_pixel(2, 2, Rgba([255, 0, 0, 255])))
                    .collect(),
            ),
            source_names: Arc::new((0..count).map(|index| format!("image-{index}")).collect()),
            ..Workspace::default()
        }
    }

    #[test]
    fn batch_job_uses_source_stems_and_sequential_numbers() {
        let directory = tempfile::tempdir().expect("temporary output directory");
        let images = Arc::new(vec![
            RgbaImage::from_pixel(2, 2, Rgba([255, 0, 0, 255])),
            RgbaImage::from_pixel(2, 2, Rgba([0, 255, 0, 255])),
        ]);
        let names = Arc::new(vec!["hero:cover".to_string(), "second".to_string()]);
        let job = WorkspaceJob::Batch {
            images,
            names,
            operation: BatchOperation::Ready(BatchOp::Convert {
                format: OutputFormat::Png,
                settings: EncodeSettings::Png {
                    compression: PngCompression::Fast,
                },
            }),
            directory: directory.path().to_path_buf(),
        };
        let mut progress = Vec::new();
        let outcome = job.run_with_progress(|completed, total| {
            progress.push((completed, total));
        });

        match outcome {
            WorkspaceOutcome::Written { status, .. } => assert_eq!(
                status,
                UiMessage::BatchComplete {
                    successes: 2,
                    failures: 0,
                    total: 2,
                }
            ),
            _ => panic!("batch job should write its outputs"),
        }
        assert!(directory.path().join("hero_cover-rastery-1.png").is_file());
        assert!(directory.path().join("second-rastery-2.png").is_file());
        assert_eq!(progress, vec![(0, 2), (1, 2), (2, 2)]);
    }

    // Feature: rastery, Property 27: errors leave the application workspace reusable
    #[test]
    fn error_outcome_keeps_workspace_reusable() {
        let mut workspace = Workspace::default();
        let first = workspace.prepare_paste(vec![1, 2, 3]);
        assert!(first.is_some());
        assert!(workspace.is_busy());

        workspace.apply(WorkspaceOutcome::Failed {
            kind: ErrorKind::CorruptedImage,
            detail: "test decode failure".to_string(),
        });

        assert!(!workspace.is_busy());
        assert_eq!(
            workspace.status,
            UiMessage::Error(ErrorKind::CorruptedImage)
        );
        assert!(workspace.prepare_paste(vec![4, 5, 6]).is_some());
    }

    #[test]
    fn batch_failures_keep_each_item_reason() {
        let directory = tempfile::tempdir().expect("temporary output directory");
        let job = WorkspaceJob::Batch {
            images: Arc::new(vec![RgbaImage::from_pixel(2, 2, Rgba([255, 0, 0, 255]))]),
            names: Arc::new(vec!["broken-item".to_string()]),
            operation: BatchOperation::Ready(BatchOp::Resize {
                width: 0,
                height: 0,
                filter: ResizeFilter::Nearest,
            }),
            directory: directory.path().to_path_buf(),
        };

        let outcome = job.run_with_progress(|_, _| {});
        match outcome {
            WorkspaceOutcome::Written {
                info: Some(InfoMessage::BatchFailures(items)),
                ..
            } => {
                assert_eq!(items.len(), 1);
                assert_eq!(items[0].name, "broken-item");
                assert_eq!(items[0].kind, ErrorKind::Operation);
            }
            _ => panic!("batch failure should preserve the item reason"),
        }
    }
}
