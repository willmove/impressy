//! 图像工作区与后台任务。
//!
//! 文件对话框在 UI 线程上只负责选择路径；文件读取、编解码和所有图像变换封装为
//! [`WorkspaceJob`]，由 `AppShell` 投递到 GPUI background executor。完成后只在主线程
//! 应用状态和刷新预览（Requirement 3、37）。

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use gpui::{RenderImage, SharedString};
use image::{Frame, RgbaImage};

use impressy_core::animation::{self, GifParams};
use impressy_core::batch::{self, BatchPipeline, BatchResizeMode, BatchStep};
use impressy_core::beautify::{self, BeautifyParams};
use impressy_core::collage::{self, CollageLayout, CollageOptions};
use impressy_core::format::{self, EncodeSettings, PngCompression};
use impressy_core::naming;
use impressy_core::qr::QrOptions;
use impressy_core::slice::{self, SliceGrid};
use impressy_core::transform::{self, CropRect, ResizeFilter, Rotation};
use impressy_core::watermark;
use impressy_core::{CoreError, exif, qr};
use rust_i18n::t;

use crate::document::{
    CurrentDocument, DocumentSnapshot, DocumentVersion, EditKind, InputSet, InputSetVersion,
};
use crate::export_plan::{self, CollisionPolicy, PublishOutcome};
use crate::feature_params::{WatermarkPlacement, WatermarkSource};
use crate::text_watermark;
use crate::ui_message::{BatchItemOutcome, BatchItemResult, ErrorKind, InfoMessage, UiMessage};

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
    AppendMultiple,
    SaveResult,
    Transform(TransformOperation),
    PreviewTransform(TransformOperation),
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
    Slice {
        grid: SliceGrid,
        collision_policy: CollisionPolicy,
    },
    MakeGif(GifParams),
    Batch(BatchRequest),
}

pub(crate) enum WorkspaceCommandRoute {
    OpenImages { multiple: bool, append: bool },
    SaveResult,
    OutputDirectory(OutputDirectoryCommand),
    Direct(WorkspaceOperation),
}

pub(crate) enum OutputDirectoryCommand {
    Slice {
        grid: SliceGrid,
        collision_policy: CollisionPolicy,
    },
    Batch(BatchRequest),
}

pub(crate) enum WorkspaceOperation {
    Transform(TransformOperation),
    PreviewTransform(TransformOperation),
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
            Self::OpenSingle => WorkspaceCommandRoute::OpenImages {
                multiple: false,
                append: false,
            },
            Self::OpenMultiple => WorkspaceCommandRoute::OpenImages {
                multiple: true,
                append: false,
            },
            Self::AppendMultiple => WorkspaceCommandRoute::OpenImages {
                multiple: true,
                append: true,
            },
            Self::SaveResult => WorkspaceCommandRoute::SaveResult,
            Self::Slice {
                grid,
                collision_policy,
            } => WorkspaceCommandRoute::OutputDirectory(OutputDirectoryCommand::Slice {
                grid,
                collision_policy,
            }),
            Self::Batch(request) => {
                WorkspaceCommandRoute::OutputDirectory(OutputDirectoryCommand::Batch(request))
            }
            Self::Transform(operation) => {
                WorkspaceCommandRoute::Direct(WorkspaceOperation::Transform(operation))
            }
            Self::PreviewTransform(operation) => {
                WorkspaceCommandRoute::Direct(WorkspaceOperation::PreviewTransform(operation))
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
    Pipeline {
        steps: Vec<BatchRequestStep>,
        output: EncodeSettings,
        collision_policy: CollisionPolicy,
    },
}

#[derive(Debug, Clone)]
pub(crate) enum BatchRequestStep {
    Resize {
        mode: crate::feature_params::ResizeMode,
        width: u32,
        height: u32,
        aspect_locked: bool,
        percentage: u32,
        longest_side: u32,
        prevent_enlarge: bool,
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
    document: Option<CurrentDocument>,
    document_name: Option<String>,
    input_set: InputSet,
    source_bytes: Option<Arc<Vec<u8>>>,
    result_bytes: Option<Arc<EncodedResult>>,
    preview: Option<Arc<RenderImage>>,
    preview_dimensions: Option<(u32, u32)>,
    thumbs: Vec<Arc<RenderImage>>,
    focus_index: usize,
    status: UiMessage,
    info: InfoMessage,
    busy: bool,
    preview_generation: u64,
    batch_cancel: Option<Arc<AtomicBool>>,
    retry_batch: Option<RetryBatch>,
}

pub(crate) struct RetryBatch {
    images: Arc<Vec<RgbaImage>>,
    names: Arc<Vec<String>>,
    operations: Vec<BatchStepOperation>,
    output: EncodeSettings,
    directory: PathBuf,
    collision_policy: CollisionPolicy,
}

#[derive(Clone)]
pub(crate) enum ImageSource {
    Document(DocumentSnapshot),
}

impl ImageSource {
    pub fn image(&self) -> &RgbaImage {
        match self {
            Self::Document(snapshot) => &snapshot.image,
        }
    }
}

#[derive(Debug)]
pub(crate) struct EncodedResult {
    bytes: Vec<u8>,
    extension: &'static str,
    kind: EncodedResultKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EncodedResultKind {
    Gif,
    Exif,
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
        self.document
            .as_ref()
            .map(CurrentDocument::snapshot)
            .map(|snapshot| snapshot.image.dimensions())
    }

    pub fn has_image(&self) -> bool {
        self.current_dimensions().is_some()
    }

    pub fn has_encoded_result(&self, kind: EncodedResultKind) -> bool {
        self.result_bytes
            .as_ref()
            .is_some_and(|result| result.kind == kind)
    }

    pub fn encoded_result_byte_len(&self, kind: EncodedResultKind) -> Option<usize> {
        self.result_bytes
            .as_ref()
            .filter(|result| result.kind == kind)
            .map(|result| result.bytes.len())
    }

    pub fn clear_encoded_result(&mut self) {
        self.result_bytes = None;
    }

    pub fn image_count(&self) -> usize {
        self.input_set.len()
    }

    pub fn focus_index(&self) -> usize {
        self.focus_index
    }

    pub fn source_name(&self, index: usize) -> Option<&str> {
        self.input_set.name(index)
    }

    pub fn input_dimensions(&self, index: usize) -> Option<(u32, u32)> {
        self.input_set.image(index).map(RgbaImage::dimensions)
    }

    pub fn thumb(&self, index: usize) -> Option<Arc<RenderImage>> {
        self.thumbs.get(index).cloned()
    }

    pub fn set_focus(&mut self, index: usize) {
        if index >= self.input_set.len() {
            return;
        }
        self.focus_index = index;
        self.preview_source_at(index);
    }

    /// 删除一张输入图。来源变更后丢掉当前结果，避免拼图/GIF 仍显示过期合成。
    pub fn remove_image(&mut self, index: usize) -> bool {
        if self.busy || index >= self.input_set.len() {
            return false;
        }
        let old_focus = self.focus_index;
        self.input_set.remove(index);
        if index < self.thumbs.len() {
            self.thumbs.remove(index);
        }
        self.result_bytes = None;
        self.retry_batch = None;
        if self.input_set.is_empty() {
            self.focus_index = 0;
            self.preview = None;
            self.preview_dimensions = None;
            self.info = InfoMessage::None;
            self.status = UiMessage::NeedMultipleImages;
            return true;
        }
        self.focus_index = if index < old_focus {
            old_focus - 1
        } else {
            old_focus.min(self.input_set.len() - 1)
        };
        self.preview_source_at(self.focus_index);
        self.status = UiMessage::LoadedMultiple {
            successes: self.input_set.len(),
            failures: 0,
        };
        true
    }

