use std::{collections::HashSet, ops::Range, sync::Arc};

use gpui::Pixels;

use super::{
    execution_detail::{TranscriptRenderMetrics, TurnExecutionRecord},
    transcript_projection::project_parent_narrative_turn,
    virtual_list::{ListScrollPosition, ListState},
};

pub(crate) const TRANSCRIPT_INITIAL_PRESENTATION_ROWS: usize = 96;
pub(crate) const TRANSCRIPT_MAX_PRESENTATION_ROWS: usize = 256;

#[derive(Clone, Default)]
pub(crate) struct TranscriptPresentationState {
    rows: Vec<TranscriptPresentationRow>,
    latest_user_prompt_anchor: Option<(usize, usize, String)>,
    next_ephemeral_row_id: u64,
}

#[derive(Clone)]
struct TranscriptPresentationRow {
    identity: TranscriptRowIdentity,
    source_turn_index: usize,
    turn: Arc<TurnExecutionRecord>,
    placeholder_height: Option<Pixels>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TranscriptRowIdentity(String);

impl TranscriptRowIdentity {
    pub(crate) fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

#[derive(Clone)]
pub(crate) struct TranscriptPresentedRow {
    pub(crate) index: usize,
    pub(crate) identity: TranscriptRowIdentity,
    pub(crate) source_turn_index: usize,
    pub(crate) turn: Arc<TurnExecutionRecord>,
    pub(crate) placeholder_height: Option<Pixels>,
}

#[allow(dead_code)]
#[derive(Clone, Default)]
pub(crate) struct TranscriptPresentationWindow {
    rows: Vec<TranscriptPresentedRow>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TranscriptPresentationPanelState {
    pub(crate) inspected_row_count: usize,
    pub(crate) active_nested_code_panel_ids: HashSet<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct TranscriptPresentationRetainedCounts {
    pub(crate) rows: usize,
    pub(crate) items: usize,
    pub(crate) text_bytes: usize,
    pub(crate) identity_bytes: usize,
    pub(crate) anchor_bytes: usize,
    pub(crate) placeholder_rows: usize,
}

impl TranscriptPresentationRetainedCounts {
    fn with_anchor_bytes(mut self, anchor_bytes: usize) -> Self {
        self.anchor_bytes = anchor_bytes;
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TranscriptActivityCaret {
    pub(crate) row_index: usize,
    pub(crate) row_identity: TranscriptRowIdentity,
}

impl TranscriptPresentationState {
    pub(crate) fn clear(&mut self) {
        self.rows.clear();
        self.latest_user_prompt_anchor = None;
        self.next_ephemeral_row_id = 0;
    }

    pub(crate) fn replace_from_turns(&mut self, turns: &[Arc<TurnExecutionRecord>]) {
        self.clear();
        let rows = turns
            .iter()
            .cloned()
            .enumerate()
            .filter_map(|(source_turn_index, turn)| self.row_for_turn(source_turn_index, turn))
            .collect::<Vec<_>>();
        self.rows = rows;
        self.rebuild_latest_user_prompt_anchor();
    }

    pub(crate) fn prepend_from_turns(&mut self, turns: &[Arc<TurnExecutionRecord>]) -> usize {
        if turns.is_empty() {
            return 0;
        }

        for row in &mut self.rows {
            row.source_turn_index += turns.len();
        }

        let mut rows = turns
            .iter()
            .cloned()
            .enumerate()
            .filter_map(|(source_turn_index, turn)| self.row_for_turn(source_turn_index, turn))
            .collect::<Vec<_>>();
        let added = rows.len();
        rows.append(&mut self.rows);
        self.rows = rows;
        self.rebuild_latest_user_prompt_anchor();
        added
    }

    pub(crate) fn append_turn(
        &mut self,
        source_turn_index: usize,
        turn: Arc<TurnExecutionRecord>,
    ) -> Option<usize> {
        let index = self.rows.len();
        let row = self.row_for_turn(source_turn_index, turn)?;
        self.rows.push(row);
        self.update_latest_user_prompt_for_replaced_row(index);
        Some(index)
    }

    pub(crate) fn replace_turn(
        &mut self,
        source_turn_index: usize,
        turn: Arc<TurnExecutionRecord>,
    ) -> Option<usize> {
        let row_index = self.presentation_index_for_source_turn(source_turn_index);
        let projected = project_parent_narrative_turn(turn.as_ref()).map(Arc::new);

        match (row_index, projected) {
            (Some(index), Some(turn)) => {
                let row = &mut self.rows[index];
                row.turn = turn;
                row.placeholder_height = None;
                self.update_latest_user_prompt_for_replaced_row(index);
                Some(index)
            }
            (Some(index), None) => {
                self.rows.remove(index);
                self.rebuild_latest_user_prompt_anchor();
                None
            }
            (None, Some(turn)) => {
                let index = self.insertion_index_for_source_turn(source_turn_index);
                let row = self.presentation_row_for_projected_turn(source_turn_index, turn);
                self.rows.insert(index, row);
                self.rebuild_latest_user_prompt_anchor();
                Some(index)
            }
            (None, None) => None,
        }
    }

    pub(crate) fn replace_turn_with_placeholder(
        &mut self,
        source_turn_index: usize,
        turn: Arc<TurnExecutionRecord>,
        placeholder_height: Option<Pixels>,
    ) -> Option<usize> {
        let index = self.presentation_index_for_source_turn(source_turn_index)?;
        let Some(projected) = project_parent_narrative_turn(turn.as_ref()) else {
            self.rows.remove(index);
            self.rebuild_latest_user_prompt_anchor();
            return None;
        };

        let row = &mut self.rows[index];
        row.turn = Arc::new(projected);
        row.placeholder_height = placeholder_height;
        self.update_latest_user_prompt_for_replaced_row(index);
        Some(index)
    }

    pub(crate) fn len(&self) -> usize {
        self.rows.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    pub(crate) fn retained_counts(&self) -> TranscriptPresentationRetainedCounts {
        self.rows
            .iter()
            .fold(
                TranscriptPresentationRetainedCounts {
                    rows: self.rows.len(),
                    ..TranscriptPresentationRetainedCounts::default()
                },
                |mut counts, row| {
                    counts.items = counts.items.saturating_add(row.turn.item_count());
                    counts.text_bytes =
                        counts.text_bytes.saturating_add(row.turn.text_char_count());
                    counts.identity_bytes = counts
                        .identity_bytes
                        .saturating_add(row.identity.as_str().len());
                    counts.placeholder_rows = counts
                        .placeholder_rows
                        .saturating_add(usize::from(row.placeholder_height.is_some()));
                    counts
                },
            )
            .with_anchor_bytes(
                self.latest_user_prompt_anchor
                    .as_ref()
                    .map_or(0, |(_, _, value)| value.len()),
            )
    }

    #[allow(dead_code)]
    pub(crate) fn row_identity(&self, index: usize) -> Option<&TranscriptRowIdentity> {
        self.rows.get(index).map(|row| &row.identity)
    }

    pub(crate) fn row_index_for_identity(&self, identity: &str) -> Option<usize> {
        self.rows
            .iter()
            .position(|row| row.identity.as_str() == identity)
    }

    pub(crate) fn turn_at(&self, index: usize) -> Option<TranscriptPresentedRow> {
        self.rows.get(index).map(|row| row.presented_row_at(index))
    }

    #[allow(dead_code)]
    pub(crate) fn source_turn_index_at(&self, index: usize) -> Option<usize> {
        self.rows.get(index).map(|row| row.source_turn_index)
    }

    pub(crate) fn source_range_for_presentation_range(&self, range: &Range<usize>) -> Range<usize> {
        if self.rows.is_empty() {
            return 0..0;
        }

        let start = range.start.min(self.rows.len());
        let end = range.end.min(self.rows.len()).max(start);
        if start == end {
            let source = if start >= self.rows.len() {
                self.rows
                    .last()
                    .map(|row| row.source_turn_index.saturating_add(1))
                    .unwrap_or_default()
            } else {
                self.rows[start].source_turn_index
            };
            return source..source;
        }

        let source_start = self.rows[start].source_turn_index;
        let source_end = self.rows[end - 1].source_turn_index.saturating_add(1);
        source_start..source_end
    }

    pub(crate) fn presentation_index_for_source_turn(
        &self,
        source_turn_index: usize,
    ) -> Option<usize> {
        self.rows
            .iter()
            .position(|row| row.source_turn_index == source_turn_index)
    }

    #[allow(dead_code)]
    pub(crate) fn window_for_range(&self, range: Range<usize>) -> TranscriptPresentationWindow {
        let start = range.start.min(self.rows.len());
        let end = range.end.min(self.rows.len()).max(start);
        TranscriptPresentationWindow {
            rows: self.rows[start..end]
                .iter()
                .enumerate()
                .map(|(offset, row)| row.presented_row_at(start + offset))
                .collect(),
        }
    }

    pub(crate) fn latest_user_prompt_anchor(&self) -> Option<(usize, usize, String)> {
        self.latest_user_prompt_anchor.clone()
    }

    pub(crate) fn panel_state_for_range(
        &self,
        range: Range<usize>,
    ) -> TranscriptPresentationPanelState {
        let start = range.start.min(self.rows.len());
        let end = range.end.min(self.rows.len()).max(start);
        TranscriptPresentationPanelState {
            inspected_row_count: end.saturating_sub(start),
            ..TranscriptPresentationPanelState::default()
        }
    }

    pub(crate) fn render_metrics(&self) -> TranscriptRenderMetrics {
        let mut metrics = TranscriptRenderMetrics {
            total_turns: self.rows.len(),
            ..TranscriptRenderMetrics::default()
        };
        for row in &self.rows {
            metrics.total_item_count += row.turn.item_count();
            metrics.total_text_chars += row.turn.text_char_count();
        }
        metrics
    }

    pub(crate) fn activity_caret_for_source_turn(
        &self,
        source_turn_index: Option<usize>,
    ) -> Option<TranscriptActivityCaret> {
        let source_turn_index = source_turn_index?;
        let row_index = self.presentation_index_for_source_turn(source_turn_index)?;
        let row = self.rows.get(row_index)?;
        Some(TranscriptActivityCaret {
            row_index,
            row_identity: row.identity.clone(),
        })
    }

    fn insertion_index_for_source_turn(&self, source_turn_index: usize) -> usize {
        self.rows
            .partition_point(|row| row.source_turn_index < source_turn_index)
    }

    fn row_for_turn(
        &mut self,
        source_turn_index: usize,
        turn: Arc<TurnExecutionRecord>,
    ) -> Option<TranscriptPresentationRow> {
        let projected = project_parent_narrative_turn(turn.as_ref())?;
        Some(self.presentation_row_for_projected_turn(source_turn_index, Arc::new(projected)))
    }

    fn presentation_row_for_projected_turn(
        &mut self,
        source_turn_index: usize,
        turn: Arc<TurnExecutionRecord>,
    ) -> TranscriptPresentationRow {
        let identity = self.identity_for_turn(turn.as_ref());
        TranscriptPresentationRow {
            identity,
            source_turn_index,
            turn,
            placeholder_height: None,
        }
    }

    fn identity_for_turn(&mut self, turn: &TurnExecutionRecord) -> TranscriptRowIdentity {
        stable_row_identity(turn).unwrap_or_else(|| {
            let id = self.next_ephemeral_row_id;
            self.next_ephemeral_row_id += 1;
            TranscriptRowIdentity(format!("ephemeral-turn:{id}"))
        })
    }

    fn update_latest_user_prompt_for_replaced_row(&mut self, index: usize) {
        let Some(row) = self.rows.get(index) else {
            return;
        };
        let prompt = user_prompt_anchor_text(row.turn.as_ref());
        match (&mut self.latest_user_prompt_anchor, prompt) {
            (
                Some((latest_index, latest_fragment_index, latest_prompt)),
                Some((fragment_index, prompt)),
            ) if index >= *latest_index => {
                *latest_index = index;
                *latest_fragment_index = fragment_index;
                *latest_prompt = prompt;
            }
            (None, Some((fragment_index, prompt))) => {
                self.latest_user_prompt_anchor = Some((index, fragment_index, prompt));
            }
            (Some((latest_index, _, _)), None) if index == *latest_index => {
                self.rebuild_latest_user_prompt_anchor();
            }
            _ => {}
        }
    }

    fn rebuild_latest_user_prompt_anchor(&mut self) {
        self.latest_user_prompt_anchor = latest_user_prompt_anchor_in_rows(&self.rows);
    }
}

impl TranscriptPresentationWindow {
    #[allow(dead_code)]
    pub(crate) fn rows(&self) -> &[TranscriptPresentedRow] {
        &self.rows
    }

    #[allow(dead_code)]
    pub(crate) fn len(&self) -> usize {
        self.rows.len()
    }

    #[allow(dead_code)]
    pub(crate) fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

impl TranscriptPresentationRow {
    fn presented_row_at(&self, index: usize) -> TranscriptPresentedRow {
        TranscriptPresentedRow {
            index,
            identity: self.identity.clone(),
            source_turn_index: self.source_turn_index,
            turn: self.turn.clone(),
            placeholder_height: self.placeholder_height,
        }
    }
}

pub(crate) fn transcript_frame_presentation_range(
    list_state: &ListState,
    turn_count: usize,
) -> Range<usize> {
    let range = clamp_transcript_range(list_state.presentation_range(), turn_count);
    if range.len() <= TRANSCRIPT_MAX_PRESENTATION_ROWS && (!range.is_empty() || turn_count == 0) {
        return range;
    }

    fallback_transcript_presentation_range(list_state, turn_count)
}

pub(crate) fn transcript_frame_preload_range(
    list_state: &ListState,
    turn_count: usize,
    vertical_margin: Pixels,
) -> Range<usize> {
    let range = clamp_transcript_range(
        list_state.range_with_vertical_margin(vertical_margin),
        turn_count,
    );
    if range.len() <= TRANSCRIPT_MAX_PRESENTATION_ROWS {
        return range;
    }

    let visible = clamp_transcript_range(list_state.visible_range(), turn_count);
    if visible.is_empty() {
        let end = range
            .start
            .saturating_add(TRANSCRIPT_MAX_PRESENTATION_ROWS)
            .min(range.end);
        return range.start..end;
    }

    let extra = TRANSCRIPT_MAX_PRESENTATION_ROWS.saturating_sub(visible.len());
    let before = extra / 2;
    let mut start = visible.start.saturating_sub(before).max(range.start);
    let mut end = start
        .saturating_add(TRANSCRIPT_MAX_PRESENTATION_ROWS)
        .min(range.end);
    start = end
        .saturating_sub(TRANSCRIPT_MAX_PRESENTATION_ROWS)
        .max(range.start);
    end = end.max(start);
    start..end
}

fn fallback_transcript_presentation_range(
    list_state: &ListState,
    turn_count: usize,
) -> Range<usize> {
    match list_state.scroll_position() {
        ListScrollPosition::Content(offset) => {
            let start = offset.item_ix.min(turn_count);
            let end = start
                .saturating_add(TRANSCRIPT_INITIAL_PRESENTATION_ROWS)
                .min(turn_count);
            start..end
        }
        ListScrollPosition::Bottom | ListScrollPosition::VirtualTail { .. } => {
            turn_count.saturating_sub(TRANSCRIPT_INITIAL_PRESENTATION_ROWS)..turn_count
        }
    }
}

fn clamp_transcript_range(range: Range<usize>, turn_count: usize) -> Range<usize> {
    let start = range.start.min(turn_count);
    let end = range.end.min(turn_count).max(start);
    start..end
}

fn stable_row_identity(turn: &TurnExecutionRecord) -> Option<TranscriptRowIdentity> {
    match (turn.thread_id.as_deref(), turn.turn_id.as_deref()) {
        (Some(thread_id), Some(turn_id)) => Some(TranscriptRowIdentity(format!(
            "thread:{thread_id}:turn:{turn_id}"
        ))),
        _ => None,
    }
}

fn latest_user_prompt_anchor_in_rows(
    rows: &[TranscriptPresentationRow],
) -> Option<(usize, usize, String)> {
    rows.iter().enumerate().rev().find_map(|(index, row)| {
        user_prompt_anchor_text(row.turn.as_ref())
            .map(|(fragment_index, prompt)| (index, fragment_index, prompt))
    })
}

fn user_prompt_anchor_text(turn: &TurnExecutionRecord) -> Option<(usize, String)> {
    turn.latest_user_input_fragment()
        .map(|(index, fragment)| (index, fragment.text.clone()))
}
