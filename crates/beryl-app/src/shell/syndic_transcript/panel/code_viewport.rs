use std::{cell::RefCell, rc::Rc};

use super::super::provider::{IndexedResourceKind, resource_index as index};
use gpui::{Bounds, Pixels, px};

use super::super::{
    IndexedResourceDemand, IndexedResourceQuery, IndexedResourceValue, ResourceTranscriptDemand,
};

pub(super) const LINE_HEIGHT: f32 = 20.0;
pub(super) const VIEWPORT_HEIGHT: f32 = 240.0;
pub(super) const BODY_PADDING: f32 = 8.0;
pub(super) const FEEDBACK_HEIGHT: f32 = 29.0;
pub(super) const OVERSCAN: u64 = 2;
const REALIZED_LINES: u64 = 16;

pub(super) struct CodeScroll {
    pub(super) bounds: Bounds<Pixels>,
    pub(super) top_line: u64,
    pub(super) remainder: f32,
    pub(super) horizontal: f32,
    pub(super) lines: u64,
    pub(super) line_height: f32,
}

impl Default for CodeScroll {
    fn default() -> Self {
        Self {
            bounds: Bounds::default(),
            top_line: 0,
            remainder: 0.0,
            horizontal: 0.0,
            lines: 0,
            line_height: LINE_HEIGHT,
        }
    }
}

impl CodeScroll {
    pub(super) fn set_lines(&mut self, lines: u64) {
        if lines != self.lines {
            self.lines = lines;
            self.set_offset_exact(
                self.top_line as f64 * self.line_height as f64 + self.remainder as f64,
            );
        }
        if lines == 0 {
            self.horizontal = 0.0;
        }
    }

    pub(super) fn set_line_height(&mut self, height: f32) {
        if height.is_finite() && height >= LINE_HEIGHT && height != self.line_height {
            self.line_height = height;
            self.set_offset_exact(self.top_line as f64 * height as f64 + self.remainder as f64);
        }
    }
    pub(super) fn offset(&self) -> f32 {
        (self.top_line as f64 * self.line_height as f64 + self.remainder as f64) as f32
    }

    pub(super) fn overflow(&self) -> f32 {
        ((self.lines as f64 * self.line_height as f64 - VIEWPORT_HEIGHT as f64).max(0.0)) as f32
    }

    pub(super) fn set_offset(&mut self, offset: f32) {
        if !offset.is_finite() {
            return;
        }
        self.set_offset_exact(offset as f64);
    }

    pub(super) fn scroll_by(&mut self, delta: f32) {
        if delta.is_finite() {
            self.set_offset_exact(
                self.top_line as f64 * self.line_height as f64
                    + self.remainder as f64
                    + delta as f64,
            );
        }
    }

    pub(super) fn scroll_horizontal_by(&mut self, delta: f32) {
        let offset = self.horizontal + delta;
        if delta.is_finite() && offset.is_finite() {
            self.horizontal = offset.max(0.0);
        }
    }

    fn set_offset_exact(&mut self, offset: f64) {
        let maximum =
            (self.lines as f64 * self.line_height as f64 - VIEWPORT_HEIGHT as f64).max(0.0);
        let offset = offset.clamp(0.0, maximum);
        self.top_line = (offset / self.line_height as f64).floor() as u64;
        self.remainder = (offset - self.top_line as f64 * self.line_height as f64) as f32;
    }

