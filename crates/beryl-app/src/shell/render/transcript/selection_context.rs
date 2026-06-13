use std::{cell::Cell, cell::RefCell, collections::HashMap, ops::Range, rc::Rc, sync::Arc};

use gpui::Entity;

use beryl_model::conversation::ConversationThreadId;

use crate::shell::execution_detail::TranscriptImagePreviewState;
use crate::shell::transcript_markdown::BlockRenderCode;
use crate::shell::transcript_selection::{
    TranscriptLineCopyGroup, TranscriptLineCopyText, TranscriptTextLineKey,
    TranscriptTextLineOrder, transcript_context_line_break_before,
};

use super::super::code_panel::{CodePanelSelectableLine, CodePanelSelection, SelectedTextStyle};
use super::TranscriptPanel;
use super::markdown_copy::code_block_copy_group;

#[derive(Clone)]
pub(super) struct TranscriptInlineSelectionContext {
    entity: Entity<TranscriptPanel>,
    row_identity: String,
    block_path: String,
    line_prefix: String,
    selection_render: Option<TranscriptTextSelectionRenderState>,
    next_order: Rc<Cell<TranscriptTextLineOrder>>,
    next_line_index: Rc<Cell<usize>>,
    next_break_before: Rc<Cell<usize>>,
    pending_start_prefix: Rc<RefCell<Option<String>>>,
    copy_group: Option<TranscriptLineCopyGroup>,
    viewport_local_scope: Option<String>,
}

#[derive(Clone)]
pub(super) struct TranscriptTextSelectionRenderState {
    selected_ranges: Rc<HashMap<TranscriptTextLineKey, Range<usize>>>,
    style: SelectedTextStyle,
}

impl TranscriptTextSelectionRenderState {
    pub(super) fn new(
        selected_ranges: HashMap<TranscriptTextLineKey, Range<usize>>,
        style: SelectedTextStyle,
    ) -> Self {
        Self {
            selected_ranges: Rc::new(selected_ranges),
            style,
        }
    }

    fn selected_text_for_key(
        &self,
        key: &TranscriptTextLineKey,
    ) -> Option<(Range<usize>, SelectedTextStyle)> {
        self.selected_ranges
            .get(key)
            .filter(|range| range.start < range.end)
            .cloned()
            .map(|range| (range, self.style))
    }
}

impl TranscriptInlineSelectionContext {
    pub(super) fn new_with_initial_break_before(
        entity: Entity<TranscriptPanel>,
        row_identity: impl Into<String>,
        block_path: impl Into<String>,
        next_order: Rc<Cell<TranscriptTextLineOrder>>,
        initial_break_before: usize,
        selection_render: Option<TranscriptTextSelectionRenderState>,
    ) -> Self {
        Self {
            entity,
            row_identity: row_identity.into(),
            block_path: block_path.into(),
            line_prefix: String::new(),
            selection_render,
            next_order,
            next_line_index: Rc::new(Cell::new(0)),
            next_break_before: Rc::new(Cell::new(initial_break_before)),
            pending_start_prefix: Rc::new(RefCell::new(None)),
            copy_group: None,
            viewport_local_scope: None,
        }
    }

    pub(super) fn with_viewport_local_scope(mut self, scope: Option<String>) -> Self {
        self.viewport_local_scope = scope;
        self
    }

    pub(super) fn with_pending_prefix(&self, prefix: impl Into<String>) -> Self {
        Self {
            entity: self.entity.clone(),
            row_identity: self.row_identity.clone(),
            block_path: self.block_path.clone(),
            line_prefix: self.line_prefix.clone(),
            selection_render: self.selection_render.clone(),
            next_order: self.next_order.clone(),
            next_line_index: self.next_line_index.clone(),
            next_break_before: self.next_break_before.clone(),
            pending_start_prefix: Rc::new(RefCell::new(Some(prefix.into()))),
            copy_group: self.copy_group.clone(),
            viewport_local_scope: self.viewport_local_scope.clone(),
        }
    }

    pub(super) fn with_line_prefix(&self, prefix: impl AsRef<str>) -> Self {
        Self {
            entity: self.entity.clone(),
            row_identity: self.row_identity.clone(),
            block_path: self.block_path.clone(),
            line_prefix: format!("{}{}", self.line_prefix, prefix.as_ref()),
            selection_render: self.selection_render.clone(),
            next_order: self.next_order.clone(),
            next_line_index: self.next_line_index.clone(),
            next_break_before: self.next_break_before.clone(),
            pending_start_prefix: Rc::new(RefCell::new(None)),
            copy_group: self.copy_group.clone(),
            viewport_local_scope: self.viewport_local_scope.clone(),
        }
    }

