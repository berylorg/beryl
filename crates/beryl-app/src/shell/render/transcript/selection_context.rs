use std::{cell::Cell, cell::RefCell, ops::Range, rc::Rc, sync::Arc};

use gpui::Entity;

use crate::shell::execution_detail::TranscriptImagePreviewState;
use crate::shell::transcript_markdown::BlockRenderCode;
use crate::shell::transcript_selection::{
    TranscriptLineCopyGroup, TranscriptLineCopyText, TranscriptTextLineKey,
    transcript_context_line_break_before,
};

use super::super::code_panel::{CodePanelSelectableLine, CodePanelSelection};
use super::TranscriptPanel;
use super::markdown_copy::code_block_copy_group;

#[derive(Clone)]
pub(super) struct TranscriptInlineSelectionContext {
    entity: Entity<TranscriptPanel>,
    row_identity: String,
    block_path: String,
    line_prefix: String,
    next_order: Rc<Cell<usize>>,
    next_line_index: Rc<Cell<usize>>,
    next_break_before: Rc<Cell<usize>>,
    pending_start_prefix: Rc<RefCell<Option<String>>>,
    copy_group: Option<TranscriptLineCopyGroup>,
}

impl TranscriptInlineSelectionContext {
    pub(super) fn new_with_initial_break_before(
        entity: Entity<TranscriptPanel>,
        row_identity: impl Into<String>,
        block_path: impl Into<String>,
        next_order: Rc<Cell<usize>>,
        initial_break_before: usize,
    ) -> Self {
        Self {
            entity,
            row_identity: row_identity.into(),
            block_path: block_path.into(),
            line_prefix: String::new(),
            next_order,
            next_line_index: Rc::new(Cell::new(0)),
            next_break_before: Rc::new(Cell::new(initial_break_before)),
            pending_start_prefix: Rc::new(RefCell::new(None)),
            copy_group: None,
        }
    }

    pub(super) fn with_pending_prefix(&self, prefix: impl Into<String>) -> Self {
        Self {
            entity: self.entity.clone(),
            row_identity: self.row_identity.clone(),
            block_path: self.block_path.clone(),
            line_prefix: self.line_prefix.clone(),
            next_order: self.next_order.clone(),
            next_line_index: self.next_line_index.clone(),
            next_break_before: self.next_break_before.clone(),
            pending_start_prefix: Rc::new(RefCell::new(Some(prefix.into()))),
            copy_group: self.copy_group.clone(),
        }
    }

    pub(super) fn with_line_prefix(&self, prefix: impl AsRef<str>) -> Self {
        Self {
            entity: self.entity.clone(),
            row_identity: self.row_identity.clone(),
            block_path: self.block_path.clone(),
            line_prefix: format!("{}{}", self.line_prefix, prefix.as_ref()),
            next_order: self.next_order.clone(),
            next_line_index: self.next_line_index.clone(),
            next_break_before: self.next_break_before.clone(),
            pending_start_prefix: Rc::new(RefCell::new(None)),
            copy_group: self.copy_group.clone(),
        }
    }

    pub(super) fn with_copy_group(&self, copy_group: TranscriptLineCopyGroup) -> Self {
        Self {
            entity: self.entity.clone(),
            row_identity: self.row_identity.clone(),
            block_path: self.block_path.clone(),
            line_prefix: self.line_prefix.clone(),
            next_order: self.next_order.clone(),
            next_line_index: self.next_line_index.clone(),
            next_break_before: self.next_break_before.clone(),
            pending_start_prefix: self.pending_start_prefix.clone(),
            copy_group: Some(copy_group),
        }
    }

    pub(super) fn with_code_copy_group(
        &self,
        structural_path: &str,
        code: &BlockRenderCode,
    ) -> Self {
        self.with_copy_group(code_block_copy_group(
            format!("{}:code:{structural_path}", self.block_path),
            code,
        ))
    }

