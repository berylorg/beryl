#![allow(dead_code)]

use std::{
    collections::{BTreeMap, BTreeSet},
    ops::Range,
};

use beryl_backend::{TurnError, TurnInfo, TurnItemsView, TurnStatus};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TranscriptTurnIndexRecord {
    pub(crate) id: String,
    pub(crate) status: TurnStatus,
    pub(crate) items_view: TurnItemsView,
    pub(crate) error: Option<TurnError>,
    history_page_cursor: Option<String>,
    history_page_index: usize,
    history_page_len: usize,
    source_position: usize,
    estimated_resident_bytes: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum TranscriptResidencyPinKind {
    ActiveContextMenu,
    EditTarget,
    MediaActionTarget,
    ActiveTurn,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TranscriptResidencyReleaseCounts {
    pub(crate) resident_turns: usize,
    pub(crate) retained_item_count: usize,
    pub(crate) released_turn_ids: Vec<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct TranscriptResidencyRetainedCounts {
    pub(crate) index_turns: usize,
    pub(crate) nonresident_turns: usize,
    pub(crate) resident_turns: usize,
    pub(crate) pinned_turns: usize,
    pub(crate) retained_item_count: usize,
    pub(crate) resident_bytes: usize,
    pub(crate) index_metadata_bytes: usize,
    pub(crate) in_flight_requests: usize,
    pub(crate) max_resident_turns: usize,
    pub(crate) max_resident_bytes: usize,
    pub(crate) max_in_flight_requests: usize,
    pub(crate) leading_viewport_margins: usize,
    pub(crate) trailing_viewport_margins: usize,
    pub(crate) cold_release_hysteresis_viewports: usize,
    pub(crate) max_resident_pages: usize,
    pub(crate) max_released_pages: usize,
    pub(crate) request_priority: TranscriptResidencyRequestPriority,
    pub(crate) budget_reason: TranscriptResidencyBudgetReason,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum TranscriptResidencyRequestPriority {
    #[default]
    ProvidedOrder,
    OldestFirst,
    NewestFirst,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum TranscriptResidencyBudgetReason {
    #[default]
    None,
    ResidentTurnLimit,
    ResidentByteLimit,
    InFlightRequestLimit,
    PinnedResidentOverBudget,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TranscriptResidencyRetention {
    turn_ids: BTreeSet<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TranscriptResidencyPolicy {
    max_resident_turns: usize,
    max_resident_bytes: usize,
    max_in_flight_requests: usize,
    leading_viewport_margins: usize,
    trailing_viewport_margins: usize,
    cold_release_hysteresis_viewports: usize,
    minimum_request_boundary_rows: usize,
    minimum_restore_margin_rows: usize,
    max_resident_pages: usize,
    max_released_pages: usize,
    request_priority: TranscriptResidencyRequestPriority,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct TranscriptResidencyStats {
    index_turns: usize,
    nonresident_turns: usize,
    resident_turns: usize,
    pinned_turns: usize,
    retained_item_count: usize,
    resident_bytes: usize,
    index_metadata_bytes: usize,
    in_flight_requests: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TranscriptResidencyState {
    thread_id: Option<String>,
    generation: u64,
    entries: BTreeMap<String, TranscriptResidencyEntry>,
    ordered_turn_ids: Vec<String>,
    policy: TranscriptResidencyPolicy,
    stats: TranscriptResidencyStats,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TranscriptResidencyEntry {
    index: Option<TranscriptTurnIndexRecord>,
    resident: Option<TranscriptResidentTurn>,
    state: TranscriptResidencyEntryState,
    pins: BTreeSet<TranscriptResidencyPinKind>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TranscriptResidentTurn {
    turn: TurnInfo,
    item_count: usize,
    resident_bytes: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum TranscriptResidencyEntryState {
    Missing,
    Full {
        item_count: usize,
        resident_bytes: usize,
    },
}

const DEFAULT_MAX_RESIDENT_TURNS: usize = 320;
const DEFAULT_MAX_RESIDENT_BYTES: usize = 100 * 1024 * 1024;
const DEFAULT_MAX_IN_FLIGHT_REQUESTS: usize = 1;
const DEFAULT_LEADING_VIEWPORT_MARGINS: usize = 1;
const DEFAULT_TRAILING_VIEWPORT_MARGINS: usize = 1;
const DEFAULT_COLD_RELEASE_HYSTERESIS_VIEWPORTS: usize = 1;
const DEFAULT_MINIMUM_REQUEST_BOUNDARY_ROWS: usize = 2;
const DEFAULT_MINIMUM_RESTORE_MARGIN_ROWS: usize = 8;
const DEFAULT_MAX_RESIDENT_PAGES: usize = 4;
const DEFAULT_MAX_RELEASED_PAGES: usize = 32;

impl Default for TranscriptResidencyPolicy {
    fn default() -> Self {
        Self {
            max_resident_turns: DEFAULT_MAX_RESIDENT_TURNS,
            max_resident_bytes: DEFAULT_MAX_RESIDENT_BYTES,
            max_in_flight_requests: DEFAULT_MAX_IN_FLIGHT_REQUESTS,
            leading_viewport_margins: DEFAULT_LEADING_VIEWPORT_MARGINS,
            trailing_viewport_margins: DEFAULT_TRAILING_VIEWPORT_MARGINS,
            cold_release_hysteresis_viewports: DEFAULT_COLD_RELEASE_HYSTERESIS_VIEWPORTS,
            minimum_request_boundary_rows: DEFAULT_MINIMUM_REQUEST_BOUNDARY_ROWS,
            minimum_restore_margin_rows: DEFAULT_MINIMUM_RESTORE_MARGIN_ROWS,
            max_resident_pages: DEFAULT_MAX_RESIDENT_PAGES,
            max_released_pages: DEFAULT_MAX_RELEASED_PAGES,
            request_priority: TranscriptResidencyRequestPriority::ProvidedOrder,
        }
    }
}

impl TranscriptResidencyPolicy {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn with_max_resident_turns(mut self, max_resident_turns: usize) -> Self {
        self.max_resident_turns = max_resident_turns;
        self
    }

    pub(crate) fn with_max_resident_bytes(mut self, max_resident_bytes: usize) -> Self {
        self.max_resident_bytes = max_resident_bytes;
        self
    }

    pub(crate) fn with_max_in_flight_requests(mut self, max_in_flight_requests: usize) -> Self {
        self.max_in_flight_requests = max_in_flight_requests;
        self
    }

    pub(crate) fn with_leading_viewport_margins(mut self, margins: usize) -> Self {
        self.leading_viewport_margins = margins;
        self
    }

    pub(crate) fn with_trailing_viewport_margins(mut self, margins: usize) -> Self {
        self.trailing_viewport_margins = margins;
        self
    }

    pub(crate) fn with_cold_release_hysteresis_viewports(mut self, viewports: usize) -> Self {
        self.cold_release_hysteresis_viewports = viewports;
        self
    }

    pub(crate) fn with_minimum_request_boundary_rows(mut self, rows: usize) -> Self {
        self.minimum_request_boundary_rows = rows;
        self
    }

    pub(crate) fn with_minimum_restore_margin_rows(mut self, rows: usize) -> Self {
        self.minimum_restore_margin_rows = rows;
        self
    }

    pub(crate) fn with_max_resident_pages(mut self, max_resident_pages: usize) -> Self {
        self.max_resident_pages = max_resident_pages;
        self
    }

    pub(crate) fn with_max_released_pages(mut self, max_released_pages: usize) -> Self {
        self.max_released_pages = max_released_pages;
        self
    }

    pub(crate) fn with_request_priority(
        mut self,
        request_priority: TranscriptResidencyRequestPriority,
    ) -> Self {
        self.request_priority = request_priority;
        self
    }

    pub(crate) fn max_resident_turns(&self) -> usize {
        self.max_resident_turns
    }

    pub(crate) fn max_resident_bytes(&self) -> usize {
        self.max_resident_bytes
    }

    pub(crate) fn max_in_flight_requests(&self) -> usize {
        self.max_in_flight_requests
    }

    pub(crate) fn leading_viewport_margins(&self) -> usize {
        self.leading_viewport_margins
    }

    pub(crate) fn trailing_viewport_margins(&self) -> usize {
        self.trailing_viewport_margins
    }

    pub(crate) fn cold_release_hysteresis_viewports(&self) -> usize {
        self.cold_release_hysteresis_viewports
    }

    pub(crate) fn max_resident_pages(&self) -> usize {
        self.max_resident_pages
    }

    pub(crate) fn max_released_pages(&self) -> usize {
        self.max_released_pages
    }

    pub(crate) fn request_priority(&self) -> TranscriptResidencyRequestPriority {
        self.request_priority
    }

    pub(crate) fn request_priority_label(&self) -> &'static str {
        self.request_priority.label()
    }

    pub(crate) fn older_request_boundary_rows(&self, visible_range: &Range<usize>) -> usize {
        margin_rows(
            visible_range,
            self.leading_viewport_margins,
            self.minimum_request_boundary_rows,
        )
    }

    pub(crate) fn restore_margin_rows(&self, visible_range: &Range<usize>) -> usize {
        margin_rows(
            visible_range,
            self.leading_viewport_margins
                .max(self.trailing_viewport_margins),
            self.minimum_restore_margin_rows,
        )
    }

    pub(crate) fn cold_release_margin_rows(&self, visible_range: &Range<usize>) -> usize {
        margin_rows(
            visible_range,
            self.leading_viewport_margins
                .max(self.trailing_viewport_margins)
                .saturating_add(self.cold_release_hysteresis_viewports),
            self.minimum_restore_margin_rows,
        )
    }

    pub(crate) fn retention_range_for_visible_range(
        &self,
        visible_range: Range<usize>,
        turn_count: usize,
    ) -> Range<usize> {
        if visible_range.is_empty() {
            return visible_range.start.min(turn_count)..visible_range.end.min(turn_count);
        }
        let leading = margin_rows(&visible_range, self.leading_viewport_margins, 0);
        let trailing = margin_rows(&visible_range, self.trailing_viewport_margins, 0);
        let start = visible_range.start.saturating_sub(leading).min(turn_count);
        let end = visible_range
            .end
            .saturating_add(trailing)
            .min(turn_count)
            .max(start);
        start..end
    }
}

impl TranscriptResidencyRequestPriority {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::ProvidedOrder => "provided_order",
            Self::OldestFirst => "oldest_first",
            Self::NewestFirst => "newest_first",
        }
    }
}

impl TranscriptResidencyBudgetReason {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::ResidentTurnLimit => "resident_turn_limit",
            Self::ResidentByteLimit => "resident_byte_limit",
            Self::InFlightRequestLimit => "in_flight_request_limit",
            Self::PinnedResidentOverBudget => "pinned_resident_over_budget",
        }
    }
}

impl TranscriptTurnIndexRecord {
    pub(crate) fn from_turn_in_history_page(
        turn: &TurnInfo,
        history_page_cursor: Option<String>,
        history_page_index: usize,
        history_page_len: usize,
        source_position: usize,
    ) -> Self {
        Self {
            id: turn.id.clone(),
            status: turn.status,
            items_view: turn.items_view,
            error: turn.error.clone(),
            history_page_cursor,
            history_page_index,
            history_page_len,
            source_position,
            estimated_resident_bytes: estimate_turn_resident_bytes(turn),
        }
    }

    fn metadata_bytes(&self) -> usize {
        self.id
            .len()
            .saturating_add(self.history_page_cursor.as_ref().map_or(0, String::len))
            .saturating_add(self.error.as_ref().map_or(0, |error| {
                error
                    .message
                    .len()
                    .saturating_add(error.additional_details.as_ref().map_or(0, String::len))
            }))
            .saturating_add(std::mem::size_of::<Self>())
    }
}

impl TranscriptResidencyRetention {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn from_turn_ids<I, S>(turn_ids: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut retention = Self::new();
        retention.include_turn_ids(turn_ids);
        retention
    }

    pub(crate) fn from_visible_range(
        ordered_turn_ids: &[String],
        visible_range: Range<usize>,
        overscan: usize,
    ) -> Self {
        let mut retention = Self::new();
        retention.include_visible_range(ordered_turn_ids, visible_range, overscan);
        retention
    }

    pub(crate) fn include_visible_range(
        &mut self,
        ordered_turn_ids: &[String],
        visible_range: Range<usize>,
        overscan: usize,
    ) {
        let start = visible_range.start.saturating_sub(overscan);
        let end = visible_range
            .end
            .saturating_add(overscan)
            .min(ordered_turn_ids.len())
            .max(start.min(ordered_turn_ids.len()));
        for turn_id in &ordered_turn_ids[start.min(ordered_turn_ids.len())..end] {
            self.turn_ids.insert(turn_id.clone());
        }
    }

    pub(crate) fn include_turn_id(&mut self, turn_id: impl Into<String>) {
        self.turn_ids.insert(turn_id.into());
    }

    pub(crate) fn include_turn_ids<I, S>(&mut self, turn_ids: I)
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        for turn_id in turn_ids {
            self.turn_ids.insert(turn_id.as_ref().to_string());
        }
    }

    pub(crate) fn contains(&self, turn_id: &str) -> bool {
        self.turn_ids.contains(turn_id)
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = &str> {
        self.turn_ids.iter().map(String::as_str)
    }

    pub(crate) fn len(&self) -> usize {
        self.turn_ids.len()
    }
}

impl TranscriptResidencyEntry {
    fn missing() -> Self {
        Self {
            index: None,
            resident: None,
            state: TranscriptResidencyEntryState::Missing,
            pins: BTreeSet::new(),
        }
    }

    fn is_pinned(&self) -> bool {
        !self.pins.is_empty()
    }

    fn cleanup_candidate(&self) -> bool {
        self.index.is_none()
            && self.resident.is_none()
            && self.pins.is_empty()
            && matches!(self.state, TranscriptResidencyEntryState::Missing)
    }
}

impl Default for TranscriptResidencyState {
    fn default() -> Self {
        Self {
            thread_id: None,
            generation: 0,
            entries: BTreeMap::new(),
            ordered_turn_ids: Vec::new(),
            policy: TranscriptResidencyPolicy::default(),
            stats: TranscriptResidencyStats::default(),
        }
    }
}

impl TranscriptResidencyState {
    pub(crate) fn reset_for_thread(&mut self, thread_id: impl Into<String>) {
        self.thread_id = Some(thread_id.into());
        self.generation = self.generation.saturating_add(1);
        self.entries.clear();
        self.ordered_turn_ids.clear();
        self.stats = TranscriptResidencyStats::default();
    }

    pub(crate) fn bind_thread(&mut self, thread_id: impl Into<String>) {
        self.thread_id = Some(thread_id.into());
        self.generation = self.generation.saturating_add(1);
    }

    pub(crate) fn clear(&mut self) {
        self.thread_id = None;
        self.generation = self.generation.saturating_add(1);
        self.entries.clear();
        self.ordered_turn_ids.clear();
        self.stats = TranscriptResidencyStats::default();
    }

    pub(crate) fn set_policy(&mut self, policy: TranscriptResidencyPolicy) {
        self.policy = policy;
        self.enforce_policy_budget(&TranscriptResidencyRetention::new());
    }

    pub(crate) fn set_in_flight_requests(&mut self, in_flight_requests: usize) {
        self.stats.in_flight_requests = in_flight_requests;
    }

    pub(crate) fn policy(&self) -> &TranscriptResidencyPolicy {
        &self.policy
    }

    pub(crate) fn thread_id(&self) -> Option<&str> {
        self.thread_id.as_deref()
    }

    pub(crate) fn index_history_page(
        &mut self,
        turns: &[TurnInfo],
        history_page_cursor: Option<&str>,
        source_start: usize,
    ) {
        let history_page_len = turns.len();
        for (history_page_index, turn) in turns.iter().enumerate() {
            let record = TranscriptTurnIndexRecord::from_turn_in_history_page(
                turn,
                history_page_cursor.map(str::to_string),
                history_page_index,
                history_page_len,
                source_start.saturating_add(history_page_index),
            );
            self.insert_index_record(record);
            if turn.items_view == TurnItemsView::Full {
                self.admit_resident_turn(turn.clone());
            }
        }
    }

    pub(crate) fn shift_source_positions(&mut self, amount: usize) {
        if amount == 0 {
            return;
        }
        for entry in self.entries.values_mut() {
            if let Some(index) = entry.index.as_mut() {
                index.source_position = index.source_position.saturating_add(amount);
            }
        }
    }

    pub(crate) fn admit_resident_turns<I>(&mut self, turns: I) -> Vec<String>
    where
        I: IntoIterator<Item = TurnInfo>,
    {
        let mut admitted = Vec::new();
        for turn in turns {
            if turn.items_view != TurnItemsView::Full {
                continue;
            }
            let turn_id = turn.id.clone();
            self.admit_resident_turn(turn);
            admitted.push(turn_id);
        }
        admitted
    }

    pub(crate) fn resident_turn(&self, turn_id: &str) -> Option<&TurnInfo> {
        self.entries
            .get(turn_id)
            .and_then(|entry| entry.resident.as_ref())
            .map(|resident| &resident.turn)
    }

    pub(crate) fn resident_turn_ids(&self) -> Vec<String> {
        self.ordered_turn_ids
            .iter()
            .filter(|turn_id| {
                self.entries
                    .get(turn_id.as_str())
                    .is_some_and(|entry| entry.resident.is_some())
            })
            .cloned()
            .collect()
    }

    pub(crate) fn full_item_count(&self, turn_id: &str) -> Option<usize> {
        match &self.entries.get(turn_id)?.state {
            TranscriptResidencyEntryState::Full { item_count, .. } => Some(*item_count),
            _ => None,
        }
    }

    pub(crate) fn pin_turn(&mut self, turn_id: &str, kind: TranscriptResidencyPinKind) {
        let before = self.entry_stats_for(turn_id);
        self.entry_mut(turn_id).pins.insert(kind);
        let after = self.entry_stats_for(turn_id);
        self.apply_entry_stats_delta(before, after);
    }

    pub(crate) fn unpin_turn(&mut self, turn_id: &str, kind: TranscriptResidencyPinKind) {
        let before = self.entry_stats_for(turn_id);
        let should_remove = if let Some(entry) = self.entries.get_mut(turn_id) {
            entry.pins.remove(&kind);
            entry.cleanup_candidate()
        } else {
            false
        };
        if should_remove {
            self.entries.remove(turn_id);
            self.ordered_turn_ids
                .retain(|ordered_turn_id| ordered_turn_id != turn_id);
        }
        let after = self.entry_stats_for(turn_id);
        self.apply_entry_stats_delta(before, after);
    }

    pub(crate) fn replace_pins<I, S>(&mut self, kind: TranscriptResidencyPinKind, turn_ids: I)
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut empty_entries = Vec::new();
        let mut deltas = Vec::new();
        for (turn_id, entry) in &mut self.entries {
            let before = Self::entry_stats(entry);
            entry.pins.remove(&kind);
            let after = if entry.cleanup_candidate() {
                TranscriptResidencyStats::default()
            } else {
                Self::entry_stats(entry)
            };
            deltas.push((before, after));
            if entry.cleanup_candidate() {
                empty_entries.push(turn_id.clone());
            }
        }
        for (before, after) in deltas {
            self.apply_entry_stats_delta(before, after);
        }
        for turn_id in empty_entries {
            self.entries.remove(&turn_id);
            self.ordered_turn_ids
                .retain(|ordered_turn_id| ordered_turn_id != &turn_id);
        }
        for turn_id in turn_ids {
            self.pin_turn(turn_id.as_ref(), kind);
        }
    }

    pub(crate) fn release_unretained_resident_turns(
        &mut self,
        retention: &TranscriptResidencyRetention,
    ) -> TranscriptResidencyReleaseCounts {
        let mut released = TranscriptResidencyReleaseCounts::default();
        let turn_ids = self.entries.keys().cloned().collect::<Vec<_>>();

        for turn_id in turn_ids {
            let Some(entry) = self.entries.get_mut(turn_id.as_str()) else {
                continue;
            };
            if retention.contains(turn_id.as_str()) || entry.is_pinned() {
                continue;
            }

            let before = Self::entry_stats(entry);
            match std::mem::replace(&mut entry.state, TranscriptResidencyEntryState::Missing) {
                TranscriptResidencyEntryState::Missing => {}
                TranscriptResidencyEntryState::Full { item_count, .. } => {
                    released.resident_turns = released.resident_turns.saturating_add(1);
                    released.retained_item_count =
                        released.retained_item_count.saturating_add(item_count);
                    released.released_turn_ids.push(turn_id.clone());
                    entry.resident = None;
                }
            }
            let after = Self::entry_stats(entry);
            self.apply_entry_stats_delta(before, after);
        }

        self.entries.retain(|_, entry| !entry.cleanup_candidate());
        self.ordered_turn_ids
            .retain(|turn_id| self.entries.contains_key(turn_id));
        released
    }

    pub(crate) fn retained_counts(&self) -> TranscriptResidencyRetainedCounts {
        let mut counts = TranscriptResidencyRetainedCounts {
            index_turns: self.stats.index_turns,
            nonresident_turns: self.stats.nonresident_turns,
            resident_turns: self.stats.resident_turns,
            pinned_turns: self.stats.pinned_turns,
            retained_item_count: self.stats.retained_item_count,
            resident_bytes: self.stats.resident_bytes,
            index_metadata_bytes: self.stats.index_metadata_bytes,
            in_flight_requests: self.stats.in_flight_requests,
            ..TranscriptResidencyRetainedCounts::default()
        };
        counts.max_resident_turns = self.policy.max_resident_turns;
        counts.max_resident_bytes = self.policy.max_resident_bytes;
        counts.max_in_flight_requests = self.policy.max_in_flight_requests;
        counts.leading_viewport_margins = self.policy.leading_viewport_margins;
        counts.trailing_viewport_margins = self.policy.trailing_viewport_margins;
        counts.cold_release_hysteresis_viewports = self.policy.cold_release_hysteresis_viewports;
        counts.max_resident_pages = self.policy.max_resident_pages;
        counts.max_released_pages = self.policy.max_released_pages;
        counts.request_priority = self.policy.request_priority;
        counts.budget_reason = self.current_budget_reason(&counts);
        counts
    }

    fn entry_stats_for(&self, turn_id: &str) -> TranscriptResidencyStats {
        self.entries
            .get(turn_id)
            .map(Self::entry_stats)
            .unwrap_or_default()
    }

    fn entry_stats(entry: &TranscriptResidencyEntry) -> TranscriptResidencyStats {
        let mut stats = TranscriptResidencyStats {
            index_turns: usize::from(entry.index.is_some()),
            pinned_turns: usize::from(entry.is_pinned()),
            index_metadata_bytes: entry
                .index
                .as_ref()
                .map_or(0, TranscriptTurnIndexRecord::metadata_bytes),
            ..TranscriptResidencyStats::default()
        };
        match &entry.state {
            TranscriptResidencyEntryState::Missing => {
                stats.nonresident_turns = 1;
            }
            TranscriptResidencyEntryState::Full {
                item_count,
                resident_bytes,
            } => {
                stats.resident_turns = 1;
                stats.retained_item_count = *item_count;
                stats.resident_bytes = *resident_bytes;
            }
        }
        stats
    }

    fn apply_entry_stats_delta(
        &mut self,
        before: TranscriptResidencyStats,
        after: TranscriptResidencyStats,
    ) {
        self.stats.index_turns = apply_stats_delta(
            self.stats.index_turns,
            before.index_turns,
            after.index_turns,
        );
        self.stats.nonresident_turns = apply_stats_delta(
            self.stats.nonresident_turns,
            before.nonresident_turns,
            after.nonresident_turns,
        );
        self.stats.resident_turns = apply_stats_delta(
            self.stats.resident_turns,
            before.resident_turns,
            after.resident_turns,
        );
        self.stats.pinned_turns = apply_stats_delta(
            self.stats.pinned_turns,
            before.pinned_turns,
            after.pinned_turns,
        );
        self.stats.retained_item_count = apply_stats_delta(
            self.stats.retained_item_count,
            before.retained_item_count,
            after.retained_item_count,
        );
        self.stats.resident_bytes = apply_stats_delta(
            self.stats.resident_bytes,
            before.resident_bytes,
            after.resident_bytes,
        );
        self.stats.index_metadata_bytes = apply_stats_delta(
            self.stats.index_metadata_bytes,
            before.index_metadata_bytes,
            after.index_metadata_bytes,
        );
    }

    fn insert_index_record(&mut self, record: TranscriptTurnIndexRecord) {
        let turn_id = record.id.clone();
        if !self.ordered_turn_ids.iter().any(|id| id == &turn_id) {
            self.ordered_turn_ids.push(turn_id.clone());
            self.ordered_turn_ids.sort_by_key(|id| {
                self.entries
                    .get(id)
                    .and_then(|entry| entry.index.as_ref())
                    .map_or(record.source_position, |index| index.source_position)
            });
        }
        let before = self.entry_stats_for(&turn_id);
        let entry = self.entry_mut(&turn_id);
        entry.index = Some(record);
        let after = self.entry_stats_for(&turn_id);
        self.apply_entry_stats_delta(before, after);
    }

    fn admit_resident_turn(&mut self, turn: TurnInfo) {
        if turn.items_view != TurnItemsView::Full {
            return;
        }
        let turn_id = turn.id.clone();
        if !self.ordered_turn_ids.iter().any(|id| id == &turn_id) {
            self.ordered_turn_ids.push(turn_id.clone());
        }
        let resident_bytes = estimate_turn_resident_bytes(&turn);
        let item_count = turn.items.len();
        let before = self.entry_stats_for(&turn_id);
        let entry = self.entry_mut(&turn_id);
        if entry.index.is_none() {
            entry.index = Some(TranscriptTurnIndexRecord::from_turn_in_history_page(
                &turn, None, 0, 1, 0,
            ));
        }
        entry.resident = Some(TranscriptResidentTurn {
            turn,
            item_count,
            resident_bytes,
        });
        entry.state = TranscriptResidencyEntryState::Full {
            item_count,
            resident_bytes,
        };
        let after = self.entry_stats_for(&turn_id);
        self.apply_entry_stats_delta(before, after);
    }

    fn enforce_policy_budget(&mut self, retention: &TranscriptResidencyRetention) {
        let _ = self.release_budget_excess(retention);
    }

    fn release_budget_excess(
        &mut self,
        retention: &TranscriptResidencyRetention,
    ) -> TranscriptResidencyReleaseCounts {
        let mut released = TranscriptResidencyReleaseCounts::default();
        while self.resident_turn_count() > self.policy.max_resident_turns
            || self.resident_bytes() > self.policy.max_resident_bytes
        {
            let Some(turn_id) = self
                .ordered_turn_ids
                .iter()
                .find(|turn_id| {
                    self.entries.get(turn_id.as_str()).is_some_and(|entry| {
                        entry.resident.is_some()
                            && !entry.is_pinned()
                            && !retention.contains(turn_id.as_str())
                    })
                })
                .cloned()
            else {
                break;
            };
            self.release_one_resident_turn(&turn_id, &mut released);
        }
        released
    }

    fn release_one_resident_turn(
        &mut self,
        turn_id: &str,
        released: &mut TranscriptResidencyReleaseCounts,
    ) {
        let Some(entry) = self.entries.get_mut(turn_id) else {
            return;
        };
        let before = Self::entry_stats(entry);
        match std::mem::replace(&mut entry.state, TranscriptResidencyEntryState::Missing) {
            TranscriptResidencyEntryState::Missing => {}
            TranscriptResidencyEntryState::Full { item_count, .. } => {
                released.resident_turns = released.resident_turns.saturating_add(1);
                released.retained_item_count =
                    released.retained_item_count.saturating_add(item_count);
                released.released_turn_ids.push(turn_id.to_string());
            }
        }
        entry.resident = None;
        let after = Self::entry_stats(entry);
        self.apply_entry_stats_delta(before, after);
    }

    fn resident_turn_count(&self) -> usize {
        self.stats.resident_turns
    }

    fn resident_bytes(&self) -> usize {
        self.stats.resident_bytes
    }

    fn current_budget_reason(
        &self,
        counts: &TranscriptResidencyRetainedCounts,
    ) -> TranscriptResidencyBudgetReason {
        let resident_over_turn_budget = counts.resident_turns > self.policy.max_resident_turns;
        let resident_over_byte_budget = counts.resident_bytes > self.policy.max_resident_bytes;
        if (resident_over_turn_budget || resident_over_byte_budget) && counts.pinned_turns > 0 {
            return TranscriptResidencyBudgetReason::PinnedResidentOverBudget;
        }
        if resident_over_turn_budget {
            return TranscriptResidencyBudgetReason::ResidentTurnLimit;
        }
        if resident_over_byte_budget {
            return TranscriptResidencyBudgetReason::ResidentByteLimit;
        }
        if counts.in_flight_requests > 0
            && counts.in_flight_requests >= self.policy.max_in_flight_requests.max(1)
        {
            return TranscriptResidencyBudgetReason::InFlightRequestLimit;
        }
        TranscriptResidencyBudgetReason::None
    }

    fn entry_mut(&mut self, turn_id: &str) -> &mut TranscriptResidencyEntry {
        self.entries
            .entry(turn_id.to_string())
            .or_insert_with(TranscriptResidencyEntry::missing)
    }
}

fn margin_rows(range: &Range<usize>, viewport_margins: usize, minimum_rows: usize) -> usize {
    range
        .len()
        .max(1)
        .saturating_mul(viewport_margins)
        .max(minimum_rows)
}

fn apply_stats_delta(current: usize, before: usize, after: usize) -> usize {
    current.saturating_sub(before).saturating_add(after)
}

fn estimate_turn_resident_bytes(turn: &TurnInfo) -> usize {
    turn.id
        .len()
        .saturating_add(std::mem::size_of::<TurnInfo>())
        .saturating_add(turn.error.as_ref().map_or(0, |error| {
            error
                .message
                .len()
                .saturating_add(error.additional_details.as_ref().map_or(0, String::len))
        }))
        .saturating_add(
            turn.items
                .iter()
                .map(estimate_thread_item_resident_bytes)
                .sum::<usize>(),
        )
}

fn estimate_thread_item_resident_bytes(item: &beryl_backend::ThreadItem) -> usize {
    match item {
        beryl_backend::ThreadItem::UserMessage(message) => std::mem::size_of_val(message)
            .saturating_add(
                message
                    .content
                    .iter()
                    .map(estimate_user_input_resident_bytes)
                    .sum::<usize>(),
            ),
        beryl_backend::ThreadItem::AgentMessage(message) => {
            std::mem::size_of_val(message).saturating_add(message.text.len())
        }
        beryl_backend::ThreadItem::Reasoning(reasoning) => std::mem::size_of_val(reasoning)
            .saturating_add(reasoning.summary.iter().map(String::len).sum::<usize>())
            .saturating_add(reasoning.content.iter().map(String::len).sum::<usize>())
            .saturating_add(reasoning.id.len()),
        beryl_backend::ThreadItem::CommandExecution(command) => std::mem::size_of_val(command)
            .saturating_add(command.id.len())
            .saturating_add(command.command.len())
            .saturating_add(command.cwd.len())
            .saturating_add(command.process_id.as_ref().map_or(0, String::len))
            .saturating_add(command.aggregated_output.as_ref().map_or(0, String::len)),
        beryl_backend::ThreadItem::FileChange(file_change) => std::mem::size_of_val(file_change)
            .saturating_add(file_change.id.len())
            .saturating_add(
                file_change
                    .changes
                    .iter()
                    .map(|change| {
                        change
                            .path
                            .to_string_lossy()
                            .len()
                            .saturating_add(change.diff.len())
                    })
                    .sum::<usize>(),
            ),
        beryl_backend::ThreadItem::ImageGeneration(image) => std::mem::size_of_val(image)
            .saturating_add(image.id.len())
            .saturating_add(image.status.as_ref().map_or(0, String::len))
            .saturating_add(image.revised_prompt.as_ref().map_or(0, String::len))
            .saturating_add(image.result.as_ref().map_or(0, String::len))
            .saturating_add(image.saved_path.as_ref().map_or(0, String::len)),
        beryl_backend::ThreadItem::Generic(generic) => std::mem::size_of_val(generic)
            .saturating_add(generic.id.len())
            .saturating_add(generic.item_type.len())
            .saturating_add(generic.tool.as_ref().map_or(0, String::len))
            .saturating_add(generic.server.as_ref().map_or(0, String::len))
            .saturating_add(generic.namespace.as_ref().map_or(0, String::len))
            .saturating_add(generic.status.as_ref().map_or(0, String::len))
            .saturating_add(generic.model.as_ref().map_or(0, String::len)),
    }
}

fn estimate_user_input_resident_bytes(input: &beryl_backend::UserInput) -> usize {
    match input {
        beryl_backend::UserInput::Text { text } => text.len(),
        beryl_backend::UserInput::Image { url } => url.len(),
        beryl_backend::UserInput::LocalImage { path } => path.len(),
        beryl_backend::UserInput::Skill { name, path }
        | beryl_backend::UserInput::Mention { name, path } => name.len().saturating_add(path.len()),
    }
}