    pub(super) fn with_copy_group(&self, copy_group: TranscriptLineCopyGroup) -> Self {
        Self {
            entity: self.entity.clone(),
            row_identity: self.row_identity.clone(),
            block_path: self.block_path.clone(),
            line_prefix: self.line_prefix.clone(),
            selection_render: self.selection_render.clone(),
            next_order: self.next_order.clone(),
            next_line_index: self.next_line_index.clone(),
            next_break_before: self.next_break_before.clone(),
            pending_start_prefix: self.pending_start_prefix.clone(),
            copy_group: Some(copy_group),
            viewport_local_scope: self.viewport_local_scope.clone(),
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

    fn reserve_line_indices(&self, line_count: usize) -> usize {
        let line_base = self.next_line_index.get();
        self.next_line_index
            .set(line_base.saturating_add(line_count.max(1)));
        line_base
    }

    pub(super) fn code_panel_selection(
        &self,
        structural_path: &str,
        code: &BlockRenderCode,
    ) -> CodePanelSelection {
        let context = self.clone();
        let range_context = self.clone();
        let copy_group =
            code_block_copy_group(format!("{}:code:{structural_path}", self.block_path), code);
        let reserved_line_base = Rc::new(Cell::new(None::<usize>));
        let range_line_base = reserved_line_base.clone();
        CodePanelSelection {
            line_prepaint_action: Arc::new(move |line: CodePanelSelectableLine| {
                let line_base = context
                    .reserve_code_panel_line_base(&reserved_line_base, line.display_line_count);
                let line_index = line_base.saturating_add(line.display_line_index);
                let copy_text = TranscriptLineCopyText::plain(line.raw_text.clone())
                    .with_group(copy_group.clone());
                let selectable_line = context.selectable_line_with_line_index_and_break_before(
                    line_index,
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
            selected_text_style: self.selection_render.as_ref().map(|state| state.style),
            selected_range_for_line: Arc::new(move |line: &CodePanelSelectableLine| {
                let line_base = range_context
                    .reserve_code_panel_line_base(&range_line_base, line.display_line_count);
                let line_index = line_base.saturating_add(line.display_line_index);
                let key = range_context.text_line_key(line_index);
                range_context
                    .selected_text_for_key(&key)
                    .map(|(range, _)| range)
            }),
        }
    }

    pub(super) fn selectable_line(
        &self,
        display_text: String,
        display_text_len: usize,
        copy_text: TranscriptLineCopyText,
    ) -> TranscriptSelectableTextLine {
        self.selectable_line_inner(None, display_text, display_text_len, copy_text, None)
    }

    fn selectable_line_with_line_index_and_break_before(
        &self,
        line_index: usize,
        display_text: String,
        display_text_len: usize,
        copy_text: TranscriptLineCopyText,
        break_before: usize,
    ) -> TranscriptSelectableTextLine {
        self.selectable_line_inner(
            Some(line_index),
            display_text,
            display_text_len,
            copy_text,
            Some(break_before),
        )
    }

    fn selectable_line_inner(
        &self,
        explicit_line_index: Option<usize>,
        display_text: String,
        display_text_len: usize,
        copy_text: TranscriptLineCopyText,
        explicit_break_before: Option<usize>,
    ) -> TranscriptSelectableTextLine {
        let line_index = explicit_line_index.unwrap_or_else(|| {
            let line_index = self.next_line_index.get();
            self.next_line_index.set(line_index.saturating_add(1));
            line_index
        });
        let order = self.next_order.get();
        self.next_order.set(order.next_line());
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
            key: self.text_line_key(line_index),
            order,
            display_text,
            copy_text,
            break_before,
            display_text_len,
            image_markers: Vec::new(),
            thread_links: Vec::new(),
        }
    }

    pub(super) fn selected_text_for_line(
        &self,
        line: &TranscriptSelectableTextLine,
    ) -> Option<(Range<usize>, SelectedTextStyle)> {
        self.selected_text_for_key(&line.key)
    }

    fn selected_text_for_key(
        &self,
        key: &TranscriptTextLineKey,
    ) -> Option<(Range<usize>, SelectedTextStyle)> {
        self.selection_render
            .as_ref()
            .and_then(|selection| selection.selected_text_for_key(key))
    }

    fn reserve_code_panel_line_base(
        &self,
        reserved_line_base: &Rc<Cell<Option<usize>>>,
        display_line_count: usize,
    ) -> usize {
        reserved_line_base.get().unwrap_or_else(|| {
            let line_base = self.reserve_line_indices(display_line_count);
            reserved_line_base.set(Some(line_base));
            line_base
        })
    }

    fn text_line_key(&self, line_index: usize) -> TranscriptTextLineKey {
        let key = TranscriptTextLineKey::new(
            self.row_identity.clone(),
            self.block_path.clone(),
            line_index,
        );
        if let Some(scope) = self.viewport_local_scope.as_ref() {
            key.with_viewport_local_scope(scope.clone())
        } else {
            key
        }
    }
}

#[derive(Clone)]
pub(super) struct TranscriptSelectableTextLine {
    pub(super) entity: Entity<TranscriptPanel>,
    pub(super) key: TranscriptTextLineKey,
    pub(super) order: TranscriptTextLineOrder,
    pub(super) display_text: String,
    pub(super) copy_text: TranscriptLineCopyText,
    pub(super) break_before: usize,
    pub(super) display_text_len: usize,
    pub(super) image_markers: Vec<TranscriptSelectableImageMarker>,
    pub(super) thread_links: Vec<TranscriptSelectableThreadLink>,
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct TranscriptSelectableThreadLink {
    pub(super) thread_id: ConversationThreadId,
    pub(super) display_range: Range<usize>,
}

impl TranscriptSelectableTextLine {
    pub(super) fn with_image_markers(
        mut self,
        image_markers: Vec<TranscriptSelectableImageMarker>,
    ) -> Self {
        self.image_markers = image_markers;
        self
    }

    pub(super) fn with_thread_links(
        mut self,
        thread_links: Vec<TranscriptSelectableThreadLink>,
    ) -> Self {
        self.thread_links = thread_links;
        self
    }
}
