//! Current-document state shared by local editing and reference-image AI flows.

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use image::RgbaImage;

pub(crate) const DEFAULT_HISTORY_BUDGET_BYTES: usize = 256 * 1024 * 1024;

static NEXT_DOCUMENT_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct DocumentVersion {
    document_id: u64,
    revision: u64,
}

impl DocumentVersion {
    #[cfg(test)]
    pub(crate) const INITIAL: Self = Self {
        document_id: 1,
        revision: 1,
    };

    fn new_document() -> Self {
        Self {
            document_id: NEXT_DOCUMENT_ID.fetch_add(1, Ordering::Relaxed),
            revision: 1,
        }
    }

    fn next(self) -> Self {
        Self {
            document_id: self.document_id,
            revision: self.revision.wrapping_add(1).max(1),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct InputSetVersion(u64);

impl InputSetVersion {
    #[cfg(test)]
    pub(crate) const INITIAL: Self = Self(1);
}

#[derive(Default)]
pub(crate) struct InputSet {
    images: Arc<Vec<RgbaImage>>,
    names: Arc<Vec<String>>,
    version: u64,
}

impl InputSet {
    pub(crate) fn replace(&mut self, images: Vec<RgbaImage>, names: Vec<String>) {
        debug_assert_eq!(images.len(), names.len());
        self.images = Arc::new(images);
        self.names = Arc::new(names);
        self.bump_version();
    }

    pub(crate) fn append(&mut self, images: Vec<RgbaImage>, names: Vec<String>) {
        debug_assert_eq!(images.len(), names.len());
        Arc::make_mut(&mut self.images).extend(images);
        Arc::make_mut(&mut self.names).extend(names);
        self.bump_version();
    }

    pub(crate) fn images(&self) -> Arc<Vec<RgbaImage>> {
        Arc::clone(&self.images)
    }

    pub(crate) fn names(&self) -> Arc<Vec<String>> {
        Arc::clone(&self.names)
    }

    pub(crate) fn name(&self, index: usize) -> Option<&str> {
        self.names.get(index).map(String::as_str)
    }

    pub(crate) fn image(&self, index: usize) -> Option<&RgbaImage> {
        self.images.get(index)
    }

    pub(crate) fn len(&self) -> usize {
        self.images.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.images.is_empty()
    }

    pub(crate) fn version(&self) -> InputSetVersion {
        InputSetVersion(self.version)
    }

    pub(crate) fn remove(&mut self, index: usize) -> bool {
        if index >= self.images.len() {
            return false;
        }
        Arc::make_mut(&mut self.images).remove(index);
        Arc::make_mut(&mut self.names).remove(index);
        self.bump_version();
        true
    }

    pub(crate) fn move_item(&mut self, from: usize, to: usize) -> bool {
        if from == to || from >= self.images.len() || to >= self.images.len() {
            return false;
        }
        let image = Arc::make_mut(&mut self.images).remove(from);
        Arc::make_mut(&mut self.images).insert(to, image);
        let name = Arc::make_mut(&mut self.names).remove(from);
        Arc::make_mut(&mut self.names).insert(to, name);
        self.bump_version();
        true
    }

    fn bump_version(&mut self) {
        self.version = self.version.wrapping_add(1).max(1);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EditKind {
    Crop,
    Resize,
    Rotate,
    Beautify,
    RestoreOriginal,
}

#[derive(Clone)]
pub(crate) struct DocumentSnapshot {
    pub(crate) version: DocumentVersion,
    pub(crate) image: Arc<RgbaImage>,
}

#[derive(Clone)]
struct Checkpoint {
    image: Arc<RgbaImage>,
    kind: EditKind,
    bytes: usize,
    exported: bool,
}

#[derive(Clone)]
pub(crate) struct TransientPreview {
    pub(crate) base_version: DocumentVersion,
    pub(crate) image: Arc<RgbaImage>,
    pub(crate) kind: EditKind,
}

pub(crate) struct CurrentDocument {
    original: Arc<RgbaImage>,
    current: Arc<RgbaImage>,
    version: DocumentVersion,
    current_exported: bool,
    undo: VecDeque<Checkpoint>,
    redo: Vec<Checkpoint>,
    history_bytes: usize,
    history_budget_bytes: usize,
    transient: Option<TransientPreview>,
}

impl CurrentDocument {
    pub(crate) fn new(image: RgbaImage) -> Self {
        Self::with_history_budget(image, DEFAULT_HISTORY_BUDGET_BYTES)
    }

    fn with_history_budget(image: RgbaImage, history_budget_bytes: usize) -> Self {
        let image = Arc::new(image);
        Self {
            original: Arc::clone(&image),
            current: image,
            version: DocumentVersion::new_document(),
            current_exported: true,
            undo: VecDeque::new(),
            redo: Vec::new(),
            history_bytes: 0,
            history_budget_bytes,
            transient: None,
        }
    }

    pub(crate) fn new_unexported(image: RgbaImage) -> Self {
        let mut document = Self::new(image);
        document.current_exported = false;
        document
    }

    pub(crate) fn snapshot(&self) -> DocumentSnapshot {
        DocumentSnapshot {
            version: self.version,
            image: Arc::clone(&self.current),
        }
    }

    pub(crate) fn original(&self) -> Arc<RgbaImage> {
        Arc::clone(&self.original)
    }

    pub(crate) fn displayed_image(&self) -> Arc<RgbaImage> {
        self.transient
            .as_ref()
            .map(|preview| {
                if preview.kind == EditKind::Crop {
                    Arc::clone(&self.current)
                } else {
                    Arc::clone(&preview.image)
                }
            })
            .unwrap_or_else(|| Arc::clone(&self.current))
    }

    pub(crate) fn set_transient(
        &mut self,
        base_version: DocumentVersion,
        image: RgbaImage,
        kind: EditKind,
    ) -> bool {
        if base_version != self.version {
            return false;
        }
        self.transient = Some(TransientPreview {
            base_version,
            image: Arc::new(image),
            kind,
        });
        true
    }

    pub(crate) fn apply_transient(&mut self) -> bool {
        self.apply_transient_for(None)
    }

    pub(crate) fn apply_transient_kind(&mut self, kind: EditKind) -> bool {
        self.apply_transient_for(Some(kind))
    }

    fn apply_transient_for(&mut self, expected_kind: Option<EditKind>) -> bool {
        if expected_kind.is_some_and(|expected| {
            self.transient
                .as_ref()
                .is_none_or(|preview| preview.kind != expected)
        }) {
            return false;
        }
        let Some(preview) = self.transient.take() else {
            return false;
        };
        if preview.base_version != self.version {
            return false;
        }
        self.commit_arc(preview.image, preview.kind);
        true
    }

    pub(crate) fn discard_transient(&mut self) -> bool {
        self.transient.take().is_some()
    }

    pub(crate) fn commit(
        &mut self,
        base_version: DocumentVersion,
        image: RgbaImage,
        kind: EditKind,
    ) -> bool {
        if base_version != self.version {
            return false;
        }
        self.transient = None;
        self.commit_arc(Arc::new(image), kind);
        true
    }

    pub(crate) fn undo(&mut self) -> bool {
        let Some(checkpoint) = self.undo.pop_back() else {
            return false;
        };
        self.transient = None;
        self.redo.push(Checkpoint {
            image: Arc::clone(&self.current),
            kind: checkpoint.kind,
            bytes: image_bytes(&self.current),
            exported: self.current_exported,
        });
        self.current = checkpoint.image;
        self.current_exported = checkpoint.exported;
        self.version = self.version.next();
        self.trim_history();
        true
    }

    pub(crate) fn redo(&mut self) -> bool {
        let Some(checkpoint) = self.redo.pop() else {
            return false;
        };
        self.transient = None;
        self.undo.push_back(Checkpoint {
            image: Arc::clone(&self.current),
            kind: checkpoint.kind,
            bytes: image_bytes(&self.current),
            exported: self.current_exported,
        });
        self.current = checkpoint.image;
        self.current_exported = checkpoint.exported;
        self.version = self.version.next();
        self.trim_history();
        true
    }

    pub(crate) fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub(crate) fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub(crate) fn undo_steps(&self) -> usize {
        self.undo.len()
    }

    pub(crate) fn redo_steps(&self) -> usize {
        self.redo.len()
    }

    pub(crate) fn has_transient(&self) -> bool {
        self.transient.is_some()
    }

    pub(crate) fn transient_kind(&self) -> Option<EditKind> {
        self.transient.as_ref().map(|preview| preview.kind)
    }

    pub(crate) fn is_dirty(&self) -> bool {
        !self.current_exported
    }

    pub(crate) fn mark_exported(&mut self, version: DocumentVersion) -> bool {
        if version != self.version {
            return false;
        }
        self.current_exported = true;
        true
    }

    fn push_undo(&mut self, checkpoint: Checkpoint) {
        self.undo.push_back(checkpoint);
        self.trim_history();
    }

    fn commit_arc(&mut self, image: Arc<RgbaImage>, kind: EditKind) {
        let previous = Checkpoint {
            image: Arc::clone(&self.current),
            kind,
            bytes: image_bytes(&self.current),
            exported: self.current_exported,
        };
        self.current = image;
        self.current_exported = false;
        self.version = self.version.next();
        self.redo.clear();
        self.push_undo(previous);
    }

    fn trim_history(&mut self) {
        self.recalculate_history_bytes();
        while self.history_bytes > self.history_budget_bytes {
            if self.undo.pop_front().is_none() && !self.redo.is_empty() {
                self.redo.remove(0);
            }
            self.recalculate_history_bytes();
            if self.undo.is_empty() && self.redo.is_empty() {
                break;
            }
        }
    }

    fn recalculate_history_bytes(&mut self) {
        self.history_bytes = self
            .undo
            .iter()
            .chain(self.redo.iter())
            .map(|checkpoint| checkpoint.bytes)
            .sum();
    }
}

fn image_bytes(image: &RgbaImage) -> usize {
    image.as_raw().len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;
    use proptest::prelude::*;

    fn image(value: u8) -> RgbaImage {
        RgbaImage::from_pixel(2, 2, Rgba([value, value, value, 255]))
    }

    #[test]
    fn preview_is_version_bound_and_one_apply_adds_one_history_entry() {
        let mut document = CurrentDocument::new(image(1));
        let base = document.snapshot();

        assert!(document.set_transient(base.version, image(2), EditKind::Beautify));
        assert_eq!(document.snapshot().image.get_pixel(0, 0).0[0], 1);
        assert_eq!(document.displayed_image().get_pixel(0, 0).0[0], 2);
        assert_eq!(document.undo_steps(), 0);

        assert!(document.apply_transient());
        assert_eq!(document.snapshot().image.get_pixel(0, 0).0[0], 2);
        assert_eq!(document.undo_steps(), 1);
        assert!(!document.set_transient(base.version, image(3), EditKind::Resize));
    }

    #[test]
    fn separate_documents_never_share_a_version_identity() {
        let first = CurrentDocument::new(image(1));
        let second = CurrentDocument::new(image(1));

        assert_ne!(first.snapshot().version, second.snapshot().version);
    }

    #[test]
    fn crop_preview_keeps_the_overlay_canvas_and_commits_exact_preview_pixels() {
        let mut document = CurrentDocument::new(image(1));
        let base = document.snapshot();
        let cropped = RgbaImage::from_pixel(1, 2, Rgba([7, 7, 7, 255]));

        assert!(document.set_transient(base.version, cropped.clone(), EditKind::Crop));
        assert_eq!(document.displayed_image().dimensions(), (2, 2));
        assert!(document.apply_transient());
        assert_eq!(document.snapshot().image.as_ref(), &cropped);
        assert_eq!(document.undo_steps(), 1);
    }

    #[test]
    fn tool_specific_apply_never_commits_another_tools_preview() {
        let mut document = CurrentDocument::new(image(1));
        let version = document.snapshot().version;
        assert!(document.set_transient(version, image(2), EditKind::Crop));

        assert!(!document.apply_transient_kind(EditKind::Resize));
        assert_eq!(document.snapshot().image.get_pixel(0, 0).0[0], 1);
        assert!(document.apply_transient());
        assert_eq!(document.snapshot().image.get_pixel(0, 0).0[0], 2);
    }

    #[test]
    fn undo_and_redo_restore_exact_pixels_and_dirty_state() {
        let mut document = CurrentDocument::new(image(1));
        let version = document.snapshot().version;
        assert!(document.commit(version, image(2), EditKind::Rotate));
        assert!(document.is_dirty());
        assert!(document.undo());
        assert_eq!(document.snapshot().image.get_pixel(0, 0).0[0], 1);
        assert!(!document.is_dirty());
        assert!(document.redo());
        assert_eq!(document.snapshot().image.get_pixel(0, 0).0[0], 2);
        assert!(document.is_dirty());
        assert!(document.mark_exported(document.snapshot().version));
        assert!(!document.is_dirty());
        assert!(document.undo());
        assert!(!document.is_dirty());
        assert!(document.redo());
        assert!(!document.is_dirty());
    }

    #[test]
    fn history_budget_evicts_oldest_checkpoints() {
        let bytes_per_image = image_bytes(&image(1));
        let mut document = CurrentDocument::with_history_budget(image(0), bytes_per_image * 2);
        for value in 1..=4 {
            let version = document.snapshot().version;
            assert!(document.commit(version, image(value), EditKind::Rotate));
        }
        assert_eq!(document.undo_steps(), 2);
        assert!(document.undo());
        assert!(document.undo());
        assert!(!document.undo());
        assert_eq!(document.snapshot().image.get_pixel(0, 0).0[0], 2);
    }

    #[test]
    fn undo_drops_an_oversized_redo_checkpoint_to_keep_the_history_budget() {
        let mut document = CurrentDocument::with_history_budget(image(0), 64);
        let large = RgbaImage::from_pixel(16, 16, Rgba([1, 1, 1, 255]));
        let version = document.snapshot().version;
        assert!(document.commit(version, large, EditKind::Resize));

        assert!(document.undo());
        assert!(!document.can_redo());
        assert_eq!(document.history_bytes, 0);
    }

    #[test]
    fn input_set_has_independent_versioned_order() {
        let mut document = CurrentDocument::new(image(9));
        let document_version = document.snapshot().version;
        let mut inputs = InputSet::default();
        inputs.replace(vec![image(1), image(2)], vec!["a".into(), "b".into()]);
        let first_version = inputs.version();
        assert!(inputs.move_item(0, 1));
        assert_ne!(inputs.version(), first_version);
        assert_eq!(inputs.names().as_ref(), &["b".to_string(), "a".to_string()]);
        assert_eq!(document.snapshot().version, document_version);
        assert!(document.commit(document_version, image(10), EditKind::Rotate));
        assert_eq!(inputs.images()[0].get_pixel(0, 0).0[0], 2);

        let moved_version = inputs.version();
        inputs.append(vec![image(3)], vec!["c".into()]);
        assert_ne!(inputs.version(), moved_version);
        assert_eq!(
            inputs.names().as_ref(),
            &["b".to_string(), "a".to_string(), "c".to_string()]
        );
    }

    proptest! {
        #[test]
        fn arbitrary_edit_sequences_round_trip_pixels_and_export_state(
            values in proptest::collection::vec(any::<u8>(), 1..24),
            export_final in any::<bool>(),
        ) {
            let mut document = CurrentDocument::with_history_budget(
                image(0),
                DEFAULT_HISTORY_BUDGET_BYTES,
            );
            for value in &values {
                let version = document.snapshot().version;
                prop_assert!(document.commit(version, image(*value), EditKind::Rotate));
            }
            if export_final {
                prop_assert!(document.mark_exported(document.snapshot().version));
            }
            let final_value = *values.last().expect("non-empty generated edit sequence");

            while document.undo() {}
            prop_assert_eq!(document.snapshot().image.get_pixel(0, 0).0[0], 0);
            prop_assert!(!document.is_dirty());

            while document.redo() {}
            prop_assert_eq!(document.snapshot().image.get_pixel(0, 0).0[0], final_value);
            prop_assert_eq!(document.is_dirty(), !export_final);
        }
    }
}