    /// 把 `from` 移到 `to`（用于图条左右排序）。
    pub fn move_image(&mut self, from: usize, to: usize) -> bool {
        if self.busy || from == to || from >= self.input_set.len() || to >= self.input_set.len() {
            return false;
        }
        self.input_set.move_item(from, to);
        if from < self.thumbs.len() && to <= self.thumbs.len() {
            let thumb = self.thumbs.remove(from);
            self.thumbs.insert(to.min(self.thumbs.len()), thumb);
        }
        self.result_bytes = None;
        self.retry_batch = None;
        self.focus_index = to;
        self.preview_source_at(to);
        true
    }

    fn preview_source_at(&mut self, index: usize) {
        if let Some(image) = self.input_set.image(index) {
            self.preview = Some(to_render_image(image));
            self.preview_dimensions = Some(image.dimensions());
            if let Some(name) = self.input_set.name(index) {
                self.info = InfoMessage::Image {
                    name: name.to_string(),
                    width: image.width(),
                    height: image.height(),
                };
            }
        }
    }

    pub fn preview_dimensions(&self) -> Option<(u32, u32)> {
        self.preview_dimensions
    }

    pub fn show_document_preview(&mut self) {
        if let Some(document) = &self.document {
            let image = document.displayed_image();
            self.preview = Some(to_render_image(&image));
            self.preview_dimensions = Some(image.dimensions());
        }
    }

    pub fn show_input_set_preview(&mut self) {
        if !self.input_set.is_empty() {
            self.preview_source_at(self.focus_index.min(self.input_set.len() - 1));
        }
    }

    pub fn document_name(&self) -> Option<&str> {
        self.document_name.as_deref()
    }

    pub fn has_transient_preview(&self) -> bool {
        self.document
            .as_ref()
            .is_some_and(CurrentDocument::has_transient)
    }

    pub fn transient_edit_kind(&self) -> Option<EditKind> {
        self.document
            .as_ref()
            .and_then(CurrentDocument::transient_kind)
    }

    pub fn apply_transient_preview(&mut self) -> bool {
        if self.busy {
            return false;
        }
        let applied = self
            .document
            .as_mut()
            .is_some_and(CurrentDocument::apply_transient);
        if applied {
            self.invalidate_preview_requests();
            self.result_bytes = None;
            self.show_document_preview();
            self.status = UiMessage::SavedReady;
        }
        applied
    }

    pub fn apply_transient_preview_for(&mut self, kind: EditKind) -> bool {
        if self.busy {
            return false;
        }
        let applied = self
            .document
            .as_mut()
            .is_some_and(|document| document.apply_transient_kind(kind));
        if applied {
            self.invalidate_preview_requests();
            self.result_bytes = None;
            self.show_document_preview();
            self.status = UiMessage::SavedReady;
        }
        applied
    }

    pub fn discard_transient_preview(&mut self) -> bool {
        if self.busy {
            return false;
        }
        let discarded = self
            .document
            .as_mut()
            .is_some_and(CurrentDocument::discard_transient);
        if discarded {
            self.invalidate_preview_requests();
            self.show_document_preview();
        }
        discarded
    }

    pub fn invalidate_preview_requests(&mut self) {
        self.preview_generation = self.preview_generation.wrapping_add(1).max(1);
    }

    pub fn can_undo(&self) -> bool {
        self.document
            .as_ref()
            .is_some_and(CurrentDocument::can_undo)
    }

    pub fn can_redo(&self) -> bool {
        self.document
            .as_ref()
            .is_some_and(CurrentDocument::can_redo)
    }

    pub fn undo(&mut self) -> bool {
        if self.busy {
            return false;
        }
        let changed = self.document.as_mut().is_some_and(CurrentDocument::undo);
        if changed {
            self.result_bytes = None;
            self.show_document_preview();
            self.status = UiMessage::UndoApplied {
                remaining: self
                    .document
                    .as_ref()
                    .map_or(0, CurrentDocument::undo_steps),
            };
        }
        changed
    }

    pub fn redo(&mut self) -> bool {
        if self.busy {
            return false;
        }
        let changed = self.document.as_mut().is_some_and(CurrentDocument::redo);
        if changed {
            self.result_bytes = None;
            self.show_document_preview();
            self.status = UiMessage::RedoApplied {
                remaining: self
                    .document
                    .as_ref()
                    .map_or(0, CurrentDocument::redo_steps),
            };
        }
        changed
    }

    pub fn restore_original(&mut self) -> bool {
        if self.busy {
            return false;
        }
        let Some(document) = self.document.as_mut() else {
            return false;
        };
        let version = document.snapshot().version;
        let original = document.original();
        let changed = document.commit(version, (*original).clone(), EditKind::RestoreOriginal);
        if changed {
            self.result_bytes = None;
            self.show_document_preview();
            self.status = UiMessage::OriginalRestored;
        }
        changed
    }

    pub fn is_dirty(&self) -> bool {
        self.document
            .as_ref()
            .is_some_and(CurrentDocument::is_dirty)
    }

    pub fn original_preview(&self) -> Option<Arc<RenderImage>> {
        self.document
            .as_ref()
            .map(CurrentDocument::original)
            .map(|image| to_render_image(&image))
    }

    pub fn original_dimensions(&self) -> Option<(u32, u32)> {
        self.document
            .as_ref()
            .map(CurrentDocument::original)
            .map(|image| image.dimensions())
    }

    pub fn source_byte_len(&self) -> Option<usize> {
        self.source_bytes.as_ref().map(|bytes| bytes.len())
    }

    pub fn image_for_estimate(&self) -> Option<ImageSource> {
        self.current_image_source()
    }

    /// Immutable decoded inputs for v2 reference-image requests. Encoding is performed on the
    /// background executor so large images never block the GPUI thread.
    pub fn ai_reference_snapshot(&self) -> Option<DocumentSnapshot> {
        self.document.as_ref().map(CurrentDocument::snapshot)
    }

    pub fn document_version(&self) -> Option<DocumentVersion> {
        self.document
            .as_ref()
            .map(CurrentDocument::snapshot)
            .map(|snapshot| snapshot.version)
    }

    pub fn use_ai_result(&mut self, image: RgbaImage) -> bool {
        if self.busy {
            self.status = UiMessage::Busy;
            return false;
        }
        self.document = Some(CurrentDocument::new_unexported(image));
        self.document_name = Some(t!("workspace.ai_result_name").to_string());
        self.source_bytes = None;
        self.result_bytes = None;
        self.show_document_preview();
        self.status = UiMessage::AiResultOpened;
        true
    }

    pub fn use_generated_result(&mut self, image: RgbaImage, status: UiMessage) -> bool {
        if self.busy {
            self.status = UiMessage::Busy;
            return false;
        }
        self.document = Some(CurrentDocument::new_unexported(image));
        self.document_name = None;
        self.source_bytes = None;
        self.result_bytes = None;
        self.show_document_preview();
        self.status = status;
        true
    }

    pub fn status_text(&self) -> SharedString {
        self.status.text()
    }

    pub fn info_text(&self) -> SharedString {
        self.info.text()
    }

    pub fn batch_results_text(&self) -> Option<SharedString> {
        matches!(self.info, InfoMessage::BatchResults(_)).then(|| self.info.text())
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
                let document = self.document_snapshot_or_status(UiMessage::NeedImage)?;
                WorkspaceJob::Transform {
                    document,
                    operation,
                    preview: false,
                    preview_generation: None,
                }
            }
            WorkspaceOperation::PreviewTransform(operation) => {
                let discarded = self
                    .document
                    .as_mut()
                    .is_some_and(CurrentDocument::discard_transient);
                if discarded {
                    self.show_document_preview();
                }
                let document = self.document_snapshot_or_status(UiMessage::NeedImage)?;
                self.preview_generation = self.preview_generation.wrapping_add(1).max(1);
                self.status = UiMessage::Processing;
                return Some(WorkspaceJob::Transform {
                    document,
                    operation,
                    preview: true,
                    preview_generation: Some(self.preview_generation),
                });
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
                let document = self.document_snapshot_or_status(UiMessage::NeedExifImage)?;
                WorkspaceJob::StripExif { document }
            }
            WorkspaceOperation::GenerateQr { text, options } => {
                WorkspaceJob::GenerateQr { text, options }
            }
            WorkspaceOperation::DecodeQr => WorkspaceJob::DecodeQr {
                images: self.first_images_with(UiMessage::NeedQrImage)?,
            },
            WorkspaceOperation::Collage { layout, options } => WorkspaceJob::Collage {
                images: self.all_images()?,
                input_version: self.input_set.version(),
                layout,
                options,
            },
            WorkspaceOperation::MakeGif(params) => WorkspaceJob::Gif {
                images: self.all_images()?,
                input_version: self.input_set.version(),
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
        self.prepare_paths_mode(paths, multiple, false)
    }