    pub(super) fn range(&self) -> index::OrdinalRange {
        let start = self.top_line.saturating_sub(OVERSCAN);
        let end = start.saturating_add(REALIZED_LINES);
        index::OrdinalRange {
            start,
            end: if self.lines == 0 {
                end
            } else {
                end.min(self.lines)
            },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct CodeInlineAnchor {
    pub(super) source: index::SourceAnchor,
    pub(super) logical_line: u64,
    pub(super) visual_offset: f32,
}

pub(super) struct CodeViewport {
    pub(super) owner: ResourceTranscriptDemand,
    pub(super) scroll: Rc<RefCell<CodeScroll>>,
    pub(super) anchor: Option<index::SourceAnchor>,
    pub(super) root: Option<[u8; 32]>,
    pub(super) revision: beryl_model::ProjectionRevision,
    pub(super) remapping: bool,
    anchor_visual_residual: f32,
    anchor_line: Option<u64>,
    resident_anchors: Vec<(u64, index::SourceAnchor)>,
    source: Option<(syndic_storage::ProjectionTextSource, u64)>,
    pub(super) source_continuity: &'static str,
    pub(super) anchor_remap_result: &'static str,
    continuation: Option<(index::Request, Option<index::Cursor>)>,
    pub(super) layout_query: Option<IndexedResourceDemand>,
    pub(super) inline_anchor: Option<CodeInlineAnchor>,
    pub(super) inline_remapping: bool,
    inline_remap_target: f32,
    inline_remap_block: (u64, f32),
    inline_anchor_scroll: Option<(u64, f32)>,
    inline_anchor_horizontal: f32,
}

impl CodeViewport {
    pub(super) fn new(
        owner: ResourceTranscriptDemand,
        revision: beryl_model::ProjectionRevision,
    ) -> Self {
        Self {
            owner,
            revision,
            scroll: Rc::default(),
            anchor: None,
            root: None,
            remapping: false,
            anchor_visual_residual: 0.0,
            anchor_line: None,
            resident_anchors: Vec::new(),
            source: None,
            source_continuity: "unknown",
            anchor_remap_result: "none",
            continuation: None,
            layout_query: None,
            inline_anchor: None,
            inline_remapping: false,
            inline_remap_target: 0.0,
            inline_remap_block: (0, 0.0),
            inline_anchor_scroll: None,
            inline_anchor_horizontal: 0.0,
        }
    }

    pub(super) fn query(&self) -> IndexedResourceDemand {
        if !self.remapping
            && let Some(query) = &self.layout_query
        {
            return query.clone();
        }
        IndexedResourceDemand {
            revision: self.revision,
            root: self.root,
            query: if self.remapping && self.anchor.is_some() {
                IndexedResourceQuery::Anchor {
                    anchor: self
                        .inline_anchor
                        .filter(|inline| self.anchor_line == Some(inline.logical_line))
                        .map_or(self.anchor.unwrap(), |inline| inline.source),
                    bytes: 8192,
                }
            } else {
                IndexedResourceQuery::Range {
                    request: index::Request::Code {
                        lines: self.scroll.borrow().range(),
                    },
                    cursor: self.continuation.and_then(|(request, cursor)| {
                        (request
                            == index::Request::Code {
                                lines: self.scroll.borrow().range(),
                            })
                        .then_some(cursor)
                        .flatten()
                    }),
                    descriptors: REALIZED_LINES as usize,
                    bytes: 65536,
                }
            },
        }
    }

    pub(super) fn restart_range(&mut self) -> IndexedResourceDemand {
        self.continuation = None;
        self.layout_query = None;
        self.query()
    }

    pub(super) fn anchor_visual_offset(&self) -> Option<f32> {
        self.anchor
            .map(|anchor| anchor.visual_offset as f32 + self.anchor_visual_residual)
    }

    pub(super) fn retain_inline_anchor(&mut self, anchor: CodeInlineAnchor) {
        let scroll = self.scroll.borrow();
        let range = scroll.range();
        if self.inline_remapping
            || anchor.logical_line < range.start
            || anchor.logical_line >= range.end
        {
            return;
        }
        self.inline_anchor_scroll = Some((scroll.top_line, scroll.remainder));
        self.inline_anchor_horizontal = scroll.horizontal;
        drop(scroll);
        self.inline_anchor = Some(anchor);
        self.capture_resident_anchor();
    }

    pub(super) fn inline_anchor_is_current(&self) -> bool {
        let scroll = self.scroll.borrow();
        let range = scroll.range();
        self.inline_anchor_scroll == Some((scroll.top_line, scroll.remainder))
            && self.inline_anchor_horizontal == scroll.horizontal
            && self.inline_anchor.is_some_and(|anchor| {
                range.start <= anchor.logical_line && anchor.logical_line < range.end
            })
    }

    pub(super) fn begin_inline_remap(&mut self) {
        self.capture_resident_anchor();
        let scroll = self.scroll.borrow();
        let range = scroll.range();
        self.inline_remapping = self.inline_anchor.is_some_and(|anchor| {
            range.start <= anchor.logical_line && anchor.logical_line < range.end
        });
        self.inline_remap_target = scroll.horizontal;
        self.inline_remap_block = (scroll.top_line, scroll.remainder);
    }

    pub(super) fn set_line_height(&mut self, height: f32) {
        let mut scroll = self.scroll.borrow_mut();
        let before = (scroll.top_line, scroll.remainder);
        scroll.set_line_height(height);
        let after = (scroll.top_line, scroll.remainder);
        if self.inline_remapping {
            self.inline_remap_block = after;
        }
        if self.inline_anchor_scroll == Some(before) {
            self.inline_anchor_scroll = Some(after);
        }
    }

    pub(super) fn pending_inline_anchor(&mut self) -> Option<CodeInlineAnchor> {
        let scroll = self.scroll.borrow();
        let range = scroll.range();
        if self.inline_remapping
            && (self.inline_remap_target != scroll.horizontal
                || self.inline_remap_block != (scroll.top_line, scroll.remainder)
                || self.inline_anchor.is_none_or(|anchor| {
                    anchor.logical_line < range.start || anchor.logical_line >= range.end
                }))
        {
            self.inline_remapping = false;
        }
        self.inline_remapping
            .then_some(self.inline_anchor)
            .flatten()
    }

    pub(super) fn resolve_inline_anchor(&mut self, anchor: CodeInlineAnchor, inline: f32) -> bool {
        if self.pending_inline_anchor() != Some(anchor) || self.remapping {
            return false;
        }
        let horizontal = inline - anchor.visual_offset;
        if !horizontal.is_finite() {
            return false;
        }
        self.scroll.borrow_mut().horizontal = horizontal.max(0.0);
        self.inline_remapping = false;
        true
    }

    pub(super) fn rebind(
        &mut self,
        owner: ResourceTranscriptDemand,
        revision: beryl_model::ProjectionRevision,
    ) {
        self.capture_resident_anchor();
        let retained = self.inline_anchor_scroll
            == Some((
                self.scroll.borrow().top_line,
                self.scroll.borrow().remainder,
            ));
        if self.anchor_line != Some(self.scroll.borrow().top_line) && !retained {
            // Compatible sources preserve their prefix; fetch this line from the successor index.
            self.anchor = None;
            self.anchor_line = None;
            self.anchor_visual_residual = 0.0;
            self.inline_anchor = None;
        }
        if let Some(anchor) = self.anchor.as_mut() {
            let scroll = self.scroll.borrow();
            if let Some(line) = self.anchor_line {
                let visual = (line as i128 - scroll.top_line as i128) as f32 * scroll.line_height
                    - scroll.remainder;
                anchor.visual_offset = visual.round() as i64;
                self.anchor_visual_residual = visual - anchor.visual_offset as f32;
                if let Some(inline) = self.inline_anchor.as_mut() {
                    inline.source.visual_offset = anchor.visual_offset;
                }
            }
        }
        self.owner = owner;
        self.revision = revision;
        self.root = None;
        self.continuation = None;
        self.layout_query = None;
        self.resident_anchors.clear();
        self.remapping = self.anchor.is_some();
        self.begin_inline_remap();
        self.source_continuity = "pending";
        self.anchor_remap_result = if self.remapping { "pending" } else { "none" };
    }

    fn capture_resident_anchor(&mut self) {
        let scroll = self.scroll.borrow();
        if self.inline_anchor_scroll == Some((scroll.top_line, scroll.remainder))
            && self
                .inline_anchor
                .is_some_and(|inline| self.anchor_line == Some(inline.logical_line))
        {
            return;
        }
        if self
            .inline_anchor
            .is_some_and(|inline| inline.logical_line != scroll.top_line)
        {
            self.inline_anchor = None;
            self.inline_anchor_scroll = None;
        }
        if let Some((ordinal, anchor)) = self
            .resident_anchors
            .iter()
            .find(|(ordinal, _)| *ordinal == scroll.top_line)
        {
            let mut anchor = *anchor;
            anchor.visual_offset = -(scroll.remainder.round() as i64);
            self.anchor_visual_residual = -scroll.remainder - anchor.visual_offset as f32;
            self.anchor = Some(anchor);
            self.anchor_line = Some(*ordinal);
            if let Some(inline) = self.inline_anchor.as_mut() {
                inline.source.visual_offset = anchor.visual_offset;
            }
        }
    }

    pub(super) fn adopt(&mut self, response: &super::super::IndexedResourceResponse) {
        let Some(root) = &response.root else {
            return;
        };
        let binding = match &response.value {
            IndexedResourceValue::Range(value) => Some(&value.binding),
            IndexedResourceValue::Display(value) => Some(&value.binding),
            IndexedResourceValue::Anchor(value) => Some(&value.binding),
            IndexedResourceValue::Source(value) => Some(&value.binding),
            IndexedResourceValue::Pending => None,
        };
        if let Some(binding) = binding {
            if self.source.is_some_and(|(source, block_start)| {
                !source.can_extend(binding.source) || block_start != binding.range.start
            }) {
                let mut scroll = self.scroll.borrow_mut();
                scroll.set_offset(0.0);
                scroll.horizontal = 0.0;
                self.anchor = None;
                self.anchor_visual_residual = 0.0;
                self.anchor_line = None;
                self.inline_anchor = None;
                self.inline_remapping = false;
                self.inline_anchor_scroll = None;
                self.resident_anchors.clear();
                self.remapping = false;
                self.source_continuity = "changed";
                self.anchor_remap_result = "changed_source";
            } else if self.source.is_none() {
                self.source_continuity = "initial";
            } else if self.source_continuity == "pending" {
                self.source_continuity = "same";
            }
            self.source = Some((binding.source, binding.range.start));
        }
        self.root = Some(root.digest);
        self.scroll.borrow_mut().set_lines(root.code_lines);
        if root.code_lines == 0 {
            self.anchor = None;
            self.anchor_visual_residual = 0.0;
            self.anchor_line = None;
            self.inline_anchor = None;
            self.inline_remapping = false;
            self.inline_anchor_scroll = None;
            self.resident_anchors.clear();
            self.remapping = false;
            self.anchor_remap_result = "empty";
            return;
        }
        match &response.value {
            IndexedResourceValue::Anchor(value) if self.remapping => {
                let mut scroll = self.scroll.borrow_mut();
                if let Some(resolved) = &value.resolved {
                    if let index::Descriptor::CodeLine {
                        ordinal, content, ..
                    } = resolved.descriptor.descriptor
                    {
                        let line_height = scroll.line_height;
                        scroll.set_offset_exact(
                            ordinal as f64 * line_height as f64
                                - resolved.anchor.visual_offset as f64
                                - self.anchor_visual_residual as f64,
                        );
                        self.anchor = Some(index::SourceAnchor {
                            position: value.binding.range.start + content.start,
                            ..resolved.anchor
                        });
                        self.anchor_line = Some(ordinal);
                        if let Some(inline) = self.inline_anchor.as_mut() {
                            inline.logical_line = ordinal;
                            inline.source = resolved.anchor;
                            inline.source.position = resolved.anchor.position.clamp(
                                value.binding.range.start + content.start,
                                value.binding.range.start + content.end,
                            );
                            self.inline_remap_block = (scroll.top_line, scroll.remainder);
                            self.inline_anchor_scroll = Some(self.inline_remap_block);
                        }
                        self.anchor_remap_result = if scroll.top_line == ordinal
                            && (-scroll.remainder
                                - resolved.anchor.visual_offset as f32
                                - self.anchor_visual_residual)
                                .abs()
                                < 0.001
                        {
                            "resolved"
                        } else {
                            "clamped"
                        };
                    }
                } else {
                    scroll.set_offset(0.0);
                    scroll.horizontal = 0.0;
                    self.anchor = None;
                    self.anchor_visual_residual = 0.0;
                    self.anchor_line = None;
                    self.inline_anchor = None;
                    self.inline_remapping = false;
                    self.inline_anchor_scroll = None;
                    self.anchor_remap_result = "unresolved";
                }
                self.remapping = false;
            }
            IndexedResourceValue::Range(value) => {
                let cursor = self.owner.indexed.as_ref().and_then(|inner| {
                    let IndexedResourceQuery::Range { cursor, .. } = &inner.query else {
                        return None;
                    };
                    *cursor
                });
                self.continuation = Some((
                    value.cursor.request,
                    if !value.request_complete
                        && !value.items.is_empty()
                        && value.cursor.fragment_offset == 0
                    {
                        Some(value.cursor)
                    } else {
                        cursor
                    },
                ));
                self.resident_anchors.clear();
                for item in value.items.iter().take(REALIZED_LINES as usize) {
                    let index::Item::Descriptor(item) = item else {
                        continue;
                    };
                    if let index::Descriptor::CodeLine {
                        ordinal, content, ..
                    } = item.descriptor
                    {
                        self.resident_anchors.push((
                            ordinal,
                            index::SourceAnchor {
                                source: value.binding.source,
                                block_start: value.binding.range.start,
                                kind: IndexedResourceKind::Code,
                                position: value.binding.range.start + content.start,
                                visual_offset: 0,
                            },
                        ));
                    }
                }
                self.capture_resident_anchor();
            }
            _ => {}
        }
    }

    pub(super) fn adopt_pages(
        &mut self,
        pages: &[std::sync::Arc<super::super::IndexedResourceResponse>],
    ) {
        let Some(response) = pages.last() else {
            return;
        };
        self.adopt(response);
        if !matches!(response.value, IndexedResourceValue::Range(_)) {
            return;
        }
        self.resident_anchors.clear();
        for response in pages {
            let IndexedResourceValue::Range(value) = &response.value else {
                continue;
            };
            for item in &value.items {
                let index::Item::Descriptor(item) = item else {
                    continue;
                };
                let index::Descriptor::CodeLine {
                    ordinal, content, ..
                } = item.descriptor
                else {
                    continue;
                };
                if self.resident_anchors.len() >= REALIZED_LINES as usize {
                    break;
                }
                self.resident_anchors.push((
                    ordinal,
                    index::SourceAnchor {
                        source: value.binding.source,
                        block_start: value.binding.range.start,
                        kind: IndexedResourceKind::Code,
                        position: value.binding.range.start + content.start,
                        visual_offset: 0,
                    },
                ));
            }
        }
        self.capture_resident_anchor();
    }
}

pub(super) fn shell(header_height: f32) -> gpui::Div {
    use gpui::prelude::*;
    gpui::div()
        .w_full()
        .h(px(VIEWPORT_HEIGHT
            + BODY_PADDING * 2.0
            + header_height
            + FEEDBACK_HEIGHT
            + 2.0))
        .min_w(px(0.0))
        .overflow_hidden()
}
