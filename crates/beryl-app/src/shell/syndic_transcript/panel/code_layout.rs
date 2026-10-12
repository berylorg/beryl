#[path = "code_layout/prepare.rs"]
mod prepare;
#[path = "code_layout/segments.rs"]
mod segments;

use gpui::{
    Pixels, StreamingEndOfSource, StreamingLayoutAdmission, StreamingLayoutBinding,
    StreamingLayoutContinuation, StreamingLayoutError, StreamingLayoutFragment,
    StreamingLayoutMode, StreamingLayoutPosition, StreamingLineFinalization, StreamingOversizeAtom,
    StreamingTextSegment, TextRun, WindowTextSystem, px,
};
use segments::{Segment, SegmentCheckpoint, SegmentKind, Segmenter};
use std::{mem::size_of, ops::Range, sync::Arc};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct CodeLayoutLimits {
    pub segment_bytes: usize,
    pub page_bytes: usize,
    pub segments_per_quantum: usize,
    pub boundary_steps_per_quantum: usize,
    pub checkpoints: usize,
    pub fragments: usize,
    pub retained_bytes: usize,
    pub retained_items: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct CodeLayoutTarget {
    pub request_id: u64,
    pub lines: Range<u64>,
    pub inline_start: Pixels,
    pub inline_end: Pixels,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct CodeLayoutAnchor {
    pub request_id: u64,
    pub logical_line: u64,
    pub source: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ResolvedCodeAnchor {
    pub anchor: CodeLayoutAnchor,
    pub inline: Pixels,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CodeLayoutReadKind {
    Forward,
    Context,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct CodeLayoutRead {
    pub range: Range<u64>,
    pub kind: CodeLayoutReadKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum CodeLayoutError {
    InvalidConfiguration,
    InvalidTarget,
    InvalidPage,
    UnexpectedPage,
    Capacity,
    Overflow,
    InvalidGeometry,
    FontEnvironmentUnavailable,
    FontEnvironmentChanged,
    Layout(StreamingLayoutError),
}

impl From<StreamingLayoutError> for CodeLayoutError {
    fn from(error: StreamingLayoutError) -> Self {
        Self::Layout(error)
    }
}

pub(super) fn checked_shaping_error(
    error: impl AsRef<dyn std::error::Error + Send + Sync + 'static>,
) -> CodeLayoutError {
    match error.as_ref().downcast_ref::<gpui::FontEnvironmentError>() {
        Some(gpui::FontEnvironmentError::Changed) => CodeLayoutError::FontEnvironmentChanged,
        Some(gpui::FontEnvironmentError::FaceCapacityExceeded) => CodeLayoutError::Capacity,
        None => CodeLayoutError::Layout(StreamingLayoutError::ShapingFailed),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CodeLayoutProgress {
    Pending,
    NeedsSource,
    Ready,
    Complete,
}

#[derive(Clone)]
pub(super) struct PreparedCodeFragment {
    pub logical_line: u64,
    pub source: Range<u64>,
    pub inline_start: Pixels,
    pub inline_end: Pixels,
    pub fragments: Arc<[StreamingLayoutFragment]>,
    pub oversize: bool,
}

#[derive(Clone)]
pub(super) struct PreparedCodeLayout {
    pub target: CodeLayoutTarget,
    pub fragments: Vec<PreparedCodeFragment>,
    pub terminal_width: Option<Pixels>,
    pub complete_max_width: Option<Pixels>,
    pub charged_bytes: usize,
    pub charged_items: usize,
}

#[derive(Clone)]
struct Checkpoint {
    segment: SegmentCheckpoint,
    continuation: Option<StreamingLayoutContinuation>,
    line: u64,
    max_width: Pixels,
}

pub(super) struct CodeLayoutIndex {
    binding: StreamingLayoutBinding,
    source: Range<u64>,
    first_line: u64,
    limits: CodeLayoutLimits,
    scan: Segmenter,
    continuation: Option<StreamingLayoutContinuation>,
    checkpoints: Vec<Checkpoint>,
    checkpoint_stride: u64,
    line: u64,
    max_width: Pixels,
    complete_max_width: Option<Pixels>,
    terminal_width: Option<Pixels>,
    target: Option<CodeLayoutTarget>,
    anchor: Option<CodeLayoutAnchor>,
    resolved_anchor: Option<ResolvedCodeAnchor>,
    output: Vec<PreparedCodeFragment>,
    output_bytes: usize,
    output_items: usize,
    ready: bool,
    prepared_taken: bool,
    complete: bool,
    refused: Option<CodeLayoutError>,
    space_advance: Option<Pixels>,
}

impl CodeLayoutIndex {
    pub fn new(
        binding: StreamingLayoutBinding,
        source: Range<u64>,
        first_line: u64,
        limits: CodeLayoutLimits,
    ) -> Result<Self, CodeLayoutError> {
        if source.start > source.end
            || binding.start_position != StreamingLayoutPosition::at(source.start)
            || limits.segment_bytes == 0
            || limits.segment_bytes > 8192
            || limits.segment_bytes != binding.limits.segment_bytes
            || limits.page_bytes < 4
            || limits.page_bytes > 65_536
            || limits.segments_per_quantum == 0
            || limits.boundary_steps_per_quantum == 0
            || limits.checkpoints < 2
            || limits.fragments == 0
            || limits.retained_items == 0
        {
            return Err(CodeLayoutError::InvalidConfiguration);
        }
        let scan = Segmenter::new(source.clone(), limits.segment_bytes, limits.page_bytes)?;
        let origin = Checkpoint {
            segment: scan.origin(),
            continuation: None,
            line: first_line,
            max_width: Pixels::ZERO,
        };
        let mut checkpoints = Vec::with_capacity(limits.checkpoints);
        checkpoints.push(origin);
        let index = Self {
            binding,
            source,
            first_line,
            limits,
            scan,
            continuation: None,
            checkpoints,
            checkpoint_stride: 1,
            line: first_line,
            max_width: Pixels::ZERO,
            complete_max_width: None,
            terminal_width: None,
            target: None,
            anchor: None,
            resolved_anchor: None,
            output: Vec::with_capacity(limits.fragments),
            output_bytes: 0,
            output_items: 0,
            ready: false,
            prepared_taken: false,
            complete: false,
            refused: None,
            space_advance: None,
        };
        index.check_capacity(0, 0)?;
        Ok(index)
    }

    pub fn request_target(&mut self, target: CodeLayoutTarget) -> Result<(), CodeLayoutError> {
        let start = f32::from(target.inline_start);
        let end = f32::from(target.inline_end);
        if target.lines.start < self.first_line
            || target.lines.start >= target.lines.end
            || !start.is_finite()
            || !end.is_finite()
            || start < 0.0
            || end <= start
        {
            return Err(CodeLayoutError::InvalidTarget);
        }
        let checkpoint = self
            .checkpoints
            .iter()
            .rev()
            .find(|checkpoint| {
                checkpoint.line < target.lines.start
                    || (checkpoint.line == target.lines.start
                        && checkpoint
                            .continuation
                            .map_or(true, |value| value.inline_offset <= target.inline_start))
            })
            .cloned()
            .ok_or(CodeLayoutError::InvalidTarget)?;
        self.scan = Segmenter::resume(
            self.source.clone(),
            self.limits.segment_bytes,
            self.limits.page_bytes,
            checkpoint.segment,
        );
        self.continuation = checkpoint.continuation;
        self.line = checkpoint.line;
        self.max_width = checkpoint.max_width;
        self.target = Some(target);
        self.anchor = None;
        self.resolved_anchor = None;
        self.output.clear();
        self.output_bytes = 0;
        self.output_items = 0;
        self.terminal_width = None;
        self.ready = false;
        self.prepared_taken = false;
        self.complete = false;
        self.refused = None;
        Ok(())
    }

    pub fn request_source_anchor(
        &mut self,
        anchor: CodeLayoutAnchor,
    ) -> Result<(), CodeLayoutError> {
        if anchor.source < self.source.start
            || anchor.source > self.source.end
            || anchor.logical_line < self.first_line
        {
            return Err(CodeLayoutError::InvalidTarget);
        }
        let checkpoint = self
            .checkpoints
            .iter()
            .rev()
            .find(|checkpoint| checkpoint.segment.offset <= anchor.source)
            .cloned()
            .ok_or(CodeLayoutError::InvalidTarget)?;
        self.scan = Segmenter::resume(
            self.source.clone(),
            self.limits.segment_bytes,
            self.limits.page_bytes,
            checkpoint.segment,
        );
        self.continuation = checkpoint.continuation;
        self.line = checkpoint.line;
        self.max_width = checkpoint.max_width;
        self.target = None;
        self.anchor = Some(anchor);
        self.resolved_anchor = None;
        self.output.clear();
        self.output_bytes = 0;
        self.output_items = 0;
        self.ready = false;
        self.prepared_taken = false;
        self.complete = false;
        self.refused = None;
        Ok(())
    }

    pub fn resolved_anchor(&self) -> Option<ResolvedCodeAnchor> {
        self.resolved_anchor
    }

    pub fn continue_indexing(&mut self) {
        self.target = None;
        self.anchor = None;
        self.resolved_anchor = None;
        self.output.clear();
        self.output_bytes = 0;
        self.output_items = 0;
        self.ready = false;
        self.prepared_taken = false;
    }

    pub fn next_read(&self) -> Option<CodeLayoutRead> {
        if self.ready || self.refused.is_some() {
            None
        } else {
            self.scan.read()
        }
    }

    pub fn supply_page(&mut self, start: u64, text: &str) -> Result<(), CodeLayoutError> {
        if let Some(error) = &self.refused {
            return Err(error.clone());
        }
        self.check_capacity(0, 0)?;
        self.scan.supply(start, text)
    }

    pub fn advance(
        &mut self,
        text_system: &WindowTextSystem,
        run: &TextRun,
    ) -> Result<CodeLayoutProgress, CodeLayoutError> {
        if let Some(error) = &self.refused {
            return Err(error.clone());
        }
        let result = self.advance_inner(text_system, run);
        if let Err(error) = &result {
            self.refused = Some(error.clone());
        }
        result
    }

    pub fn take_prepared(&mut self) -> Option<PreparedCodeLayout> {
        if !self.ready || self.prepared_taken || self.refused.is_some() {
            return None;
        }
        let target = self.target.clone()?;
        let wrapper_items = self.output.len();
        let prepared = PreparedCodeLayout {
            target,
            fragments: std::mem::replace(
                &mut self.output,
                Vec::with_capacity(self.limits.fragments),
            ),
            terminal_width: self.terminal_width,
            complete_max_width: self.complete_max_width,
            charged_bytes: self.output_bytes
                + self.limits.fragments * size_of::<PreparedCodeFragment>(),
            charged_items: self.output_items + wrapper_items,
        };
        self.output_bytes = 0;
        self.output_items = 0;
        self.prepared_taken = true;
        Some(prepared)
    }

    pub fn complete_max_width(&self) -> Option<Pixels> {
        self.complete_max_width
    }
    pub fn checkpoint_count(&self) -> usize {
        self.checkpoints.len()
    }
    pub fn charged_bytes(&self) -> Result<usize, CodeLayoutError> {
        self.base_charge()?
            .checked_add(self.output_bytes)
            .ok_or(CodeLayoutError::Overflow)
    }

    fn add_checkpoint(
        &mut self,
        segment: SegmentCheckpoint,
        continuation: StreamingLayoutContinuation,
    ) -> Result<(), CodeLayoutError> {
        if continuation.ended
            || continuation.next_ordinal % self.checkpoint_stride != 0
            || self
                .checkpoints
                .last()
                .is_some_and(|checkpoint| checkpoint.segment.offset >= segment.offset)
        {
            return Ok(());
        }
        if self.checkpoints.len() == self.limits.checkpoints {
            self.checkpoint_stride = self
                .checkpoint_stride
                .checked_mul(2)
                .ok_or(CodeLayoutError::Overflow)?;
            let stride = self.checkpoint_stride;
            self.checkpoints.retain(|checkpoint| {
                checkpoint.continuation.is_none()
                    || checkpoint
                        .continuation
                        .is_some_and(|value| value.next_ordinal % stride == 0)
            });
        }
        if continuation.next_ordinal % self.checkpoint_stride == 0
            && self.checkpoints.len() < self.limits.checkpoints
        {
            self.checkpoints.push(Checkpoint {
                segment,
                continuation: Some(continuation),
                line: self.line,
                max_width: self.max_width,
            });
        }
        Ok(())
    }

    fn base_charge(&self) -> Result<usize, CodeLayoutError> {
        let counts = [
            size_of::<Self>(),
            self.limits
                .page_bytes
                .checked_mul(2)
                .ok_or(CodeLayoutError::Overflow)?,
            self.limits
                .segment_bytes
                .checked_mul(4)
                .ok_or(CodeLayoutError::Overflow)?,
            self.limits
                .checkpoints
                .checked_mul(size_of::<Checkpoint>())
                .ok_or(CodeLayoutError::Overflow)?,
            self.limits
                .fragments
                .checked_mul(size_of::<PreparedCodeFragment>())
                .ok_or(CodeLayoutError::Overflow)?,
        ];
        counts.into_iter().try_fold(0usize, |total, value| {
            total.checked_add(value).ok_or(CodeLayoutError::Overflow)
        })
    }

    fn check_capacity(&self, bytes: usize, items: usize) -> Result<(), CodeLayoutError> {
        let total_bytes = self
            .base_charge()?
            .checked_add(self.output_bytes)
            .and_then(|total| total.checked_add(bytes))
            .and_then(|total| total.checked_add(self.binding.limits.retained_bytes))
            .ok_or(CodeLayoutError::Overflow)?;
        let total_items = self
            .limits
            .checkpoints
            .checked_mul(2)
            .and_then(|total| total.checked_add(8))
            .and_then(|total| total.checked_add(self.limits.fragments))
            .and_then(|total| total.checked_add(self.output_items))
            .and_then(|total| total.checked_add(items))
            .and_then(|total| total.checked_add(self.binding.limits.retained_items))
            .ok_or(CodeLayoutError::Overflow)?;
        if total_bytes > self.limits.retained_bytes || total_items > self.limits.retained_items {
            Err(CodeLayoutError::Capacity)
        } else {
            Ok(())
        }
    }
}

fn logical_range(source: &Range<u64>) -> Range<StreamingLayoutPosition> {
    StreamingLayoutPosition::at(source.start)..StreamingLayoutPosition::at(source.end)
}

pub(super) fn tab_advance(offset: Pixels, stop: Pixels) -> Result<Pixels, CodeLayoutError> {
    let offset = f64::from(f32::from(offset));
    let stop = f64::from(f32::from(stop));
    if !offset.is_finite() || offset < 0.0 || !stop.is_finite() || stop <= 0.0 {
        return Err(CodeLayoutError::InvalidGeometry);
    }
    let next = (offset / stop).floor().mul_add(stop, stop);
    let next_pixel = next as f32;
    let width = (next - offset) as f32;
    if !next_pixel.is_finite() || next_pixel <= offset as f32 || !width.is_finite() || width <= 0.0
    {
        return Err(CodeLayoutError::InvalidGeometry);
    }
    Ok(px(width))
}