    pub fn prepare_append_paths(&mut self, paths: Vec<PathBuf>) -> Option<WorkspaceJob> {
        self.prepare_paths_mode(paths, true, true)
    }

    fn prepare_paths_mode(
        &mut self,
        paths: Vec<PathBuf>,
        multiple: bool,
        append: bool,
    ) -> Option<WorkspaceJob> {
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
            append,
        })
    }

    /// 用异步保存 prompt 返回的具体路径准备保存任务。
    pub fn prepare_save_path(
        &mut self,
        path: PathBuf,
        default_export: EncodeSettings,
        encoded_kind: Option<EncodedResultKind>,
    ) -> Option<WorkspaceJob> {
        self.prepare_export_path(
            path,
            default_export,
            CollisionPolicy::PreserveBoth,
            encoded_kind,
        )
    }

    pub fn prepare_export_path(
        &mut self,
        path: PathBuf,
        export: EncodeSettings,
        collision_policy: CollisionPolicy,
        encoded_kind: Option<EncodedResultKind>,
    ) -> Option<WorkspaceJob> {
        if self.busy {
            self.status = UiMessage::Busy;
            return None;
        }
        let source = if let Some(result) = self
            .result_bytes
            .as_ref()
            .filter(|result| Some(result.kind) == encoded_kind)
        {
            SaveSource::Bytes {
                bytes: Arc::clone(result),
            }
        } else if let Some(image) = self.current_image_source() {
            SaveSource::Image {
                image,
                settings: export,
            }
        } else {
            self.status = UiMessage::NeedResult;
            return None;
        };
        self.start_job(WorkspaceJob::Save {
            source,
            path,
            collision_policy,
        })
    }

    /// Prepare an export of the applied document, even when the active feature also owns an
    /// encoded result (for example, a GIF assembled from the current input set). Dirty-document
    /// continuations use this path so exporting before replace or quit always saves the document
    /// that triggered the guard.
    pub fn prepare_document_export_path(
        &mut self,
        path: PathBuf,
        export: EncodeSettings,
        collision_policy: CollisionPolicy,
    ) -> Option<WorkspaceJob> {
        if self.busy {
            self.status = UiMessage::Busy;
            return None;
        }
        let Some(image) = self.current_image_source() else {
            self.status = UiMessage::NeedResult;
            return None;
        };
        self.start_job(WorkspaceJob::Save {
            source: SaveSource::Image {
                image,
                settings: export,
            },
            path,
            collision_policy,
        })
    }

    /// 用异步目录 prompt 返回的具体路径准备切图任务。
    pub fn prepare_slice_directory(
        &mut self,
        grid: SliceGrid,
        directory: PathBuf,
        collision_policy: CollisionPolicy,
    ) -> Option<WorkspaceJob> {
        if self.busy {
            self.status = UiMessage::Busy;
            return None;
        }
        let document = self.document_snapshot_or_status(UiMessage::NeedImage)?;
        let document_version = document.version;
        let images = Arc::new(vec![(*document.image).clone()]);
        self.start_job(WorkspaceJob::Slice {
            images,
            document_version,
            grid,
            directory,
            collision_policy,
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
        let names = self.input_set.names();
        let BatchRequest::Pipeline {
            steps,
            output,
            collision_policy,
        } = request;
        let mut operations = Vec::with_capacity(steps.len());
        for step in steps {
            let operation = match step {
                BatchRequestStep::Resize {
                    mode,
                    width,
                    height,
                    aspect_locked,
                    percentage,
                    longest_side,
                    prevent_enlarge,
                } => BatchStepOperation::Ready(BatchStep::Resize {
                    mode: match mode {
                        crate::feature_params::ResizeMode::Pixels => BatchResizeMode::Pixels {
                            width,
                            height,
                            preserve_aspect: aspect_locked,
                        },
                        crate::feature_params::ResizeMode::Percentage => {
                            BatchResizeMode::Percentage(percentage)
                        }
                        crate::feature_params::ResizeMode::LongestSide => {
                            BatchResizeMode::LongestSide(longest_side)
                        }
                    },
                    filter: ResizeFilter::default(),
                    prevent_enlarge,
                }),
                BatchRequestStep::Watermark {
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
                        WatermarkSource::Text => BatchStepOperation::TextWatermark {
                            text,
                            opacity,
                            position,
                        },
                        WatermarkSource::Image => {
                            let Some(path) = watermark_path
                                .as_ref()
                                .filter(|path| is_supported_image_path(path))
                                .cloned()
                            else {
                                self.status = UiMessage::NeedImage;
                                return None;
                            };
                            BatchStepOperation::ImageWatermark {
                                path,
                                opacity,
                                position,
                            }
                        }
                    }
                }
            };
            operations.push(operation);
        }
        let cancel = Arc::new(AtomicBool::new(false));
        self.batch_cancel = Some(Arc::clone(&cancel));
        self.retry_batch = None;
        self.start_job(WorkspaceJob::Batch {
            images,
            names,
            input_version: self.input_set.version(),
            operations,
            output,
            directory,
            collision_policy,
            cancel,
            preview_index: self.focus_index,
        })
    }

    pub fn cancel_batch(&mut self) -> bool {
        let Some(cancel) = &self.batch_cancel else {
            return false;
        };
        cancel.store(true, Ordering::Relaxed);
        true
    }

    pub fn has_retry_batch(&self) -> bool {
        self.retry_batch.is_some() && !self.busy
    }

    pub fn prepare_retry_batch(&mut self) -> Option<WorkspaceJob> {
        if self.busy {
            self.status = UiMessage::Busy;
            return None;
        }
        let retry = self.retry_batch.take()?;
        let cancel = Arc::new(AtomicBool::new(false));
        self.batch_cancel = Some(Arc::clone(&cancel));
        self.start_job(WorkspaceJob::Batch {
            images: retry.images,
            names: retry.names,
            input_version: self.input_set.version(),
            operations: retry.operations,
            output: retry.output,
            directory: retry.directory,
            collision_policy: retry.collision_policy,
            cancel,
            preview_index: 0,
        })
    }

    /// 当前结果建议使用的保存文件名。
    pub fn suggested_save_name(
        &self,
        default_export: EncodeSettings,
        encoded_kind: Option<EncodedResultKind>,
    ) -> String {
        let extension = self.export_extension(default_export, encoded_kind);
        t!("dialog.output_filename", extension = extension).to_string()
    }

    pub fn export_extension(
        &self,
        default_export: EncodeSettings,
        encoded_kind: Option<EncodedResultKind>,
    ) -> &'static str {
        if let Some(kind) = encoded_kind {
            self.result_bytes
                .as_deref()
                .filter(|result| result.kind == kind)
                .map(|result| result.extension)
                .unwrap_or_else(|| default_export.format().extension())
        } else {
            default_export.format().extension()
        }
    }