    pub(super) fn set_next_break_before(&self, break_before: usize) {
        self.next_break_before.set(break_before);
    }

    pub(super) fn code_panel_selection(
        &self,
        structural_path: &str,
        code: &BlockRenderCode,
    ) -> CodePanelSelection {
        let context = self.clone();
        let copy_group =
            code_block_copy_group(format!("{}:code:{structural_path}", self.block_path), code);
        CodePanelSelection {
            line_prepaint_action: Arc::new(move |line: CodePanelSelectableLine| {
                let copy_text = TranscriptLineCopyText::plain(line.raw_text.clone())
                    .with_group(copy_group.clone());
                let selectable_line = context.selectable_line_with_break_before(
                    line.raw_text,
                    line.display_text_len,
                    copy_text,
                    line.break_before,
                );
                Arc::new(move |bounds, layout, cx| {
                    selectable_line.entity.update(cx, |view, _| {
                        view.register_selectable_text_line(selectable_line.clone(), bounds, layout);
                    });
                })
            }),
        }
    }

    pub(super) fn selectable_line(
        &self,
        display_text: String,
        display_text_len: usize,
        copy_text: TranscriptLineCopyText,
    ) -> TranscriptSelectableTextLine {
        self.selectable_line_inner(display_text, display_text_len, copy_text, None)
    }

    fn selectable_line_with_break_before(
        &self,
        display_text: String,
        display_text_len: usize,
        copy_text: TranscriptLineCopyText,
        break_before: usize,
    ) -> TranscriptSelectableTextLine {
        self.selectable_line_inner(
            display_text,
            display_text_len,
            copy_text,
            Some(break_before),
        )
    }

    fn selectable_line_inner(
        &self,
        display_text: String,
        display_text_len: usize,
        copy_text: TranscriptLineCopyText,
        explicit_break_before: Option<usize>,
    ) -> TranscriptSelectableTextLine {
        let line_index = self.next_line_index.get();
        self.next_line_index.set(line_index.saturating_add(1));
        let order = self.next_order.get();
        self.next_order.set(order.saturating_add(1));
        let start_prefix = self
            .pending_start_prefix
            .borrow_mut()
            .take()
            .unwrap_or_default();
        let mut copy_text = copy_text.with_prefixes(self.line_prefix.clone(), start_prefix);
        if let Some(copy_group) = &self.copy_group {
            copy_text = copy_text.with_group(copy_group.clone());
        }
        let context_break_before = self.next_break_before.replace(1);
        let break_before = transcript_context_line_break_before(
            line_index,
            context_break_before,
            explicit_break_before,
        );

        TranscriptSelectableTextLine {
            entity: self.entity.clone(),
            key: TranscriptTextLineKey::new(
                self.row_identity.clone(),
                self.block_path.clone(),
                line_index,
            ),
            order,
            display_text,
            copy_text,
            break_before,
            display_text_len,
            image_markers: Vec::new(),
        }
    }
}

#[derive(Clone)]
pub(super) struct TranscriptSelectableTextLine {
    pub(super) entity: Entity<TranscriptPanel>,
    pub(super) key: TranscriptTextLineKey,
    pub(super) order: usize,
    pub(super) display_text: String,
    pub(super) copy_text: TranscriptLineCopyText,
    pub(super) break_before: usize,
    pub(super) display_text_len: usize,
    pub(super) image_markers: Vec<TranscriptSelectableImageMarker>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct TranscriptSelectableImageMarker {
    pub(super) occurrence_id: String,
    pub(super) label: String,
    pub(super) display_text: String,
    pub(super) display_range: Range<usize>,
    pub(super) copy_text: String,
    pub(super) asset_id: Option<String>,
    pub(super) preview_state: TranscriptImagePreviewState,
}

impl TranscriptSelectableTextLine {
    pub(super) fn with_image_markers(
        mut self,
        image_markers: Vec<TranscriptSelectableImageMarker>,
    ) -> Self {
        self.image_markers = image_markers;
        self
    }
}
