use std::{
    collections::{HashMap, HashSet},
    ops::Range,
};

#[path = "transcript_selection/copy_text.rs"]
mod copy_text;
#[path = "transcript_selection/hit_test.rs"]
mod hit_test;
#[path = "transcript_selection/word.rs"]
mod word;

#[allow(unused_imports)]
pub(crate) use self::copy_text::{TranscriptLineCopyGroup, TranscriptLineCopyText};
use self::copy_text::{TranscriptSelectedLineCopy, selected_text_from_copy_lines};
pub(crate) use self::hit_test::vertical_hit_candidate_range;
use self::word::word_range_at;

pub(crate) const TRANSCRIPT_NARRATIVE_BLOCK_BREAK_BEFORE: usize = 2;

pub(crate) fn transcript_narrative_block_break_before(
    previous_rendered_narrative_blocks: usize,
) -> usize {
    if previous_rendered_narrative_blocks == 0 {
        0
    } else {
        TRANSCRIPT_NARRATIVE_BLOCK_BREAK_BEFORE
    }
}

pub(crate) fn transcript_context_line_break_before(
    context_line_index: usize,
    context_break_before: usize,
    explicit_break_before: Option<usize>,
) -> usize {
    match explicit_break_before {
        Some(0) if context_line_index > 0 => 0,
        Some(explicit_break_before) if context_break_before > 0 => {
            context_break_before.max(explicit_break_before)
        }
        Some(_) | None => context_break_before,
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TranscriptTextLineKey {
    row_identity: String,
    block_path: String,
    line_index: usize,
}

impl TranscriptTextLineKey {
    pub(crate) fn new(
        row_identity: impl Into<String>,
        block_path: impl Into<String>,
        line_index: usize,
    ) -> Self {
        Self {
            row_identity: row_identity.into(),
            block_path: block_path.into(),
            line_index,
        }
    }

    pub(crate) fn row_identity(&self) -> &str {
        &self.row_identity
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TranscriptTextPoint {
    pub(crate) key: TranscriptTextLineKey,
    pub(crate) offset: usize,
}

impl TranscriptTextPoint {
    pub(crate) fn new(key: TranscriptTextLineKey, offset: usize) -> Self {
        Self { key, offset }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VisibleTranscriptTextLine {
    pub(crate) key: TranscriptTextLineKey,
    pub(crate) order: usize,
    pub(crate) text: String,
    pub(crate) copy_text: TranscriptLineCopyText,
    pub(crate) break_before: usize,
}

impl VisibleTranscriptTextLine {
    pub(crate) fn new(
        key: TranscriptTextLineKey,
        order: usize,
        text: impl Into<String>,
        break_before: usize,
    ) -> Self {
        let text = text.into();
        Self {
            key,
            order,
            copy_text: TranscriptLineCopyText::plain(text.clone()),
            text,
            break_before,
        }
    }

    pub(crate) fn with_copy_text(
        key: TranscriptTextLineKey,
        order: usize,
        text: impl Into<String>,
        copy_text: TranscriptLineCopyText,
        break_before: usize,
    ) -> Self {
        Self {
            key,
            order,
            text: text.into(),
            copy_text,
            break_before,
        }
    }

    fn atomic_ranges(&self) -> Vec<Range<usize>> {
        self.copy_text.atomic_ranges()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct VisibleTranscriptTextFrame {
    lines: Vec<VisibleTranscriptTextLine>,
    key_indexes: HashMap<TranscriptTextLineKey, usize>,
}

impl VisibleTranscriptTextFrame {
    pub(crate) fn clear(&mut self) {
        self.lines.clear();
        self.key_indexes.clear();
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    pub(crate) fn insert_line(&mut self, line: VisibleTranscriptTextLine) {
        if let Some(index) = self.key_indexes.get(&line.key).copied() {
            self.lines[index] = line;
            return;
        }

        self.key_indexes.insert(line.key.clone(), self.lines.len());
        self.lines.push(line);
    }

    pub(crate) fn finish_insertions(&mut self) {
        self.lines.sort_by_key(|line| line.order);
        self.rebuild_indexes();
    }

    pub(crate) fn line(&self, key: &TranscriptTextLineKey) -> Option<&VisibleTranscriptTextLine> {
        self.key_indexes
            .get(key)
            .and_then(|index| self.lines.get(*index))
    }

    pub(crate) fn contains_key(&self, key: &TranscriptTextLineKey) -> bool {
        self.key_indexes.contains_key(key)
    }

    pub(crate) fn lines(&self) -> &[VisibleTranscriptTextLine] {
        &self.lines
    }

    fn rebuild_indexes(&mut self) {
        self.key_indexes.clear();
        self.key_indexes.extend(
            self.lines
                .iter()
                .enumerate()
                .map(|(index, line)| (line.key.clone(), index)),
        );
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TranscriptSelectedLineRange {
    pub(crate) key: TranscriptTextLineKey,
    pub(crate) start: usize,
    pub(crate) end: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TranscriptSelectionSnapshot {
    selected_text: String,
    selected_lines: Vec<TranscriptSelectedLineSnapshot>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TranscriptSelectedLineSnapshot {
    key: TranscriptTextLineKey,
    start: usize,
    end: usize,
    copy_text: TranscriptLineCopyText,
    break_before: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TranscriptSelectionState {
    anchor: Option<TranscriptTextPoint>,
    focus: Option<TranscriptTextPoint>,
    dragging: bool,
    snapshot: Option<TranscriptSelectionSnapshot>,
}

impl TranscriptSelectionState {
    pub(crate) fn begin(
        &mut self,
        point: TranscriptTextPoint,
        frame: &VisibleTranscriptTextFrame,
    ) -> bool {
        let changed = self.anchor.as_ref() != Some(&point)
            || self.focus.as_ref() != Some(&point)
            || !self.dragging;
        self.anchor = Some(point.clone());
        self.focus = Some(point);
        self.dragging = true;
        self.sync_visible_frame(frame) || changed
    }

    pub(crate) fn extend(
        &mut self,
        point: TranscriptTextPoint,
        frame: &VisibleTranscriptTextFrame,
    ) -> bool {
        if self.anchor.is_none() {
            return self.begin(point, frame);
        }

        let changed = self.focus.as_ref() != Some(&point);
        self.focus = Some(point);
        self.sync_visible_frame(frame) || changed
    }

    pub(crate) fn select_word(
        &mut self,
        point: TranscriptTextPoint,
        frame: &VisibleTranscriptTextFrame,
    ) -> bool {
        let Some(line) = frame.line(&point.key) else {
            return self.clear();
        };
        let Some(range) = word_range_at(line.text.as_str(), point.offset) else {
            return self.clear();
        };

        let anchor = TranscriptTextPoint::new(point.key.clone(), range.start);
        let focus = TranscriptTextPoint::new(point.key, range.end);
        let changed = self.anchor.as_ref() != Some(&anchor)
            || self.focus.as_ref() != Some(&focus)
            || self.dragging;
        self.anchor = Some(anchor);
        self.focus = Some(focus);
        self.dragging = false;
        self.sync_visible_frame(frame) || changed
    }

    pub(crate) fn finish_drag(&mut self) -> bool {
        if !self.dragging {
            return false;
        }

        self.dragging = false;
        true
    }

    pub(crate) fn clear(&mut self) -> bool {
        let changed = self.anchor.is_some()
            || self.focus.is_some()
            || self.dragging
            || self.snapshot.is_some();
        self.anchor = None;
        self.focus = None;
        self.dragging = false;
        self.snapshot = None;
        changed
    }

    pub(crate) fn is_dragging(&self) -> bool {
        self.dragging
    }

    pub(crate) fn has_selected_text(&self) -> bool {
        self.snapshot.is_some()
    }

    pub(crate) fn selected_text(&self) -> Option<&str> {
        self.snapshot
            .as_ref()
            .map(|snapshot| snapshot.selected_text.as_str())
    }

    pub(crate) fn sync_visible_frame(&mut self, frame: &VisibleTranscriptTextFrame) -> bool {
        if self.anchor.is_none() && self.focus.is_none() {
            return self.snapshot.take().is_some();
        }

        let Some((start, end)) = self.normalized_points(frame) else {
            return false;
        };
        let snapshot = selection_snapshot_for_points(frame, &start, &end);
        if self.snapshot == snapshot {
            return false;
        }

        self.snapshot = snapshot;
        true
    }

    pub(crate) fn clear_if_intersects_row_identities(
        &mut self,
        row_identities: &HashSet<String>,
    ) -> bool {
        if row_identities.is_empty()
            || !self.snapshot.as_ref().is_some_and(|snapshot| {
                snapshot
                    .selected_lines
                    .iter()
                    .any(|line| row_identities.contains(line.key.row_identity()))
            })
        {
            return false;
        }

        self.clear()
    }

    pub(crate) fn selected_line_ranges(
        &self,
        frame: &VisibleTranscriptTextFrame,
    ) -> Vec<TranscriptSelectedLineRange> {
        let Some(snapshot) = self.snapshot.as_ref() else {
            return Vec::new();
        };

        frame
            .lines()
            .iter()
            .filter_map(|line| {
                let selected_line = snapshot
                    .selected_lines
                    .iter()
                    .find(|selected_line| selected_line.key == line.key)?;
                let start = clamp_to_char_boundary(line.text.as_str(), selected_line.start);
                let end = clamp_to_char_boundary(line.text.as_str(), selected_line.end);
                (start <= end).then(|| TranscriptSelectedLineRange {
                    key: selected_line.key.clone(),
                    start,
                    end,
                })
            })
            .collect()
    }

    fn normalized_points(
        &self,
        frame: &VisibleTranscriptTextFrame,
    ) -> Option<(NormalizedTranscriptTextPoint, NormalizedTranscriptTextPoint)> {
        let anchor = normalize_point(self.anchor.as_ref()?, frame)?;
        let focus = normalize_point(self.focus.as_ref()?, frame)?;
        if anchor.order < focus.order
            || (anchor.order == focus.order && anchor.offset <= focus.offset)
        {
            Some((anchor, focus))
        } else {
            Some((focus, anchor))
        }
    }
}

fn selection_snapshot_for_points(
    frame: &VisibleTranscriptTextFrame,
    start: &NormalizedTranscriptTextPoint,
    end: &NormalizedTranscriptTextPoint,
) -> Option<TranscriptSelectionSnapshot> {
    if start.order == end.order && start.offset == end.offset {
        return None;
    }

    let selected_lines = frame
        .lines()
        .iter()
        .filter(|line| line.order >= start.order && line.order <= end.order)
        .filter_map(|line| {
            let range_start = if line.order == start.order {
                start.offset
            } else {
                0
            };
            let range_end = if line.order == end.order {
                end.offset
            } else {
                line.text.len()
            };
            let (range_start, range_end) =
                expand_range_to_intersecting_atoms(range_start, range_end, &line.atomic_ranges());
            (range_start <= range_end).then(|| TranscriptSelectedLineSnapshot {
                key: line.key.clone(),
                start: range_start,
                end: range_end,
                copy_text: line.copy_text.clone(),
                break_before: line.break_before,
            })
        })
        .collect::<Vec<_>>();
    if selected_lines.is_empty() {
        return None;
    }

    let selected_text = selected_text_from_snapshot_lines(&selected_lines);
    if selected_text.is_empty() {
        return None;
    }

    Some(TranscriptSelectionSnapshot {
        selected_text,
        selected_lines,
    })
}

fn selected_text_from_snapshot_lines(lines: &[TranscriptSelectedLineSnapshot]) -> String {
    let lines = lines
        .iter()
        .map(|line| TranscriptSelectedLineCopy {
            copy_text: &line.copy_text,
            start: line.start,
            end: line.end,
            break_before: line.break_before,
        })
        .collect::<Vec<_>>();
    selected_text_from_copy_lines(lines.as_slice())
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct NormalizedTranscriptTextPoint {
    order: usize,
    offset: usize,
}

fn normalize_point(
    point: &TranscriptTextPoint,
    frame: &VisibleTranscriptTextFrame,
) -> Option<NormalizedTranscriptTextPoint> {
    let line = frame.line(&point.key)?;
    let offset = snap_offset_to_atoms(line, point.offset);
    Some(NormalizedTranscriptTextPoint {
        order: line.order,
        offset: clamp_to_char_boundary(line.text.as_str(), offset),
    })
}

fn snap_offset_to_atoms(line: &VisibleTranscriptTextLine, offset: usize) -> usize {
    let offset = clamp_to_char_boundary(line.text.as_str(), offset);
    for range in line.atomic_ranges() {
        if offset > range.start && offset < range.end {
            return if offset - range.start < range.end - offset {
                range.start
            } else {
                range.end
            };
        }
    }
    offset
}

fn expand_range_to_intersecting_atoms(
    mut start: usize,
    mut end: usize,
    atomic_ranges: &[Range<usize>],
) -> (usize, usize) {
    for range in atomic_ranges {
        if ranges_intersect(&(start..end), range) {
            start = start.min(range.start);
            end = end.max(range.end);
        }
    }
    (start, end)
}

fn ranges_intersect(a: &Range<usize>, b: &Range<usize>) -> bool {
    a.start < b.end && b.start < a.end
}

fn clamp_to_char_boundary(text: &str, offset: usize) -> usize {
    let mut offset = offset.min(text.len());
    while offset > 0 && !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}