    /// 准备把当前结果编码后复制到系统剪贴板。
    pub fn prepare_copy(
        &mut self,
        encoded_kind: Option<EncodedResultKind>,
    ) -> Option<WorkspaceJob> {
        if self.busy {
            self.status = UiMessage::Busy;
            return None;
        }
        let job = if let Some(result) = self
            .result_bytes
            .as_ref()
            .filter(|result| Some(result.kind) == encoded_kind)
        {
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
        let normal_job_running = self.busy;
        let outcome = match outcome {
            WorkspaceOutcome::InputVersioned { version, outcome } => {
                self.busy = false;
                if version != self.input_set.version() {
                    return WorkspaceEffects::default();
                }
                *outcome
            }
            WorkspaceOutcome::DocumentVersioned { version, outcome } => {
                self.busy = false;
                if self.document_version() != Some(version) {
                    return WorkspaceEffects::default();
                }
                *outcome
            }
            outcome => outcome,
        };
        let is_transient_preview = matches!(
            &outcome,
            WorkspaceOutcome::Image {
                update: Some(DocumentUpdate { preview: true, .. }),
                ..
            }
        ) || matches!(&outcome, WorkspaceOutcome::PreviewFailed { .. });
        if is_transient_preview && normal_job_running {
            return WorkspaceEffects::default();
        }
        if !is_transient_preview {
            self.busy = false;
        }
        let mut effects = WorkspaceEffects::default();
        match outcome {
            WorkspaceOutcome::Loaded {
                images,
                source_names,
                source_bytes,
                thumbs,
                info,
                status,
                single,
                append,
            } => {
                if single {
                    let mut images = images.into_iter();
                    if let Some(image) = images.next() {
                        self.document = Some(CurrentDocument::new(image));
                        self.document_name = source_names.into_iter().next();
                        self.source_bytes = source_bytes.map(Arc::new);
                        self.result_bytes = None;
                        self.show_document_preview();
                    }
                } else {
                    let loaded_thumbs = if thumbs.is_empty() {
                        images.iter().map(to_thumb).collect()
                    } else {
                        thumbs
                    };
                    if append {
                        self.thumbs.extend(loaded_thumbs);
                        self.input_set.append(images, source_names);
                    } else {
                        self.thumbs = loaded_thumbs;
                        self.input_set.replace(images, source_names);
                    }
                    self.retry_batch = None;
                    if !append {
                        self.focus_index = 0;
                    }
                    self.result_bytes = None;
                    self.preview_source_at(self.focus_index);
                }
                self.info = info;
                self.status = status;
            }
            WorkspaceOutcome::Image {
                image,
                status,
                update,
            } => {
                let accepted = if let Some(update) = update {
                    let Some(document) = self.document.as_mut() else {
                        return effects;
                    };
                    if update.preview {
                        update.preview_generation == Some(self.preview_generation)
                            && document.set_transient(update.base_version, image, update.kind)
                    } else {
                        document.commit(update.base_version, image, update.kind)
                    }
                } else {
                    if self
                        .document
                        .as_ref()
                        .is_some_and(CurrentDocument::is_dirty)
                    {
                        effects.replacement_document =
                            Some(PendingDocumentReplacement { image, status });
                        return effects;
                    }
                    self.use_generated_result(image, status.clone());
                    true
                };
                if accepted {
                    self.result_bytes = None;
                    self.show_document_preview();
                    self.status = status;
                }
            }
            WorkspaceOutcome::Bytes {
                bytes,
                extension,
                kind,
                preview,
                status,
            } => {
                if let Some(preview) = preview {
                    self.preview = Some(to_render_image(&preview));
                    self.preview_dimensions = Some(preview.dimensions());
                }
                self.result_bytes = Some(Arc::new(EncodedResult {
                    bytes,
                    extension,
                    kind,
                }));
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
                exported_version,
            } => {
                self.status = status;
                if let Some(info) = info {
                    self.info = info;
                }
                effects.last_output_dir = Some(directory);
                if let (Some(version), Some(document)) = (exported_version, &mut self.document) {
                    effects.document_exported = document.mark_exported(version);
                }
            }
            WorkspaceOutcome::BatchFinished {
                status,
                directory,
                results,
                retry,
                preview,
            } => {
                self.batch_cancel = None;
                self.retry_batch = retry;
                self.info = InfoMessage::BatchResults(results);
                if let Some(preview) = preview {
                    self.preview = Some(to_render_image(&preview));
                    self.preview_dimensions = Some(preview.dimensions());
                }
                self.status = status;
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
                self.batch_cancel = None;
                log::error!("workspace operation failed: {detail}");
                self.status = UiMessage::Error(kind);
            }
            WorkspaceOutcome::PreviewFailed {
                generation,
                base_version,
                kind,
                detail,
            } => {
                if generation == Some(self.preview_generation)
                    && self.document_version() == Some(base_version)
                {
                    log::error!("workspace preview failed: {detail}");
                    self.status = UiMessage::Error(kind);
                }
            }
            WorkspaceOutcome::InputVersioned { .. }
            | WorkspaceOutcome::DocumentVersioned { .. } => {
                unreachable!("input-version wrapper is removed before applying its outcome")
            }
        }
        effects
    }

    fn first_images_with(&mut self, missing: UiMessage) -> Option<Arc<Vec<RgbaImage>>> {
        let Some(document) = &self.document else {
            self.status = missing;
            return None;
        };
        Some(Arc::new(vec![(*document.snapshot().image).clone()]))
    }

    fn all_images(&mut self) -> Option<Arc<Vec<RgbaImage>>> {
        if self.input_set.is_empty() {
            self.status = UiMessage::NeedMultipleImages;
            return None;
        }
        Some(self.input_set.images())
    }

    fn current_image_source(&self) -> Option<ImageSource> {
        self.document
            .as_ref()
            .map(CurrentDocument::snapshot)
            .map(ImageSource::Document)
    }

    fn document_snapshot_or_status(&mut self, missing: UiMessage) -> Option<DocumentSnapshot> {
        let snapshot = self.document.as_ref().map(CurrentDocument::snapshot);
        if snapshot.is_none() {
            self.status = missing;
        }
        snapshot
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
        append: bool,
    },
    LoadClipboard {
        bytes: Vec<u8>,
    },
    Save {
        source: SaveSource,
        path: PathBuf,
        collision_policy: CollisionPolicy,
    },
    Transform {
        document: DocumentSnapshot,
        operation: TransformOperation,
        preview: bool,
        preview_generation: Option<u64>,
    },
    ReadExif {
        bytes: Arc<Vec<u8>>,
    },
    StripExif {
        document: DocumentSnapshot,
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
        input_version: InputSetVersion,
        layout: CollageLayout,
        options: CollageOptions,
    },
    Slice {
        images: Arc<Vec<RgbaImage>>,
        document_version: DocumentVersion,
        grid: SliceGrid,
        directory: PathBuf,
        collision_policy: CollisionPolicy,
    },
    Gif {
        images: Arc<Vec<RgbaImage>>,
        input_version: InputSetVersion,
        params: GifParams,
    },
    Batch {
        images: Arc<Vec<RgbaImage>>,
        names: Arc<Vec<String>>,
        input_version: InputSetVersion,
        operations: Vec<BatchStepOperation>,
        output: EncodeSettings,
        directory: PathBuf,
        collision_policy: CollisionPolicy,
        cancel: Arc<AtomicBool>,
        preview_index: usize,
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

impl TransformOperation {
    pub(crate) fn edit_kind(&self) -> EditKind {
        match self {
            Self::Rotate(_) => EditKind::Rotate,
            Self::Crop(_) => EditKind::Crop,
            Self::Resize { .. } => EditKind::Resize,
            Self::Beautify(_) => EditKind::Beautify,
        }
    }
}

impl WorkspaceOperation {
    pub(crate) fn edit_kind(&self) -> Option<EditKind> {
        match self {
            Self::Transform(operation) | Self::PreviewTransform(operation) => {
                Some(operation.edit_kind())
            }
            _ => None,
        }
    }
}

pub(crate) struct DocumentUpdate {
    base_version: DocumentVersion,
    kind: EditKind,
    preview: bool,
    preview_generation: Option<u64>,
}

#[derive(Clone)]
pub(crate) enum BatchStepOperation {
    Ready(BatchStep),
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
            Self::LoadPaths {
                paths,
                single,
                append,
            } => load_paths(paths, single, append),
            Self::LoadClipboard { bytes } => match decode_bytes(&bytes) {
                Ok(image) => {
                    let thumbs = vec![to_thumb(&image)];
                    WorkspaceOutcome::Loaded {
                        info: InfoMessage::ClipboardImage {
                            width: image.width(),
                            height: image.height(),
                        },
                        images: vec![image],
                        source_names: vec!["clipboard".to_string()],
                        source_bytes: Some(bytes),
                        thumbs,
                        status: UiMessage::ImagePasted,
                        single: true,
                        append: false,
                    }
                }
                Err((kind, detail)) => WorkspaceOutcome::Failed { kind, detail },
            },
            Self::Save {
                source,
                path,
                collision_policy,
            } => {
                let exported_version = match &source {
                    SaveSource::Image {
                        image: ImageSource::Document(snapshot),
                        ..
                    } => Some(snapshot.version),
                    _ => None,
                };
                let bytes = match source {
                    SaveSource::Image { image, settings } => {
                        match format::encode(image.image(), settings) {
                            Ok(bytes) => bytes,
                            Err(error) => return failure(ErrorKind::Encode, error),
                        }
                    }
                    SaveSource::Bytes { bytes } => bytes.bytes.clone(),
                };
                match export_plan::publish_image(&path, &bytes, collision_policy) {
                    Ok(PublishOutcome::Written(written_path)) => WorkspaceOutcome::Written {
                        status: UiMessage::Exported,
                        directory: parent_or_path(&written_path),
                        info: None,
                        exported_version,
                    },
                    Ok(PublishOutcome::Skipped(skipped_path)) => WorkspaceOutcome::Written {
                        status: UiMessage::ExportSkipped,
                        directory: parent_or_path(&skipped_path),
                        info: None,
                        exported_version: None,
                    },
                    Err(error) => io_failure(&path, error),
                }
            }
            Self::Transform {
                document,
                operation,
                preview,
                preview_generation,
            } => {
                let kind = operation.edit_kind();
                let result = match operation {
                    TransformOperation::Rotate(rotation) => {
                        Ok(transform::rotate(&document.image, rotation))
                    }
                    TransformOperation::Crop(rect) => transform::crop(&document.image, rect),
                    TransformOperation::Resize { width, height } => {
                        transform::resize(&document.image, width, height, ResizeFilter::default())
                    }
                    TransformOperation::Beautify(params) => {
                        beautify::beautify(&document.image, &params)
                    }
                };
                match result {
                    Ok(image) => WorkspaceOutcome::Image {
                        image,
                        status: UiMessage::SavedReady,
                        update: Some(DocumentUpdate {
                            base_version: document.version,
                            kind,
                            preview,
                            preview_generation,
                        }),
                    },
                    Err(error) if preview => WorkspaceOutcome::PreviewFailed {
                        generation: preview_generation,
                        base_version: document.version,
                        kind: ErrorKind::Operation,
                        detail: error.to_string(),
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
            Self::StripExif { document } => versioned_document(
                document.version,
                match format::encode(
                    &document.image,
                    EncodeSettings::Png {
                        compression: PngCompression::Default,
                    },
                ) {
                    Ok(clean) => WorkspaceOutcome::Bytes {
                        extension: "png",
                        kind: EncodedResultKind::Exif,
                        bytes: clean,
                        preview: Some((*document.image).clone()),
                        status: UiMessage::ExifStripped,
                    },
                    Err(error) => failure(ErrorKind::Exif, error),
                },
            ),
            Self::GenerateQr { text, options } => match qr::generate(&text, options) {
                Ok(image) => WorkspaceOutcome::Image {
                    image,
                    status: UiMessage::QrGenerated,
                    update: None,
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
                input_version,
                layout,
                options,
            } => versioned_input(
                input_version,
                match collage::compose(&images, layout, options) {
                    Ok(image) => WorkspaceOutcome::Image {
                        image,
                        status: UiMessage::CollageReady,
                        update: None,
                    },
                    Err(error) => failure(ErrorKind::Operation, error),
                },
            ),
            Self::Slice {
                images,
                document_version,
                grid,
                directory,
                collision_policy,
            } => versioned_document(
                document_version,
                match slice::slice(&images[0], grid) {
                    Ok(tiles) => {
                        let total = tiles.len();
                        let requested_paths = tiles
                            .iter()
                            .map(|tile| directory.join(naming::tile_filename(tile)))
                            .collect::<Vec<_>>();
                        let resolved_paths =
                            export_plan::resolve_destinations(&requested_paths, collision_policy);
                        let mut successes = 0;
                        for (tile, destination) in tiles.into_iter().zip(resolved_paths) {
                            let Some(path) = destination else {
                                continue;
                            };
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
                            match export_plan::publish_image(&path, &bytes, collision_policy) {
                                Ok(PublishOutcome::Written(_)) => successes += 1,
                                Ok(PublishOutcome::Skipped(_)) => {}
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
                            exported_version: None,
                        }
                    }
                    Err(error) => failure(ErrorKind::Operation, error),
                },
            ),
            Self::Gif {
                images,
                input_version,
                params,
            } => versioned_input(
                input_version,
                match animation::compose(&images, &params) {
                    Ok(bytes) => WorkspaceOutcome::Bytes {
                        bytes,
                        extension: "gif",
                        kind: EncodedResultKind::Gif,
                        preview: images.first().cloned(),
                        status: UiMessage::GifReady,
                    },
                    Err(error) => failure(ErrorKind::Operation, error),
                },
            ),
            Self::Batch {
                images,
                names,
                input_version,
                operations,
                output,
                directory,
                collision_policy,
                cancel,
                preview_index,
            } => {
                let retry_operations = operations.clone();
                let steps = match operations
                    .into_iter()
                    .map(prepare_batch_operation)
                    .collect::<Result<Vec<_>, _>>()
                {
                    Ok(steps) => steps,
                    Err((kind, detail)) => {
                        return versioned_input(
                            input_version,
                            WorkspaceOutcome::Failed { kind, detail },
                        );
                    }
                };
                let pipeline = BatchPipeline { steps, output };
                let total = images.len();
                let requested_paths = names
                    .iter()
                    .enumerate()
                    .map(|(index, source_stem)| {
                        directory.join(naming::batch_filename(
                            source_stem,
                            index,
                            pipeline.output.format(),
                        ))
                    })
                    .collect::<Vec<_>>();
                let resolved_paths =
                    export_plan::resolve_destinations(&requested_paths, collision_policy);
                report_progress(0, total);
                let mut successes = 0;
                let mut failures = 0;
                let mut cancelled = false;
                let mut item_results = Vec::with_capacity(total);
                let mut retry_images = Vec::new();
                let mut retry_names = Vec::new();
                let mut selected_preview = None;
                for (index, image) in images.iter().enumerate() {
                    if cancel.load(Ordering::Relaxed) {
                        cancelled = true;
                        item_results.extend((index..total).map(|remaining| BatchItemResult {
                            name: batch_item_name(&names, remaining),
                            width: None,
                            height: None,
                            bytes: None,
                            outcome: BatchItemOutcome::Cancelled,
                        }));
                        break;
                    }
                    let mut result =
                        batch::process_pipeline(std::slice::from_ref(image), &pipeline);
                    let Some((_, output)) = result.successes.pop() else {
                        failures += 1;
                        let name = batch_item_name(&names, index);
                        let kind = if let Some(failure) = result.failures.pop() {
                            log::error!("batch item {} failed: {}", index + 1, failure.error);
                            core_error_kind(&failure.error)
                        } else {
                            ErrorKind::Operation
                        };
                        item_results.push(BatchItemResult {
                            name: name.clone(),
                            width: None,
                            height: None,
                            bytes: None,
                            outcome: BatchItemOutcome::Failure(kind),
                        });
                        retry_images.push(image.clone());
                        retry_names.push(name);
                        report_progress(index + 1, total);
                        continue;
                    };
                    let output_size = output.bytes.len();
                    let output_width = output.width;
                    let output_height = output.height;
                    let name = batch_item_name(&names, index);
                    if index == preview_index {
                        selected_preview = format::decode(&output.bytes).ok();
                    }
                    let Some(path) = resolved_paths.get(index).cloned().flatten() else {
                        failures += 1;
                        item_results.push(BatchItemResult {
                            name,
                            width: Some(output_width),
                            height: Some(output_height),
                            bytes: Some(output_size),
                            outcome: BatchItemOutcome::Failure(ErrorKind::Skipped),
                        });
                        report_progress(index + 1, total);
                        continue;
                    };
                    match export_plan::publish_image(&path, &output.bytes, collision_policy) {
                        Ok(PublishOutcome::Written(_)) => {
                            successes += 1;
                            item_results.push(BatchItemResult {
                                name,
                                width: Some(output_width),
                                height: Some(output_height),
                                bytes: Some(output_size),
                                outcome: BatchItemOutcome::Success,
                            });
                        }
                        Ok(PublishOutcome::Skipped(_)) => {
                            failures += 1;
                            item_results.push(BatchItemResult {
                                name: name.clone(),
                                width: Some(output_width),
                                height: Some(output_height),
                                bytes: Some(output_size),
                                outcome: BatchItemOutcome::Failure(ErrorKind::Skipped),
                            });
                        }
                        Err(error) => {
                            failures += 1;
                            item_results.push(BatchItemResult {
                                name: name.clone(),
                                width: Some(output_width),
                                height: Some(output_height),
                                bytes: Some(output_size),
                                outcome: BatchItemOutcome::Failure(classify_io(&error)),
                            });
                            retry_images.push(image.clone());
                            retry_names.push(name);
                            log::error!(
                                "batch output write failed for {}: {error}",
                                path.display()
                            );
                        }
                    }
                    report_progress(index + 1, total);
                }
                let retry = (!retry_images.is_empty()).then_some(RetryBatch {
                    images: Arc::new(retry_images),
                    names: Arc::new(retry_names),
                    operations: retry_operations,
                    output,
                    directory: directory.clone(),
                    collision_policy,
                });
                versioned_input(
                    input_version,
                    WorkspaceOutcome::BatchFinished {
                        status: if cancelled {
                            UiMessage::BatchCancelled { successes, total }
                        } else {
                            UiMessage::BatchComplete {
                                successes,
                                failures,
                                total,
                            }
                        },
                        directory,
                        results: item_results,
                        retry,
                        preview: selected_preview,
                    },
                )
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
    InputVersioned {
        version: InputSetVersion,
        outcome: Box<WorkspaceOutcome>,
    },
    DocumentVersioned {
        version: DocumentVersion,
        outcome: Box<WorkspaceOutcome>,
    },
    Loaded {
        images: Vec<RgbaImage>,
        source_names: Vec<String>,
        source_bytes: Option<Vec<u8>>,
        thumbs: Vec<Arc<RenderImage>>,
        info: InfoMessage,
        status: UiMessage,
        single: bool,
        append: bool,
    },
    Image {
        image: RgbaImage,
        status: UiMessage,
        update: Option<DocumentUpdate>,
    },
    Bytes {
        bytes: Vec<u8>,
        extension: &'static str,
        kind: EncodedResultKind,
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
        exported_version: Option<DocumentVersion>,
    },
    BatchFinished {
        status: UiMessage,
        directory: PathBuf,
        results: Vec<BatchItemResult>,
        retry: Option<RetryBatch>,
        preview: Option<RgbaImage>,
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
    PreviewFailed {
        generation: Option<u64>,
        base_version: DocumentVersion,
        kind: ErrorKind,
        detail: String,
    },
}

#[derive(Default)]
pub(crate) struct WorkspaceEffects {
    pub last_output_dir: Option<PathBuf>,
    pub clipboard: Option<ClipboardPayload>,
    pub document_exported: bool,
    pub replacement_document: Option<PendingDocumentReplacement>,
}

pub(crate) struct PendingDocumentReplacement {
    pub image: RgbaImage,
    pub status: UiMessage,
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

fn prepare_batch_operation(
    operation: BatchStepOperation,
) -> Result<BatchStep, (ErrorKind, String)> {
    match operation {
        BatchStepOperation::Ready(operation) => Ok(operation),
        BatchStepOperation::TextWatermark {
            text,
            opacity,
            position,
        } => text_watermark::rasterize(&text)
            .map(|overlay| BatchStep::Watermark {
                overlay,
                opacity,
                position,
            })
            .map_err(|detail| (ErrorKind::Operation, detail)),
        BatchStepOperation::ImageWatermark {
            path,
            opacity,
            position,
        } => {
            let bytes = std::fs::read(&path)
                .map_err(|error| (classify_io(&error), format!("{}: {error}", path.display())))?;
            let overlay = decode_bytes(&bytes)?;
            Ok(BatchStep::Watermark {
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

fn load_paths(paths: Vec<PathBuf>, single: bool, append: bool) -> WorkspaceOutcome {
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
                    source_names.push(if single {
                        path.file_name()
                            .map(|name| name.to_string_lossy().into_owned())
                            .unwrap_or_else(|| "image".to_string())
                    } else {
                        path.file_stem()
                            .map(|name| name.to_string_lossy().into_owned())
                            .unwrap_or_else(|| "image".to_string())
                    });
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
    let thumbs = images.iter().map(to_thumb).collect();
    WorkspaceOutcome::Loaded {
        images,
        source_names,
        source_bytes: if single { first_bytes } else { None },
        thumbs,
        info,
        status: if single {
            UiMessage::LoadedImage
        } else {
            UiMessage::LoadedMultiple {
                successes,
                failures,
            }
        },
        single,
        append,
    }
}

fn versioned_input(version: InputSetVersion, outcome: WorkspaceOutcome) -> WorkspaceOutcome {
    WorkspaceOutcome::InputVersioned {
        version,
        outcome: Box::new(outcome),
    }
}

fn versioned_document(version: DocumentVersion, outcome: WorkspaceOutcome) -> WorkspaceOutcome {
    WorkspaceOutcome::DocumentVersioned {
        version,
        outcome: Box::new(outcome),
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

/// 把 `impressy-core` 的 RGBA 图转成 GPUI 的 [`RenderImage`]（BGRA 序）。
pub(crate) fn to_render_image(img: &RgbaImage) -> Arc<RenderImage> {
    let mut bgra = img.clone();
    for pixel in bgra.pixels_mut() {
        pixel.0.swap(0, 2);
    }
    Arc::new(RenderImage::new(vec![Frame::new(bgra)]))
}

fn to_thumb(img: &RgbaImage) -> Arc<RenderImage> {
    const MAX: u32 = 72;
    let (width, height) = img.dimensions();
    let long_edge = width.max(height).max(1);
    if long_edge <= MAX {
        return to_render_image(img);
    }
    let next_width = ((u64::from(width) * u64::from(MAX)) / u64::from(long_edge)).max(1) as u32;
    let next_height = ((u64::from(height) * u64::from(MAX)) / u64::from(long_edge)).max(1) as u32;
    match transform::resize(img, next_width, next_height, ResizeFilter::Bilinear) {
        Ok(small) => to_render_image(&small),
        Err(_) => to_render_image(img),
    }
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
            WorkspaceJob::LoadPaths { paths, single, .. } => {
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
                None,
            )
            .expect("a selected path should prepare the save job");

        assert!(matches!(job, WorkspaceJob::Save { path: job_path, .. } if job_path == path));
        assert!(workspace.is_busy());
    }

    #[test]
    fn dirty_document_export_ignores_an_unrelated_encoded_feature_result() {
        let mut workspace = workspace_with_images(1);
        workspace.result_bytes = Some(Arc::new(EncodedResult {
            bytes: vec![1, 2, 3],
            extension: "gif",
            kind: EncodedResultKind::Gif,
        }));

        let job = workspace
            .prepare_document_export_path(
                PathBuf::from("document.png"),
                EncodeSettings::Png {
                    compression: PngCompression::Default,
                },
                CollisionPolicy::PreserveBoth,
            )
            .expect("the guarded export should target the dirty document");

        assert!(matches!(
            job,
            WorkspaceJob::Save {
                source: SaveSource::Image {
                    image: ImageSource::Document(_),
                    ..
                },
                ..
            }
        ));
    }

    #[test]
    fn encoded_feature_result_is_exportable_without_a_current_document() {
        let mut workspace = Workspace {
            result_bytes: Some(Arc::new(EncodedResult {
                bytes: vec![1, 2, 3],
                extension: "gif",
                kind: EncodedResultKind::Gif,
            })),
            ..Workspace::default()
        };

        assert!(workspace.has_encoded_result(EncodedResultKind::Gif));
        let job = workspace
            .prepare_save_path(
                PathBuf::from("animation.gif"),
                EncodeSettings::Png {
                    compression: PngCompression::Default,
                },
                Some(EncodedResultKind::Gif),
            )
            .expect("an encoded feature result should remain exportable");
        assert!(matches!(
            job,
            WorkspaceJob::Save {
                source: SaveSource::Bytes { .. },
                ..
            }
        ));
    }

    #[test]
    fn encoded_results_are_scoped_to_the_feature_that_created_them() {
        let mut workspace = workspace_with_images(1);
        workspace.result_bytes = Some(Arc::new(EncodedResult {
            bytes: vec![1, 2, 3],
            extension: "gif",
            kind: EncodedResultKind::Gif,
        }));

        let job = workspace
            .prepare_save_path(
                PathBuf::from("document.png"),
                EncodeSettings::Png {
                    compression: PngCompression::Default,
                },
                Some(EncodedResultKind::Exif),
            )
            .expect("a result from another feature must fall back to the document");

        assert!(matches!(
            job,
            WorkspaceJob::Save {
                source: SaveSource::Image { .. },
                ..
            }
        ));
    }

    #[test]
    fn export_extension_matches_the_actual_selected_encoder() {
        let mut workspace = workspace_with_images(1);
        workspace.result_bytes = Some(Arc::new(EncodedResult {
            bytes: vec![1, 2, 3],
            extension: "gif",
            kind: EncodedResultKind::Gif,
        }));

        assert_eq!(
            workspace.export_extension(
                EncodeSettings::Jpeg {
                    quality: impressy_core::format::Quality::new(85).unwrap(),
                },
                None,
            ),
            "jpg"
        );
        assert_eq!(
            workspace.export_extension(
                EncodeSettings::Png {
                    compression: PngCompression::Default,
                },
                Some(EncodedResultKind::Gif),
            ),
            "gif"
        );
    }

    #[test]
    fn generated_output_defers_replacement_of_a_dirty_document() {
        let mut workspace = workspace_with_images(1);
        let version = workspace.document_version().unwrap();
        workspace.document.as_mut().unwrap().commit(
            version,
            RgbaImage::from_pixel(2, 2, Rgba([2, 0, 0, 255])),
            EditKind::Rotate,
        );

        let effects = workspace.apply(WorkspaceOutcome::Image {
            image: RgbaImage::from_pixel(2, 2, Rgba([3, 0, 0, 255])),
            status: UiMessage::QrGenerated,
            update: None,
        });

        assert_eq!(
            workspace
                .document
                .as_ref()
                .unwrap()
                .snapshot()
                .image
                .get_pixel(0, 0)
                .0[0],
            2
        );
        let replacement = effects
            .replacement_document
            .expect("dirty document replacement must be deferred to the shell guard");
        assert_eq!(replacement.image.get_pixel(0, 0).0[0], 3);
        assert_eq!(replacement.status, UiMessage::QrGenerated);
    }

    #[test]
    fn late_preview_cannot_replace_a_newer_task_status() {
        let mut workspace = workspace_with_images(1);
        let preview = workspace
            .prepare(WorkspaceOperation::PreviewTransform(
                TransformOperation::Resize {
                    width: 1,
                    height: 1,
                },
            ))
            .expect("preview job");
        workspace.busy = true;
        workspace.status = UiMessage::BatchProgress {
            completed: 1,
            total: 2,
        };

        workspace.apply(preview.run_with_progress(|_, _| {}));

        assert_eq!(
            workspace.status,
            UiMessage::BatchProgress {
                completed: 1,
                total: 2,
            }
        );
        assert!(!workspace.has_transient_preview());
    }

    #[test]
    fn requesting_a_new_preview_removes_the_previous_transient() {
        let mut workspace = workspace_with_images(1);
        let version = workspace.document_version().unwrap();
        workspace.document.as_mut().unwrap().set_transient(
            version,
            RgbaImage::from_pixel(1, 1, Rgba([4, 0, 0, 255])),
            EditKind::Resize,
        );
        assert!(workspace.has_transient_preview());

        let job = workspace.prepare(WorkspaceOperation::PreviewTransform(
            TransformOperation::Resize {
                width: 2,
                height: 2,
            },
        ));

        assert!(job.is_some());
        assert!(!workspace.has_transient_preview());
    }

    #[test]
    fn ai_reference_uses_applied_pixels_instead_of_transient_preview_pixels() {
        let mut workspace = workspace_with_images(1);
        let version = workspace.document_version().unwrap();
        workspace.document.as_mut().unwrap().set_transient(
            version,
            RgbaImage::from_pixel(2, 2, Rgba([9, 0, 0, 255])),
            EditKind::Beautify,
        );

        let reference = workspace
            .ai_reference_snapshot()
            .expect("document reference");

        assert_eq!(reference.version, version);
        assert_eq!(reference.image.get_pixel(0, 0).0[0], 255);
    }

    #[test]
    fn busy_workspace_rejects_generated_document_replacement() {
        let mut workspace = workspace_with_images(1);
        let version = workspace.document_version();
        workspace.busy = true;

        assert!(!workspace.use_ai_result(RgbaImage::from_pixel(2, 2, Rgba([9, 0, 0, 255]),)));
        assert_eq!(workspace.document_version(), version);
        assert_eq!(workspace.status, UiMessage::Busy);
    }

    #[test]
    fn strip_exif_uses_the_latest_applied_document_pixels() {
        let mut workspace = workspace_with_images(1);
        let version = workspace.document_version().unwrap();
        workspace.document.as_mut().unwrap().commit(
            version,
            RgbaImage::from_pixel(2, 2, Rgba([7, 8, 9, 255])),
            EditKind::Rotate,
        );
        let job = workspace
            .prepare(WorkspaceOperation::StripExif)
            .expect("strip EXIF job");

        let WorkspaceOutcome::DocumentVersioned { outcome, .. } = job.run_with_progress(|_, _| {})
        else {
            panic!("strip EXIF outcome must be document-versioned");
        };
        let WorkspaceOutcome::Bytes { bytes, kind, .. } = *outcome else {
            panic!("strip EXIF must produce encoded bytes");
        };
        assert_eq!(kind, EncodedResultKind::Exif);
        assert_eq!(
            format::decode(&bytes).unwrap().get_pixel(0, 0).0,
            [7, 8, 9, 255]
        );
    }

    #[test]
    fn concrete_directory_prepares_slice_job_without_opening_a_dialog() {
        let mut workspace = workspace_with_images(1);
        let directory = PathBuf::from("tiles");
        let grid = SliceGrid::new(2, 3).expect("valid slice grid");

        let job = workspace
            .prepare_slice_directory(grid, directory.clone(), CollisionPolicy::PreserveBoth)
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
        let request = BatchRequest::Pipeline {
            steps: vec![BatchRequestStep::Watermark {
                source: WatermarkSource::Image,
                text: String::new(),
                opacity: 0.5,
                placement: WatermarkPlacement::BottomRight,
            }],
            output: EncodeSettings::Png {
                compression: PngCompression::Fast,
            },
            collision_policy: CollisionPolicy::PreserveBoth,
        };

        let job = workspace
            .prepare_batch_paths(request, directory.clone(), Some(watermark.clone()))
            .expect("selected output and watermark paths should prepare the batch job");

        assert!(matches!(
            job,
            WorkspaceJob::Batch {
                operations,
                directory: job_directory,
                ..
            } if matches!(operations.as_slice(), [BatchStepOperation::ImageWatermark { path, .. }] if path == &watermark)
                && job_directory == directory
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
                    None,
                )
                .is_none()
        );
        assert_eq!(workspace.status, UiMessage::Busy);
    }

    fn workspace_with_images(count: usize) -> Workspace {
        let images = (0..count)
            .map(|_| RgbaImage::from_pixel(2, 2, Rgba([255, 0, 0, 255])))
            .collect::<Vec<_>>();
        let names = (0..count)
            .map(|index| format!("image-{index}"))
            .collect::<Vec<_>>();
        let mut workspace = Workspace::default();
        workspace.input_set.replace(images.clone(), names);
        if let Some(image) = images.into_iter().next() {
            workspace.document = Some(CurrentDocument::new(image));
        }
        workspace
    }

    #[test]
    fn removing_and_reordering_images_preserves_names_and_focus() {
        let mut workspace = workspace_with_images(3);
        assert!(workspace.move_image(2, 0));
        assert_eq!(workspace.source_name(0), Some("image-2"));
        assert_eq!(workspace.source_name(1), Some("image-0"));
        assert_eq!(workspace.focus_index(), 0);

        assert!(workspace.remove_image(1));
        assert_eq!(workspace.image_count(), 2);
        assert_eq!(workspace.source_name(0), Some("image-2"));
        assert_eq!(workspace.source_name(1), Some("image-1"));
        assert!(!workspace.is_busy());
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
            input_version: InputSetVersion::INITIAL,
            operations: vec![],
            output: EncodeSettings::Png {
                compression: PngCompression::Fast,
            },
            directory: directory.path().to_path_buf(),
            collision_policy: CollisionPolicy::PreserveBoth,
            cancel: Arc::new(AtomicBool::new(false)),
            preview_index: 0,
        };
        let mut progress = Vec::new();
        let outcome = job.run_with_progress(|completed, total| {
            progress.push((completed, total));
        });

        match outcome {
            WorkspaceOutcome::InputVersioned { outcome, .. } => match *outcome {
                WorkspaceOutcome::BatchFinished { status, .. } => assert_eq!(
                    status,
                    UiMessage::BatchComplete {
                        successes: 2,
                        failures: 0,
                        total: 2,
                    }
                ),
                _ => panic!("batch job should report batch results"),
            },
            _ => panic!("batch job should write its outputs"),
        }
        assert!(directory.path().join("hero_cover-impressy-1.png").is_file());
        assert!(directory.path().join("second-impressy-2.png").is_file());
        assert_eq!(progress, vec![(0, 2), (1, 2), (2, 2)]);
    }

    // Feature: impressy, Property 27: errors leave the application workspace reusable
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
            input_version: InputSetVersion::INITIAL,
            operations: vec![BatchStepOperation::Ready(BatchStep::Resize {
                mode: BatchResizeMode::Pixels {
                    width: 0,
                    height: 0,
                    preserve_aspect: false,
                },
                filter: ResizeFilter::Nearest,
                prevent_enlarge: false,
            })],
            output: EncodeSettings::Png {
                compression: PngCompression::Fast,
            },
            directory: directory.path().to_path_buf(),
            collision_policy: CollisionPolicy::PreserveBoth,
            cancel: Arc::new(AtomicBool::new(false)),
            preview_index: 0,
        };

        let outcome = job.run_with_progress(|_, _| {});
        match outcome {
            WorkspaceOutcome::InputVersioned { outcome, .. } => match *outcome {
                WorkspaceOutcome::BatchFinished { results, retry, .. } => {
                    assert_eq!(results.len(), 1);
                    assert_eq!(results[0].name, "broken-item");
                    assert_eq!(
                        results[0].outcome,
                        BatchItemOutcome::Failure(ErrorKind::Operation)
                    );
                    assert!(retry.is_some());
                }
                _ => panic!("batch failure should report item results"),
            },
            _ => panic!("batch failure should preserve the item reason"),
        }
    }

    #[test]
    fn cancelled_batch_reports_unstarted_items_without_writing_them() {
        let directory = tempfile::tempdir().expect("temporary output directory");
        let cancel = Arc::new(AtomicBool::new(true));
        let job = WorkspaceJob::Batch {
            images: Arc::new(vec![
                RgbaImage::from_pixel(2, 2, Rgba([255, 0, 0, 255])),
                RgbaImage::from_pixel(2, 2, Rgba([0, 255, 0, 255])),
            ]),
            names: Arc::new(vec!["first".to_string(), "second".to_string()]),
            input_version: InputSetVersion::INITIAL,
            operations: vec![],
            output: EncodeSettings::Png {
                compression: PngCompression::Fast,
            },
            directory: directory.path().to_path_buf(),
            collision_policy: CollisionPolicy::PreserveBoth,
            cancel,
            preview_index: 0,
        };

        match job.run_with_progress(|_, _| {}) {
            WorkspaceOutcome::InputVersioned { outcome, .. } => match *outcome {
                WorkspaceOutcome::BatchFinished {
                    status, results, ..
                } => {
                    assert_eq!(
                        status,
                        UiMessage::BatchCancelled {
                            successes: 0,
                            total: 2
                        }
                    );
                    assert!(
                        results
                            .iter()
                            .all(|item| item.outcome == BatchItemOutcome::Cancelled)
                    );
                }
                _ => panic!("cancelled batch should produce a batch summary"),
            },
            _ => panic!("cancelled batch should retain its input version"),
        }
        assert!(
            directory
                .path()
                .read_dir()
                .expect("read directory")
                .next()
                .is_none()
        );
    }

    #[test]
    fn stale_input_outcome_does_not_replace_current_status() {
        let mut workspace = workspace_with_images(2);
        let stale_version = workspace.input_set.version();
        workspace.input_set.move_item(0, 1);
        workspace.status = UiMessage::LoadedImage;

        workspace.apply(versioned_input(
            stale_version,
            WorkspaceOutcome::Info {
                info: InfoMessage::None,
                status: UiMessage::CollageReady,
            },
        ));

        assert_eq!(workspace.status, UiMessage::LoadedImage);
    }

    #[test]
    fn stale_document_outcome_does_not_replace_current_status() {
        let mut workspace = workspace_with_images(1);
        let stale_version = workspace.document_version().unwrap();
        workspace.document.as_mut().unwrap().commit(
            stale_version,
            RgbaImage::from_pixel(2, 2, Rgba([4, 0, 0, 255])),
            EditKind::Rotate,
        );
        workspace.status = UiMessage::LoadedImage;

        workspace.apply(versioned_document(
            stale_version,
            WorkspaceOutcome::Info {
                info: InfoMessage::None,
                status: UiMessage::SlicesSaved {
                    successes: 1,
                    total: 1,
                },
            },
        ));

        assert_eq!(workspace.status, UiMessage::LoadedImage);
    }
}
