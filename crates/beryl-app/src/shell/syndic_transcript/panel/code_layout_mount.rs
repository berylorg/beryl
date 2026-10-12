use std::{
    ops::Range,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

#[path = "code_layout_mount/extent.rs"]
mod extent;
use extent::ResourceExtent;

static NEXT_LAYOUT_EPOCH: AtomicU64 = AtomicU64::new(1);

use gpui::{
    Font, StreamingLayoutBinding, StreamingLayoutLimits, StreamingLayoutPosition, TextRun, Window,
    px,
};

use super::super::{
    IndexedResourceDemand, IndexedResourceQuery, IndexedResourceResponse, IndexedResourceValue,
    provider::resource_index as index,
};
use super::{
    code_appearance::CodeAppearance,
    code_layout::*,
    code_viewport::{CodeInlineAnchor, CodeViewport},
};

#[derive(Clone, PartialEq)]
struct Environment {
    font_environment: gpui::FontEnvironmentKey,
    font: Font,
    size: f32,
    line_height: f32,
    scale: f32,
}

struct Line {
    ordinal: u64,
    source: Range<u64>,
    index: CodeLayoutIndex,
    target: Option<(f32, f32)>,
    prepared: Option<PreparedCodeLayout>,
    candidate: Option<PreparedCodeLayout>,
    width: Option<f32>,
    scanning_extent: bool,
    anchor_request: Option<CodeLayoutAnchor>,
}

#[derive(Default)]
pub(super) struct CodeLayoutMount {
    binding: Option<index::Binding>,
    root: Option<[u8; 32]>,
    environment: Option<Environment>,
    lines: Vec<Line>,
    seeds: Vec<Arc<IndexedResourceResponse>>,
    sequence: u64,
    pub(super) query: Option<IndexedResourceDemand>,
    pub(super) refused: Option<CodeLayoutError>,
    pub(super) queued: bool,
    range: Option<index::OrdinalRange>,
    pub(super) epoch: u64,
    remaining_query: Option<IndexedResourceDemand>,
    requested_window: Option<(f32, f32)>,
    range_admitted: bool,
    resolved_inline: Option<(CodeInlineAnchor, f32)>,
    restored_inline: Option<(CodeInlineAnchor, f32)>,
    extent: Option<ResourceExtent>,
    total_lines: Option<u64>,
    range_restart: bool,
}

impl CodeLayoutMount {
    pub(super) fn reconcile_range(&mut self, viewport: &mut CodeViewport) {
        let scroll = viewport.scroll.borrow();
        let range = scroll.range();
        let target = (
            scroll.horizontal,
            scroll.horizontal + (f32::from(scroll.bounds.size.width) - 20.0).max(1.0),
        );
        drop(scroll);
        if self.requested_window != Some(target) {
            self.requested_window = Some(target);
            if self
                .query
                .as_ref()
                .is_some_and(|query| matches!(query.query, IndexedResourceQuery::Source { .. }))
                || viewport
                    .layout_query
                    .as_ref()
                    .is_some_and(|query| matches!(query.query, IndexedResourceQuery::Source { .. }))
            {
                self.query = None;
                viewport.layout_query = None;
            }
            if self.refused.take().is_some() {
                self.discard_candidate();
                self.query = None;
                viewport.layout_query = None;
                for line in &mut self.lines {
                    line.target = None;
                }
            }
        }
        if self.range != Some(range) {
            self.range = Some(range);
            self.query = None;
            self.refused = None;
            self.seeds.clear();
            self.remaining_query = None;
            self.range_admitted = false;
            viewport.layout_query = None;
        }
    }

    pub(super) fn reset(&mut self) {
        let queued = self.queued;
        *self = Self::default();
        self.queued = queued;
    }

    pub(super) fn source_binding(&self) -> Option<&index::Binding> {
        self.binding.as_ref()
    }
    pub(super) fn source_root(&self) -> Option<[u8; 32]> {
        self.root
    }

    pub(super) fn reconcile_environment(
        &mut self,
        viewport: &mut CodeViewport,
        appearance: &CodeAppearance,
        window: &Window,
        line_height: f32,
        font_environment: gpui::FontEnvironmentKey,
    ) -> Result<(), CodeLayoutError> {
        if window.text_system().font_environment_key() != Some(font_environment) {
            viewport.begin_inline_remap();
            self.reset();
            self.range_restart = true;
            viewport.layout_query = None;
            return Err(CodeLayoutError::FontEnvironmentChanged);
        }
        if matches!(
            self.refused,
            Some(
                CodeLayoutError::FontEnvironmentUnavailable
                    | CodeLayoutError::FontEnvironmentChanged
            )
        ) {
            self.refused = None;
        }
        let environment = Environment {
            font_environment,
            font: appearance.body_font.font.clone(),
            size: appearance.body_font.size,
            line_height,
            scale: window.scale_factor(),
        };
        if self
            .environment
            .as_ref()
            .is_some_and(|current| *current != environment)
        {
            viewport.begin_inline_remap();
            self.reset();
            self.range_restart = true;
            viewport.layout_query = None;
        }
        Ok(())
    }

    pub(super) fn charge(&self) -> usize {
        if self.lines.is_empty() && self.seeds.is_empty() && self.extent.is_none() {
            return 0;
        }
        self.lines
            .iter()
            .fold(512usize * 1024, |charge, line| {
                charge
                    .saturating_add(line.index.charged_bytes().unwrap_or(usize::MAX))
                    .saturating_add(
                        line.prepared
                            .as_ref()
                            .map_or(0, |prepared| prepared.charged_bytes),
                    )
                    .saturating_add(
                        line.candidate
                            .as_ref()
                            .map_or(0, |candidate| candidate.charged_bytes),
                    )
            })
            .saturating_add(
                self.seeds
                    .iter()
                    .map(|page| page.encoded_bytes)
                    .sum::<usize>(),
            )
            .saturating_add(self.extent.as_ref().map_or(0, |extent| {
                extent.index.charged_bytes().unwrap_or(usize::MAX)
            }))
    }

    pub(super) fn take_range_restart(&mut self) -> bool {
        std::mem::take(&mut self.range_restart)
    }

    pub(super) fn adopt(
        &mut self,
        viewport: &CodeViewport,
        pages: Vec<Arc<IndexedResourceResponse>>,
        appearance: &CodeAppearance,
        window: &Window,
        font_environment: gpui::FontEnvironmentKey,
    ) -> Result<(), CodeLayoutError> {
        if window.text_system().font_environment_key() != Some(font_environment) {
            return Err(CodeLayoutError::FontEnvironmentChanged);
        }
        let environment = Environment {
            font_environment,
            font: appearance.body_font.font.clone(),
            size: appearance.body_font.size,
            line_height: viewport.scroll.borrow().line_height,
            scale: window.scale_factor(),
        };
        if self.environment.as_ref() != Some(&environment) {
            let range = self.range;
            self.reset();
            self.range = range;
            self.environment = Some(environment);
            self.epoch = NEXT_LAYOUT_EPOCH.fetch_add(1, Ordering::Relaxed);
        }
        for page in &pages {
            let Some(root) = &page.root else {
                continue;
            };
            let binding = match &page.value {
                IndexedResourceValue::Range(value) => &value.binding,
                IndexedResourceValue::Display(value) => &value.binding,
                IndexedResourceValue::Source(value) => &value.binding,
                _ => continue,
            };
            if self
                .binding
                .as_ref()
                .is_some_and(|current| current != binding)
                || self.root.is_some_and(|current| current != root.digest)
            {
                self.lines.clear();
                self.seeds.clear();
                self.query = None;
                self.refused = None;
                self.remaining_query = None;
                self.range_admitted = false;
                self.resolved_inline = None;
                self.restored_inline = None;
                self.extent = None;
                self.epoch = NEXT_LAYOUT_EPOCH.fetch_add(1, Ordering::Relaxed);
            }
            self.binding = Some(binding.clone());
            self.root = Some(root.digest);
            self.total_lines = Some(root.code_lines);
        }
        let range = viewport.scroll.borrow().range();
        self.lines
            .retain(|line| range.start <= line.ordinal && line.ordinal < range.end);
        for page in &pages {
            let IndexedResourceValue::Range(value) = &page.value else {
                continue;
            };
            if let (index::Request::Code { lines }, index::Position::Code(next)) =
                (value.cursor.request, value.cursor.next)
            {
                if lines.start <= range.start && lines.end >= range.end {
                    self.range_admitted = true;
                }
                let start = next.saturating_add(u64::from(value.cursor.fragment_offset != 0));
                self.remaining_query = (!value.request_complete && start < lines.end).then(|| {
                    let query = if value.cursor.fragment_offset == 0 {
                        IndexedResourceQuery::Range {
                            request: value.cursor.request,
                            cursor: Some(value.cursor),
                            descriptors: 16,
                            bytes: 65536,
                        }
                    } else {
                        IndexedResourceQuery::Range {
                            request: index::Request::Code {
                                lines: index::OrdinalRange {
                                    start,
                                    end: lines.end,
                                },
                            },
                            cursor: None,
                            descriptors: (lines.end - start).min(16) as usize,
                            bytes: 65536,
                        }
                    };
                    IndexedResourceDemand {
                        revision: viewport.revision,
                        root: self.root,
                        query,
                    }
                });
            }
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
                if ordinal < range.start
                    || ordinal >= range.end
                    || self.lines.iter().any(|line| line.ordinal == ordinal)
                {
                    continue;
                }
                if self.lines.len() >= 16 {
                    return Err(CodeLayoutError::Capacity);
                }
                let source = value
                    .binding
                    .range
                    .start
                    .checked_add(content.start)
                    .ok_or(CodeLayoutError::Overflow)?
                    ..value
                        .binding
                        .range
                        .start
                        .checked_add(content.end)
                        .ok_or(CodeLayoutError::Overflow)?;
                self.sequence = self
                    .sequence
                    .checked_add(1)
                    .ok_or(CodeLayoutError::Overflow)?;
                let binding = StreamingLayoutBinding {
                    input_id: self.sequence,
                    segment_policy_id: 1,
                    start_position: StreamingLayoutPosition::at(source.start),
                    wrap_width: px(1.0),
                    font_size: px(appearance.body_font.size),
                    line_height: px(viewport.scroll.borrow().line_height),
                    limits: StreamingLayoutLimits {
                        segment_bytes: 1024,
                        runs: 8,
                        decorations: 8,
                        glyphs: 4096,
                        wraps: 1,
                        maps: 4097,
                        fragments: 1,
                        retained_items: 16384,
                        retained_bytes: 512 * 1024,
                    },
                };
                let limits = CodeLayoutLimits {
                    segment_bytes: 1024,
                    page_bytes: 8192,
                    segments_per_quantum: 1,
                    boundary_steps_per_quantum: 2048,
                    checkpoints: 32,
                    fragments: 4,
                    retained_bytes: 2 * 1024 * 1024,
                    retained_items: 65536,
                };
                let index = CodeLayoutIndex::new(binding, source.clone(), ordinal, limits)?;
                self.lines.push(Line {
                    ordinal,
                    source,
                    index,
                    target: None,
                    prepared: None,
                    candidate: None,
                    width: None,
                    scanning_extent: false,
                    anchor_request: None,
                });
            }
        }
        if pages
            .iter()
            .any(|page| matches!(&page.value, IndexedResourceValue::Range(_)))
        {
            self.seeds = pages;
        } else if let Some(page) = pages.last() {
            if let IndexedResourceValue::Source(value) = &page.value {
                if let Some(extent) = &mut self.extent {
                    if extent.index.next_read().is_some_and(|read| {
                        read.range.start == value.range.start && read.range.end == value.range.end
                    }) {
                        extent.supply(value)?;
                    }
                }
                return Ok(());
            }
            let IndexedResourceValue::Display(value) = &page.value else {
                return Ok(());
            };
            let Some(fragment) = &value.fragment else {
                return Ok(());
            };
            let index::Target::Code(ordinal) = value.cursor.request.target else {
                return Err(CodeLayoutError::InvalidPage);
            };
            if let Some(line) = self.lines.iter_mut().find(|line| line.ordinal == ordinal) {
                Self::supply(line, fragment)?;
            }
        }
        Ok(())
    }

    fn supply(line: &mut Line, fragment: &index::Fragment) -> Result<bool, CodeLayoutError> {
        let Some(read) = line.index.next_read() else {
            return Ok(false);
        };
        let mut start = read.range.start.max(fragment.source.start);
        let mut end = read.range.end.min(fragment.source.end);
        if read.kind == CodeLayoutReadKind::Context {
            while start < end
                && !fragment
                    .text
                    .is_char_boundary((start - fragment.source.start) as usize)
            {
                start += 1;
            }
        }
        if read.kind == CodeLayoutReadKind::Forward {
            while end > start
                && !fragment
                    .text
                    .is_char_boundary((end - fragment.source.start) as usize)
            {
                end -= 1;
            }
        }
        if start >= end
            || (read.kind == CodeLayoutReadKind::Forward && start != read.range.start)
            || (read.kind == CodeLayoutReadKind::Context && end != read.range.end)
        {
            return Ok(false);
        }
        let start_index = usize::try_from(start - fragment.source.start)
            .map_err(|_| CodeLayoutError::Overflow)?;
        let end_index =
            usize::try_from(end - fragment.source.start).map_err(|_| CodeLayoutError::Overflow)?;
        let text = fragment
            .text
            .get(start_index..end_index)
            .ok_or(CodeLayoutError::InvalidPage)?;
        line.index.supply_page(start, text)?;
        Ok(true)
    }

    pub(super) fn advance(
        &mut self,
        viewport: &mut CodeViewport,
        appearance: &CodeAppearance,
        window: &Window,
    ) -> Result<bool, CodeLayoutError> {
        self.validate_environment(viewport, window)?;
        let result = self.advance_current(viewport, appearance, window);
        self.validate_environment(viewport, window)?;
        result
    }

    pub(super) fn validate_environment(
        &mut self,
        viewport: &mut CodeViewport,
        window: &Window,
    ) -> Result<(), CodeLayoutError> {
        let current = window.text_system().font_environment_key();
        if current.is_some()
            && current
                == self
                    .environment
                    .as_ref()
                    .map(|environment| environment.font_environment)
        {
            return Ok(());
        }
        viewport.begin_inline_remap();
        self.reset();
        self.range_restart = true;
        viewport.layout_query = None;
        Err(if current.is_some() {
            CodeLayoutError::FontEnvironmentChanged
        } else {
            CodeLayoutError::FontEnvironmentUnavailable
        })
    }

    fn advance_current(
        &mut self,
        viewport: &mut CodeViewport,
        appearance: &CodeAppearance,
        window: &Window,
    ) -> Result<bool, CodeLayoutError> {
        if self.refused.is_some() {
            return Ok(false);
        }
        if !self.range_admitted {
            self.query = None;
            return Ok(false);
        }
        let anchor = viewport.pending_inline_anchor();
        if self
            .resolved_inline
            .is_some_and(|(resolved, _)| Some(resolved) != anchor)
        {
            self.resolved_inline = None;
        }
        let scroll = viewport.scroll.borrow();
        let left = scroll.horizontal;
        let right = left + (f32::from(scroll.bounds.size.width) - 20.0).max(1.0);
        drop(scroll);
        let foreground = self.lines.iter().position(|line| {
            line.target != Some((left, right))
                || !line
                    .candidate
                    .as_ref()
                    .or(line.prepared.as_ref())
                    .is_some_and(|prepared| {
                        f32::from(prepared.target.inline_start) == left
                            && f32::from(prepared.target.inline_end) == right
                    })
        });
        let remap = anchor.and_then(|anchor| {
            self.lines
                .iter()
                .position(|line| line.ordinal == anchor.logical_line)
        });
        if anchor.is_some() && remap.is_none() {
            self.query = self.remaining_query.clone();
            return Ok(false);
        }
        if foreground.is_none() && self.remaining_query.is_some() {
            self.query = self.remaining_query.clone();
            return Ok(false);
        }
        let Some(position) = remap
            .or(foreground)
            .or_else(|| self.lines.iter().position(|line| line.width.is_none()))
        else {
            return self.advance_extent(viewport, appearance, window);
        };
        let line = &mut self.lines[position];
        if let Some(anchor) = anchor {
            if line
                .anchor_request
                .is_none_or(|request| request.source != anchor.source.position)
            {
                self.sequence = self
                    .sequence
                    .checked_add(1)
                    .ok_or(CodeLayoutError::Overflow)?;
                let request = CodeLayoutAnchor {
                    request_id: self.sequence,
                    logical_line: line.ordinal,
                    source: anchor.source.position,
                };
                line.index.request_source_anchor(request)?;
                line.anchor_request = Some(request);
                line.target = None;
                line.candidate = None;
                line.scanning_extent = false;
            }
        } else if line.target != Some((left, right)) || line.anchor_request.is_some() {
            self.sequence = self
                .sequence
                .checked_add(1)
                .ok_or(CodeLayoutError::Overflow)?;
            line.index.request_target(CodeLayoutTarget {
                request_id: self.sequence,
                lines: line.ordinal..line.ordinal + 1,
                inline_start: px(left),
                inline_end: px(right),
            })?;
            line.target = Some((left, right));
            line.scanning_extent = false;
            line.anchor_request = None;
        }
        if let Some(read) = line.index.next_read() {
            let mut supplied = false;
            for seed in &self.seeds {
                let IndexedResourceValue::Range(value) = &seed.value else {
                    continue;
                };
                for presentation in value.presentations.iter().flatten() {
                    if let Some(fragment) = &presentation.fragment {
                        if Self::supply(line, fragment)? {
                            supplied = true;
                            break;
                        }
                    }
                }
                if supplied {
                    break;
                }
            }
            if !supplied {
                let start = if read.kind == CodeLayoutReadKind::Context
                    && read.range.start > line.source.start
                {
                    // Display can round back three UTF-8 bytes; retain coverage of the context end.
                    read.range
                        .start
                        .saturating_add(3)
                        .min(read.range.end.saturating_sub(1))
                } else {
                    read.range.start
                };
                self.query = Some(IndexedResourceDemand {
                    revision: viewport.revision,
                    root: self.root,
                    query: IndexedResourceQuery::Display {
                        request: index::DisplayRequest {
                            target: index::Target::Code(line.ordinal),
                            offset: start
                                .checked_sub(line.source.start)
                                .ok_or(CodeLayoutError::InvalidPage)?,
                        },
                        cursor: None,
                        bytes: 16384,
                    },
                });
                return Ok(false);
            }
        }
        self.query = None;
        let run = TextRun {
            len: 0,
            font: appearance.body_font.font.clone(),
            color: appearance.body_text.into(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let progress = line.index.advance(window.text_system(), &run)?;
        if matches!(
            progress,
            CodeLayoutProgress::Ready | CodeLayoutProgress::Complete
        ) {
            if let Some(anchor) = anchor {
                let resolved = line
                    .index
                    .resolved_anchor()
                    .ok_or(CodeLayoutError::InvalidGeometry)?;
                if Some(resolved.anchor) != line.anchor_request {
                    return Err(CodeLayoutError::InvalidTarget);
                }
                if !(f32::from(resolved.inline) - anchor.visual_offset).is_finite() {
                    return Err(CodeLayoutError::InvalidGeometry);
                }
                self.resolved_inline = Some((anchor, f32::from(resolved.inline)));
                return Ok(true);
            }
            if line.scanning_extent {
                line.width = line.index.complete_max_width().map(f32::from);
                return Ok(true);
            }
            let prepared = line
                .index
                .take_prepared()
                .ok_or(CodeLayoutError::InvalidGeometry)?;
            if let Some(width) = prepared.complete_max_width {
                line.width = Some(f32::from(width));
            }
            if !line.scanning_extent {
                line.candidate = Some(prepared);
            }
            if line.width.is_none() {
                line.index.continue_indexing();
                line.scanning_extent = true;
            }
        }
        Ok(true)
    }

    pub(super) fn resolve_inline_anchor(&mut self, viewport: &mut CodeViewport) -> bool {
        let Some((anchor, inline)) = self.resolved_inline.take() else {
            return false;
        };
        if viewport.resolve_inline_anchor(anchor, inline) {
            self.restored_inline = Some((anchor, viewport.scroll.borrow().horizontal));
            true
        } else {
            false
        }
    }

    pub(super) fn complete_width(&self, total_lines: u64) -> Option<f32> {
        if self.total_lines != Some(total_lines) {
            return None;
        }
        if let Some(width) = self.extent.as_ref().and_then(|extent| extent.complete) {
            return Some(width);
        }
        if self.binding.is_none() || self.lines.len() as u64 != total_lines {
            return None;
        }
        self.lines
            .iter()
            .try_fold(0.0f32, |width, line| Some(width.max(line.width?)))
    }

    pub(super) fn source_bytes(&self) -> Option<u64> {
        self.binding
            .as_ref()
            .map(|binding| binding.range.end - binding.range.start)
    }

    pub(super) fn prepared(&self) -> impl Iterator<Item = &PreparedCodeLayout> {
        self.lines.iter().filter_map(|line| line.prepared.as_ref())
    }

    pub(super) fn empty_line_caret(&self, ordinal: u64) -> Option<u64> {
        let line = self.lines.iter().find(|line| line.ordinal == ordinal)?;
        (line.source.is_empty() && line.prepared.as_ref()?.terminal_width == Some(px(0.0)))
            .then_some(line.source.start)
    }

    #[cfg(test)]
    pub(super) fn work_status(&self) -> String {
        format!(
            "admitted={} range={:?} lines={:?} remaining={:?} query={:?}",
            self.range_admitted,
            self.range,
            self.lines
                .iter()
                .map(|line| line.ordinal)
                .collect::<Vec<_>>(),
            self.remaining_query,
            self.query
        )
    }

    #[cfg(test)]
    pub(super) fn foreground_response(&self) -> Option<Arc<IndexedResourceResponse>> {
        self.seeds.last().cloned()
    }

    #[cfg(test)]
    pub(super) fn line_width(&self, ordinal: u64) -> Option<f32> {
        self.lines
            .iter()
            .find(|line| line.ordinal == ordinal)?
            .width
    }

    pub(super) fn publish_candidate(&mut self, viewport: &mut CodeViewport) {
        if viewport.inline_remapping || viewport.remapping {
            return;
        }
        let scroll = viewport.scroll.borrow();
        let target = (
            scroll.horizontal,
            scroll.horizontal + (f32::from(scroll.bounds.size.width) - 20.0).max(1.0),
        );
        if !self.lines.iter().all(|line| {
            line.candidate
                .as_ref()
                .or(line.prepared.as_ref())
                .is_some_and(|prepared| {
                    target
                        == (
                            f32::from(prepared.target.inline_start),
                            f32::from(prepared.target.inline_end),
                        )
                })
        }) {
            return;
        }
        let changed = !viewport.inline_anchor_is_current();
        let restored = self.restored_inline.take().filter(|(anchor, left)| {
            *left == scroll.horizontal
                && scroll.range().start <= anchor.logical_line
                && anchor.logical_line < scroll.range().end
        });
        for line in &mut self.lines {
            if let Some(candidate) = line.candidate.take() {
                line.prepared = Some(candidate);
            }
        }
        drop(scroll);
        if let Some((anchor, _)) = restored {
            viewport.retain_inline_anchor(anchor);
        } else if changed {
            self.retain_inline_anchor(viewport);
        }
    }

    fn retain_inline_anchor(&self, viewport: &mut CodeViewport) {
        let scroll = viewport.scroll.borrow();
        let Some(binding) = &self.binding else {
            return;
        };
        let Some(line) = self
            .lines
            .iter()
            .find(|line| line.ordinal == scroll.top_line)
        else {
            return;
        };
        let Some(prepared) = &line.prepared else {
            return;
        };
        let left = f32::from(prepared.target.inline_start);
        let mut before: Option<(u64, f32)> = None;
        let mut after: Option<(u64, f32)> = None;
        for fragment in &prepared.fragments {
            for fragment in fragment.fragments.iter() {
                let maps: &[gpui::StreamingLayoutMap] = match fragment {
                    gpui::StreamingLayoutFragment::Text(fragment) => fragment.maps(),
                    gpui::StreamingLayoutFragment::OversizeAtom(fragment) => fragment.maps(),
                    gpui::StreamingLayoutFragment::InlineObject(fragment) => fragment.maps(),
                    gpui::StreamingLayoutFragment::Boundary(fragment) => fragment.maps(),
                };
                for map in maps {
                    let source = map.logical_position.byte_offset;
                    let inline = f32::from(map.position.x);
                    if source < line.source.start || source > line.source.end || !inline.is_finite()
                    {
                        continue;
                    }
                    if inline <= left && before.is_none_or(|(_, previous)| inline >= previous) {
                        before = Some((source, inline));
                    } else if inline > left && after.is_none_or(|(_, previous)| inline < previous) {
                        after = Some((source, inline));
                    }
                }
            }
        }
        let Some((position, inline)) = before
            .or(after)
            .or_else(|| line.width.map(|width| (line.source.end, width)))
        else {
            return;
        };
        let anchor = CodeInlineAnchor {
            source: index::SourceAnchor {
                source: binding.source,
                block_start: binding.range.start,
                kind: super::super::provider::IndexedResourceKind::Code,
                position,
                visual_offset: -(scroll.remainder.round() as i64),
            },
            logical_line: line.ordinal,
            visual_offset: inline - left,
        };
        drop(scroll);
        viewport.retain_inline_anchor(anchor);
    }

    pub(super) fn discard_candidate(&mut self) {
        for line in &mut self.lines {
            line.candidate = None;
        }
    }

    pub(super) fn checkpoints(&self) -> usize {
        self.lines
            .iter()
            .map(|line| line.index.checkpoint_count())
            .sum::<usize>()
            + self
                .extent
                .as_ref()
                .map_or(0, |extent| extent.index.checkpoint_count())
    }

    pub(super) fn state(&self, viewport: &CodeViewport) -> &'static str {
        let scroll = viewport.scroll.borrow();
        let target = (
            scroll.horizontal,
            scroll.horizontal + (f32::from(scroll.bounds.size.width) - 20.0).max(1.0),
        );
        if self.refused.is_some() {
            "refused"
        } else if viewport.inline_remapping || viewport.remapping {
            "pending"
        } else if self.lines.is_empty()
            && self
                .binding
                .as_ref()
                .is_some_and(|binding| binding.range.start == binding.range.end)
        {
            "ready"
        } else if self.lines.is_empty()
            || self
                .range
                .is_some_and(|range| self.lines.len() as u64 != range.end - range.start)
        {
            "pending"
        } else if self.lines.iter().all(|line| {
            line.prepared.as_ref().is_some_and(|prepared| {
                target
                    == (
                        f32::from(prepared.target.inline_start),
                        f32::from(prepared.target.inline_end),
                    )
            })
        }) {
            "ready"
        } else {
            "pending"
        }
    }
}
