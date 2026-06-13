use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    ops::Range,
    time::Duration,
};

use beryl_backend::{
    ManagedBackendError, ManagedBackendSession, SortDirection, ThreadTurnsListOptions,
    ThreadTurnsListResponse, TurnInfo, TurnItemsView,
};

#[path = "transcript_history/residency.rs"]
mod residency;
#[path = "transcript_history/residency_plan.rs"]
mod residency_plan;
#[path = "transcript_fallback.rs"]
mod transcript_fallback;

#[allow(unused_imports)]
pub(crate) use residency::{
    TranscriptResidencyBudgetReason, TranscriptResidencyIndexedTurn, TranscriptResidencyPinKind,
    TranscriptResidencyPolicy, TranscriptResidencyReleaseCounts,
    TranscriptResidencyRequestPriority, TranscriptResidencyRetainedCounts,
    TranscriptResidencyRetention, TranscriptResidencyState, TranscriptTurnIndexRecord,
    estimate_turn_payload_resident_bytes,
};
#[allow(unused_imports)]
pub(crate) use residency_plan::{
    TranscriptResidencyGrowthStrategy, TranscriptResidencyTargetDiagnostics,
    TranscriptResidencyTargetInput, TranscriptResidencyTargetPlan, TranscriptResidencyTargetPolicy,
    TranscriptResidencyTurnPlanInput, TranscriptResidencyViewport,
    plan_transcript_residency_target,
};
pub(crate) use transcript_fallback::is_oversized_turn_fallback_marker;

pub(crate) const THREAD_HISTORY_PAGE_LIMIT: u32 = 80;
const INITIAL_THREAD_ACTIVATION_VIEWPORT_ROWS: usize = 8;
pub(crate) const TRANSCRIPT_RESIDENCY_ESTIMATED_ROW_HEIGHT: usize = 96;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TranscriptHistoryWindow {
    older_cursor: Option<String>,
    newer_cursor: Option<String>,
    loading_page: Option<LoadingTranscriptHistoryPage>,
    pages: Vec<TranscriptHistoryPageState>,
    residency: TranscriptResidencyState,
    next_page_id: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LoadedTranscriptHistoryPage {
    pub turns: Vec<TurnInfo>,
    pub older_cursor: Option<String>,
    pub newer_cursor: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TranscriptTurnAdmissionPlan {
    pub(crate) resident_turn_ids: Vec<String>,
    pub(crate) oversized_turn_fallback_ids: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TranscriptResidencyMeasuredTurnHeight {
    pub(crate) source_position: usize,
    pub(crate) measured_height: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct TranscriptResidencyStreamedTurnFill {
    pub(crate) source_position: usize,
    pub(crate) leading_margin_satisfied: bool,
    pub(crate) trailing_margin_satisfied: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TranscriptResidentPageValidationError {
    turn_id: String,
    items_view: TurnItemsView,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TranscriptResidentHistoryPageError<E> {
    Backend(E),
    Incomplete(TranscriptResidentPageValidationError),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TranscriptHistoryPageId(u64);

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum TranscriptHistoryPageRequest {
    Older {
        cursor: String,
    },
    Indexed {
        page_id: TranscriptHistoryPageId,
        cursor: Option<String>,
    },
    Released {
        page_id: TranscriptHistoryPageId,
        cursor: Option<String>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TranscriptHistoryPageRelease {
    pub page_id: TranscriptHistoryPageId,
    pub range: Range<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RestoredTranscriptHistoryPage {
    pub range: Range<usize>,
    pub turn_ids: Vec<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct TranscriptHistoryRetainedCounts {
    pub(crate) pages: usize,
    pub(crate) resident_pages: usize,
    pub(crate) released_pages: usize,
    pub(crate) loading_pages: usize,
    pub(crate) pinned_pages: usize,
    pub(crate) turn_ids: usize,
    pub(crate) turn_id_bytes: usize,
    pub(crate) cursor_bytes: usize,
    pub(crate) metadata_bytes: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct TranscriptHistoryBoundaryState {
    pub(crate) can_request_older: bool,
    pub(crate) released_page_near: bool,
    pub(crate) loading_page: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TranscriptHistoryPageState {
    id: TranscriptHistoryPageId,
    start: usize,
    len: usize,
    turn_ids: Vec<String>,
    load_cursor: Option<String>,
    older_cursor: Option<String>,
    newer_cursor: Option<String>,
    resident: bool,
    pinned: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum LoadingTranscriptHistoryPage {
    Older { cursor: String },
    Indexed { page_id: TranscriptHistoryPageId },
    Released { page_id: TranscriptHistoryPageId },
}

pub(crate) trait TranscriptHistoryBackend {
    type Error: fmt::Display;

    fn list_thread_turns(
        &mut self,
        thread_id: &str,
        options: &ThreadTurnsListOptions,
        timeout: Duration,
    ) -> Result<ThreadTurnsListResponse, Self::Error>;
}

impl TranscriptHistoryBackend for ManagedBackendSession {
    type Error = ManagedBackendError;

    fn list_thread_turns(
        &mut self,
        thread_id: &str,
        options: &ThreadTurnsListOptions,
        timeout: Duration,
    ) -> Result<ThreadTurnsListResponse, Self::Error> {
        ManagedBackendSession::list_thread_turns(self, thread_id, options, timeout)
    }
}

impl fmt::Display for TranscriptResidentPageValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "full history request returned turn {} with itemsView {:?}",
            self.turn_id, self.items_view
        )
    }
}

impl<E> fmt::Display for TranscriptResidentHistoryPageError<E>
where
    E: fmt::Display,
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Backend(error) => write!(formatter, "{error}"),
            Self::Incomplete(error) => write!(formatter, "{error}"),
        }
    }
}

impl TranscriptHistoryWindow {
    pub(crate) fn from_latest_page(page: &LoadedTranscriptHistoryPage) -> Self {
        let mut window = Self {
            older_cursor: page.older_cursor.clone(),
            newer_cursor: page.newer_cursor.clone(),
            loading_page: None,
            pages: Vec::new(),
            residency: TranscriptResidencyState::default(),
            next_page_id: 0,
        };
        let turn_ids = page
            .turns
            .iter()
            .map(|turn| turn.id.clone())
            .collect::<Vec<_>>();
        window.push_latest_page(page, turn_ids);
        window
    }

    pub(crate) fn from_turns(turns: &[TurnInfo]) -> Self {
        if turns.is_empty() {
            return Self::default();
        }
        Self::from_latest_page(&LoadedTranscriptHistoryPage {
            turns: turns.to_vec(),
            older_cursor: None,
            newer_cursor: None,
        })
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.pages.is_empty()
            && self.older_cursor.is_none()
            && self.newer_cursor.is_none()
            && self.loading_page.is_none()
    }

    pub(crate) fn reset_residency_for_thread(&mut self, thread_id: impl Into<String>) {
        self.residency.reset_for_thread(thread_id);
    }

    pub(crate) fn bind_residency_to_thread(&mut self, thread_id: impl Into<String>) {
        self.residency.bind_thread(thread_id);
    }

    pub(crate) fn clear_residency(&mut self) {
        self.residency.clear();
    }

    pub(crate) fn set_residency_policy(&mut self, policy: TranscriptResidencyPolicy) {
        self.residency.set_policy(policy);
    }

    pub(crate) fn residency_retained_counts(&self) -> TranscriptResidencyRetainedCounts {
        self.residency.retained_counts()
    }

    pub(crate) fn residency_revision(&self) -> u64 {
        self.residency.revision()
    }

    pub(crate) fn indexed_turn_count(&self) -> usize {
        self.residency.indexed_turn_count()
    }

    pub(crate) fn replace_residency_pins<I, S>(
        &mut self,
        kind: TranscriptResidencyPinKind,
        turn_ids: I,
    ) where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.residency.replace_pins(kind, turn_ids);
    }

    pub(crate) fn pin_resident_turn(&mut self, turn_id: &str, kind: TranscriptResidencyPinKind) {
        self.residency.pin_turn(turn_id, kind);
    }

    pub(crate) fn unpin_resident_turn(&mut self, turn_id: &str, kind: TranscriptResidencyPinKind) {
        self.residency.unpin_turn(turn_id, kind);
    }

    pub(crate) fn release_unretained_resident_turns(
        &mut self,
        retention: &TranscriptResidencyRetention,
    ) -> TranscriptResidencyReleaseCounts {
        self.residency.release_unretained_resident_turns(retention)
    }

    pub(crate) fn release_resident_turns_by_id<I, S>(
        &mut self,
        turn_ids: I,
    ) -> TranscriptResidencyReleaseCounts
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.residency.release_resident_turns_by_id(turn_ids)
    }

    pub(crate) fn release_resident_turns_by_id_with_oversized_fallbacks<I, S, J, T>(
        &mut self,
        turn_ids: I,
        oversized_fallback_turn_ids: J,
    ) -> TranscriptResidencyReleaseCounts
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
        J: IntoIterator<Item = T>,
        T: AsRef<str>,
    {
        self.residency
            .release_resident_turns_by_id_with_oversized_fallbacks(
                turn_ids,
                oversized_fallback_turn_ids,
            )
    }

    pub(crate) fn update_residency_derived_byte_estimates<I, S>(&mut self, estimates: I) -> bool
    where
        I: IntoIterator<Item = (S, usize)>,
        S: AsRef<str>,
    {
        self.residency.update_estimated_derived_bytes(estimates)
    }

    pub(crate) fn retention_range_for_visible_range(
        &self,
        visible_range: Range<usize>,
        turn_count: usize,
    ) -> Range<usize> {
        self.residency
            .policy()
            .retention_range_for_visible_range(visible_range, turn_count)
    }

    pub(crate) fn begin_loading_older(&mut self) -> Option<String> {
        if self.loading_page.is_some() {
            return None;
        }
        let cursor = self.older_cursor.clone()?;
        self.loading_page = Some(LoadingTranscriptHistoryPage::Older {
            cursor: cursor.clone(),
        });
        self.residency.set_in_flight_requests(1);
        Some(cursor)
    }

    pub(crate) fn begin_loading_page_for_visible_range(
        &mut self,
        visible_range: &Range<usize>,
    ) -> Option<TranscriptHistoryPageRequest> {
        if self.loading_page.is_some() {
            return None;
        }

        if let Some(page) = self.released_page_near(visible_range) {
            let page_id = page.id;
            let cursor = page.load_cursor.clone();
            self.loading_page = Some(LoadingTranscriptHistoryPage::Released { page_id });
            self.residency.set_in_flight_requests(1);
            return Some(TranscriptHistoryPageRequest::Released { page_id, cursor });
        }

        if self.should_request_older(visible_range) {
            return self
                .begin_loading_older()
                .map(|cursor| TranscriptHistoryPageRequest::Older { cursor });
        }

        None
    }

    #[allow(dead_code)]
    pub(crate) fn finish_loading_older_with_added(
        &mut self,
        page: &LoadedTranscriptHistoryPage,
        added_turn_count: usize,
    ) {
        let turn_ids = page
            .turns
            .iter()
            .take(added_turn_count)
            .map(|turn| turn.id.clone())
            .collect::<Vec<_>>();
        self.finish_loading_older_with_turn_ids(page, turn_ids);
    }

    pub(crate) fn finish_loading_older_with_turn_ids(
        &mut self,
        page: &LoadedTranscriptHistoryPage,
        turn_ids: Vec<String>,
    ) {
        let load_cursor = match self.loading_page.take() {
            Some(LoadingTranscriptHistoryPage::Older { cursor }) => Some(cursor),
            other => {
                self.loading_page = other;
                None
            }
        };
        let added_turn_count = turn_ids.len();
        self.older_cursor = if added_turn_count == 0 && page.older_cursor == self.older_cursor {
            None
        } else {
            page.older_cursor.clone()
        };
        if page.newer_cursor.is_some() {
            self.newer_cursor = page.newer_cursor.clone();
        }
        self.loading_page = None;
        self.residency.set_in_flight_requests(0);

        if added_turn_count > 0 {
            for page in &mut self.pages {
                page.start += added_turn_count;
            }
            self.residency.shift_source_positions(added_turn_count);
            let id = self.allocate_page_id();
            let resident = page_has_resident_turns(page);
            self.residency
                .index_history_page(&page.turns, load_cursor.as_deref(), 0);
            self.pages.insert(
                0,
                TranscriptHistoryPageState {
                    id,
                    start: 0,
                    len: added_turn_count,
                    turn_ids,
                    load_cursor,
                    older_cursor: page.older_cursor.clone(),
                    newer_cursor: page.newer_cursor.clone(),
                    resident,
                    pinned: false,
                },
            );
        }
    }

    pub(crate) fn fail_loading_older(&mut self) {
        self.loading_page = None;
        self.residency.set_in_flight_requests(0);
    }

    pub(crate) fn is_loading_older(&self) -> bool {
        self.loading_page.is_some()
    }

    pub(crate) fn loading_page_matches_request(
        &self,
        request: &TranscriptHistoryPageRequest,
    ) -> bool {
        match (&self.loading_page, request) {
            (
                Some(LoadingTranscriptHistoryPage::Older {
                    cursor: loading_cursor,
                }),
                TranscriptHistoryPageRequest::Older { cursor },
            ) => loading_cursor == cursor,
            (
                Some(LoadingTranscriptHistoryPage::Released {
                    page_id: loading_page_id,
                }),
                TranscriptHistoryPageRequest::Released { page_id, .. },
            ) => {
                loading_page_id == page_id
                    && self
                        .pages
                        .iter()
                        .find(|page| page.id == *page_id)
                        .is_some_and(|page| {
                            page.load_cursor == request.cursor().map(str::to_string)
                        })
            }
            (
                Some(LoadingTranscriptHistoryPage::Indexed {
                    page_id: loading_page_id,
                }),
                TranscriptHistoryPageRequest::Indexed { page_id, .. },
            ) => {
                loading_page_id == page_id
                    && self
                        .pages
                        .iter()
                        .find(|page| page.id == *page_id)
                        .is_some_and(|page| {
                            page.load_cursor == request.cursor().map(str::to_string)
                        })
            }
            _ => false,
        }
    }

    pub(crate) fn source_start_for_loading_request(
        &self,
        request: &TranscriptHistoryPageRequest,
    ) -> Option<usize> {
        match request {
            TranscriptHistoryPageRequest::Older { .. } => Some(0),
            TranscriptHistoryPageRequest::Indexed { page_id, .. } => self
                .pages
                .iter()
                .find(|page| page.id == *page_id)
                .map(|page| page.start),
            TranscriptHistoryPageRequest::Released { page_id, .. } => self
                .pages
                .iter()
                .find(|page| page.id == *page_id)
                .map(|page| page.start),
        }
    }

    pub(crate) fn retained_counts(&self) -> TranscriptHistoryRetainedCounts {
        let resident_pages = self.pages.iter().filter(|page| page.resident).count();
        let pinned_pages = self.pages.iter().filter(|page| page.pinned).count();
        let turn_ids = self.pages.iter().map(|page| page.turn_ids.len()).sum();
        let turn_id_bytes = self
            .pages
            .iter()
            .flat_map(|page| page.turn_ids.iter())
            .map(String::len)
            .sum::<usize>();
        let cursor_bytes = self
            .older_cursor
            .as_ref()
            .map_or(0, String::len)
            .saturating_add(self.newer_cursor.as_ref().map_or(0, String::len))
            .saturating_add(
                self.loading_page
                    .as_ref()
                    .map_or(0, |loading| match loading {
                        LoadingTranscriptHistoryPage::Older { cursor } => cursor.len(),
                        LoadingTranscriptHistoryPage::Indexed { .. } => 0,
                        LoadingTranscriptHistoryPage::Released { .. } => 0,
                    }),
            )
            .saturating_add(
                self.pages
                    .iter()
                    .map(|page| {
                        page.load_cursor.as_ref().map_or(0, String::len)
                            + page.older_cursor.as_ref().map_or(0, String::len)
                            + page.newer_cursor.as_ref().map_or(0, String::len)
                    })
                    .sum::<usize>(),
            );
        TranscriptHistoryRetainedCounts {
            pages: self.pages.len(),
            resident_pages,
            released_pages: self.pages.len().saturating_sub(resident_pages),
            loading_pages: usize::from(self.loading_page.is_some()),
            pinned_pages,
            turn_ids,
            turn_id_bytes,
            cursor_bytes,
            metadata_bytes: turn_id_bytes.saturating_add(cursor_bytes),
        }
    }

    pub(crate) fn resident_turn_ids(&self) -> Vec<String> {
        self.residency.resident_turn_ids()
    }

    pub(crate) fn pinned_turn_ids(&self) -> Vec<String> {
        self.residency.pinned_turn_ids()
    }

    pub(crate) fn indexed_turns(&self) -> Vec<TranscriptResidencyIndexedTurn> {
        self.residency.indexed_turns()
    }

    pub(crate) fn source_planning_range_for_visible_range(
        &self,
        source_visible_range: Range<usize>,
        viewport_height: usize,
    ) -> Range<usize> {
        let policy = self.residency.policy();
        let viewport_turns = viewport_height
            .max(1)
            .div_ceil(TRANSCRIPT_RESIDENCY_ESTIMATED_ROW_HEIGHT)
            .max(1);
        let leading_turns = viewport_turns
            .saturating_mul(policy.leading_viewport_margins())
            .min(policy.max_resident_turns());
        let trailing_turns = viewport_turns
            .saturating_mul(policy.trailing_viewport_margins())
            .min(policy.max_resident_turns());
        self.residency.source_range_with_turn_margins(
            &source_visible_range,
            leading_turns,
            trailing_turns,
        )
    }

    pub(crate) fn residency_target_plan<I>(
        &self,
        source_visible_range: Range<usize>,
        viewport_height: usize,
        measured_heights: I,
        active_turn_id: Option<&str>,
    ) -> TranscriptResidencyTargetPlan
    where
        I: IntoIterator<Item = TranscriptResidencyMeasuredTurnHeight>,
    {
        self.residency_target_plan_with_streamed_fill(
            source_visible_range,
            viewport_height,
            measured_heights,
            Vec::<TranscriptResidencyStreamedTurnFill>::new(),
            active_turn_id,
        )
    }

    pub(crate) fn residency_target_plan_with_streamed_fill<I, J>(
        &self,
        source_visible_range: Range<usize>,
        viewport_height: usize,
        measured_heights: I,
        streamed_turn_fills: J,
        active_turn_id: Option<&str>,
    ) -> TranscriptResidencyTargetPlan
    where
        I: IntoIterator<Item = TranscriptResidencyMeasuredTurnHeight>,
        J: IntoIterator<Item = TranscriptResidencyStreamedTurnFill>,
    {
        let measured_heights = measured_heights
            .into_iter()
            .map(|height| (height.source_position, height.measured_height))
            .collect::<BTreeMap<_, _>>();
        let streamed_turn_fills = streamed_turn_fills
            .into_iter()
            .map(|fill| (fill.source_position, fill))
            .collect::<BTreeMap<_, _>>();
        let turns = self
            .residency
            .indexed_turns()
            .into_iter()
            .map(|turn| {
                let mut input = TranscriptResidencyTurnPlanInput::new(turn.turn_id)
                    .with_source_position(turn.source_position)
                    .with_estimated_resident_bytes(turn.estimated_resident_bytes)
                    .with_resident(turn.resident)
                    .with_oversized_fallback(turn.oversized_fallback);
                if let Some(measured_height) = measured_heights.get(&turn.source_position) {
                    input = input.with_measured_height(*measured_height);
                }
                if let Some(fill) = streamed_turn_fills.get(&turn.source_position) {
                    input = input.with_streamed_margin_satisfaction(
                        fill.leading_margin_satisfied,
                        fill.trailing_margin_satisfied,
                    );
                }
                input
            })
            .collect::<Vec<_>>();
        let target_policy = self.residency_target_policy();
        let counts = self.residency.retained_counts();
        let mut input = TranscriptResidencyTargetInput::new(
            TranscriptResidencyViewport::new(source_visible_range, viewport_height.max(1)),
            turns,
        )
        .with_pinned_turn_ids(self.residency.cached_pinned_turn_ids())
        .with_in_flight_requests(counts.in_flight_requests)
        .with_policy(target_policy);
        if let Some(active_turn_id) = active_turn_id {
            input = input.with_active_turn_id(active_turn_id);
        }
        let mut plan = plan_transcript_residency_target(input);
        self.extend_plan_with_budget_excess_release_intents(&mut plan, active_turn_id, &counts);
        plan
    }

    pub(crate) fn residency_target_plan_for_source_window<I>(
        &self,
        source_visible_range: Range<usize>,
        source_planning_range: Range<usize>,
        viewport_height: usize,
        measured_heights: I,
        active_turn_id: Option<&str>,
    ) -> TranscriptResidencyTargetPlan
    where
        I: IntoIterator<Item = TranscriptResidencyMeasuredTurnHeight>,
    {
        self.residency_target_plan_for_source_window_with_streamed_fill(
            source_visible_range,
            source_planning_range,
            viewport_height,
            measured_heights,
            Vec::<TranscriptResidencyStreamedTurnFill>::new(),
            active_turn_id,
        )
    }

    pub(crate) fn residency_target_plan_for_source_window_with_streamed_fill<I, J>(
        &self,
        source_visible_range: Range<usize>,
        source_planning_range: Range<usize>,
        viewport_height: usize,
        measured_heights: I,
        streamed_turn_fills: J,
        active_turn_id: Option<&str>,
    ) -> TranscriptResidencyTargetPlan
    where
        I: IntoIterator<Item = TranscriptResidencyMeasuredTurnHeight>,
        J: IntoIterator<Item = TranscriptResidencyStreamedTurnFill>,
    {
        let source_planning_range = source_planning_range.start.min(source_visible_range.start)
            ..source_planning_range.end.max(source_visible_range.end);
        let measured_heights = measured_heights
            .into_iter()
            .map(|height| (height.source_position, height.measured_height))
            .collect::<BTreeMap<_, _>>();
        let streamed_turn_fills = streamed_turn_fills
            .into_iter()
            .map(|fill| (fill.source_position, fill))
            .collect::<BTreeMap<_, _>>();
        let turns = self
            .residency
            .indexed_turns_for_source_range_and_required(&source_planning_range, active_turn_id)
            .into_iter()
            .map(|turn| {
                let mut input = TranscriptResidencyTurnPlanInput::new(turn.turn_id)
                    .with_source_position(turn.source_position)
                    .with_estimated_resident_bytes(turn.estimated_resident_bytes)
                    .with_resident(turn.resident)
                    .with_oversized_fallback(turn.oversized_fallback);
                if let Some(measured_height) = measured_heights.get(&turn.source_position) {
                    input = input.with_measured_height(*measured_height);
                }
                if let Some(fill) = streamed_turn_fills.get(&turn.source_position) {
                    input = input.with_streamed_margin_satisfaction(
                        fill.leading_margin_satisfied,
                        fill.trailing_margin_satisfied,
                    );
                }
                input
            })
            .collect::<Vec<_>>();
        let source_visible_range =
            local_source_visible_range_for_planning_input(&turns, &source_visible_range);
        let target_policy = self.residency_target_policy();
        let counts = self.residency.retained_counts();
        let mut input = TranscriptResidencyTargetInput::new(
            TranscriptResidencyViewport::new(source_visible_range, viewport_height.max(1)),
            turns,
        )
        .with_pinned_turn_ids(self.residency.cached_pinned_turn_ids())
        .with_in_flight_requests(counts.in_flight_requests)
        .with_policy(target_policy);
        if let Some(active_turn_id) = active_turn_id {
            input = input.with_active_turn_id(active_turn_id);
        }
        let mut plan = plan_transcript_residency_target(input);
        self.extend_plan_with_budget_excess_release_intents(&mut plan, active_turn_id, &counts);
        plan
    }

    pub(crate) fn residency_target_policy(&self) -> TranscriptResidencyTargetPolicy {
        let policy = self.residency.policy();
        TranscriptResidencyTargetPolicy::new()
            .with_max_resident_turns(policy.max_resident_turns())
            .with_max_resident_bytes(policy.max_resident_bytes())
            .with_max_in_flight_requests(policy.max_in_flight_requests())
            .with_leading_viewport_margins(policy.leading_viewport_margins())
            .with_trailing_viewport_margins(policy.trailing_viewport_margins())
            .with_default_row_height(TRANSCRIPT_RESIDENCY_ESTIMATED_ROW_HEIGHT)
    }

    fn extend_plan_with_budget_excess_release_intents(
        &self,
        plan: &mut TranscriptResidencyTargetPlan,
        active_turn_id: Option<&str>,
        counts: &TranscriptResidencyRetainedCounts,
    ) {
        let resident_turn_limit = counts.resident_turns > counts.max_resident_turns;
        let resident_byte_limit = counts.resident_bytes > counts.max_resident_bytes;
        if !resident_turn_limit && !resident_byte_limit {
            return;
        }

        let mut retention =
            TranscriptResidencyRetention::from_turn_ids(&plan.desired_full_turn_ids);
        if let Some(active_turn_id) = active_turn_id {
            retention.include_turn_id(active_turn_id);
        }
        let mut release_turn_ids = plan
            .release_turn_ids
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        for turn_id in self.residency.budget_excess_release_turn_ids(
            &retention,
            &plan.release_turn_ids,
            &plan.oversized_turn_fallback_ids,
        ) {
            if release_turn_ids.insert(turn_id.clone()) {
                plan.release_turn_ids.push(turn_id);
            }
        }

        plan.diagnostics.resident_turn_limit |= resident_turn_limit;
        plan.diagnostics.resident_byte_limit |= resident_byte_limit;
        if plan.diagnostics.limiting_reason == TranscriptResidencyBudgetReason::None {
            plan.diagnostics.limiting_reason = counts.budget_reason;
        }
    }

    pub(crate) fn begin_loading_page_for_residency_target_plan(
        &mut self,
        plan: &TranscriptResidencyTargetPlan,
        source_visible_range: &Range<usize>,
    ) -> Option<TranscriptHistoryPageRequest> {
        if self.loading_page.is_some() {
            return None;
        }

        if let Some(page) = self.released_page_for_target_ranges(
            plan.missing_transport_ranges.as_slice(),
            source_visible_range,
        ) {
            let page_id = page.id;
            let cursor = page.load_cursor.clone();
            self.loading_page = Some(LoadingTranscriptHistoryPage::Released { page_id });
            self.residency.set_in_flight_requests(1);
            return Some(TranscriptHistoryPageRequest::Released { page_id, cursor });
        }

        if let Some(page) = self.indexed_page_for_target_ranges(
            plan.missing_transport_ranges.as_slice(),
            source_visible_range,
        ) {
            let page_id = page.id;
            let cursor = page.load_cursor.clone();
            self.loading_page = Some(LoadingTranscriptHistoryPage::Indexed { page_id });
            self.residency.set_in_flight_requests(1);
            return Some(TranscriptHistoryPageRequest::Indexed { page_id, cursor });
        }

        if self.should_request_older(source_visible_range) {
            return self
                .begin_loading_older()
                .map(|cursor| TranscriptHistoryPageRequest::Older { cursor });
        }

        None
    }

    #[allow(dead_code)]
    pub(crate) fn has_older_pages(&self) -> bool {
        self.older_cursor.is_some()
    }

    pub(crate) fn oldest_source_position_known(&self) -> bool {
        !self.has_older_pages()
    }

    pub(crate) fn current_tail_known(&self) -> bool {
        if matches!(
            self.loading_page,
            Some(LoadingTranscriptHistoryPage::Older { .. })
        ) {
            return false;
        }

        self.pages
            .last()
            .map(|page| page.newer_cursor.is_none())
            .unwrap_or_else(|| self.newer_cursor.is_none())
    }

    pub(crate) fn selected_thread_turn_total_is_exact(&self) -> bool {
        self.oldest_source_position_known() && self.current_tail_known()
    }

    pub(crate) fn should_request_older(&self, visible_range: &Range<usize>) -> bool {
        self.has_older_pages()
            && self.loading_page.is_none()
            && visible_range.start
                <= self
                    .residency
                    .policy()
                    .older_request_boundary_rows(visible_range)
    }

    pub(crate) fn boundary_state_for_visible_range(
        &self,
        visible_range: &Range<usize>,
    ) -> TranscriptHistoryBoundaryState {
        TranscriptHistoryBoundaryState {
            can_request_older: self.should_request_older(visible_range),
            released_page_near: self.released_page_near(visible_range).is_some(),
            loading_page: self.loading_page.is_some(),
        }
    }

    pub(crate) fn finish_loading_released_page(
        &mut self,
        page_id: TranscriptHistoryPageId,
        page: &LoadedTranscriptHistoryPage,
    ) -> Option<RestoredTranscriptHistoryPage> {
        self.loading_page = None;
        self.residency.set_in_flight_requests(0);
        let page_state = self.pages.iter_mut().find(|page| page.id == page_id)?;
        page_state.resident = page_has_resident_turns(page);
        page_state.older_cursor = page.older_cursor.clone();
        page_state.newer_cursor = page.newer_cursor.clone();
        self.residency.index_history_page(
            &page.turns,
            page_state.load_cursor.as_deref(),
            page_state.start,
        );
        Some(RestoredTranscriptHistoryPage {
            range: page_state.range(),
            turn_ids: page_state.turn_ids.clone(),
        })
    }

    pub(crate) fn release_cold_pages(
        &mut self,
        visible_range: &Range<usize>,
    ) -> Vec<TranscriptHistoryPageRelease> {
        self.release_cold_pages_with_limit(
            visible_range,
            self.residency.policy().max_resident_pages(),
        )
    }

    pub(crate) fn release_cold_pages_with_limit(
        &mut self,
        visible_range: &Range<usize>,
        max_resident_pages: usize,
    ) -> Vec<TranscriptHistoryPageRelease> {
        let mut resident_count = self.pages.iter().filter(|page| page.resident).count();
        if resident_count <= max_resident_pages {
            return Vec::new();
        }

        let keep_range = expand_range(
            visible_range,
            self.residency
                .policy()
                .cold_release_margin_rows(visible_range),
        );
        let loading_page_id = match self.loading_page {
            Some(LoadingTranscriptHistoryPage::Released { page_id }) => Some(page_id),
            Some(LoadingTranscriptHistoryPage::Indexed { page_id }) => Some(page_id),
            _ => None,
        };
        let mut candidates = self
            .pages
            .iter()
            .enumerate()
            .filter(|(_, page)| {
                page.resident
                    && !page.pinned
                    && Some(page.id) != loading_page_id
                    && !ranges_intersect(&page.range(), &keep_range)
            })
            .map(|(index, page)| (index, page_distance_to_range(&page.range(), visible_range)))
            .collect::<Vec<_>>();
        candidates.sort_by_key(|(_, distance)| std::cmp::Reverse(*distance));

        let mut releases = Vec::new();
        for (index, _) in candidates {
            if resident_count <= max_resident_pages {
                break;
            }
            let page = &mut self.pages[index];
            if !page.resident {
                continue;
            }
            page.resident = false;
            resident_count -= 1;
            releases.push(TranscriptHistoryPageRelease {
                page_id: page.id,
                range: page.range(),
            });
        }

        let retention = self.resident_page_turn_retention();
        self.residency.release_unretained_resident_turns(&retention);
        self.prune_released_page_metadata(visible_range);
        releases
    }

    #[cfg(test)]
    pub(crate) fn resident_page_count(&self) -> usize {
        self.pages.iter().filter(|page| page.resident).count()
    }

    #[cfg(test)]
    pub(crate) fn released_page_count(&self) -> usize {
        self.pages.iter().filter(|page| !page.resident).count()
    }

    fn push_latest_page(&mut self, page: &LoadedTranscriptHistoryPage, turn_ids: Vec<String>) {
        if turn_ids.is_empty() {
            return;
        }

        let id = self.allocate_page_id();
        let resident = page_has_resident_turns(page);
        self.residency.index_history_page(&page.turns, None, 0);
        self.pages.push(TranscriptHistoryPageState {
            id,
            start: 0,
            len: turn_ids.len(),
            turn_ids,
            load_cursor: None,
            older_cursor: page.older_cursor.clone(),
            newer_cursor: page.newer_cursor.clone(),
            resident,
            pinned: true,
        });
    }

    fn released_page_near(
        &self,
        visible_range: &Range<usize>,
    ) -> Option<&TranscriptHistoryPageState> {
        let request_range = expand_range(
            visible_range,
            self.residency.policy().restore_margin_rows(visible_range),
        );
        self.pages
            .iter()
            .filter(|page| !page.resident && ranges_intersect(&page.range(), &request_range))
            .min_by_key(|page| page_distance_to_range(&page.range(), visible_range))
    }

    fn released_page_for_target_ranges(
        &self,
        target_ranges: &[Range<usize>],
        visible_range: &Range<usize>,
    ) -> Option<&TranscriptHistoryPageState> {
        if target_ranges.is_empty() {
            return None;
        }

        self.pages
            .iter()
            .filter(|page| {
                !page.resident
                    && target_ranges
                        .iter()
                        .any(|target| ranges_intersect(&page.range(), target))
            })
            .min_by_key(|page| page_distance_to_range(&page.range(), visible_range))
    }

    fn indexed_page_for_target_ranges(
        &self,
        target_ranges: &[Range<usize>],
        visible_range: &Range<usize>,
    ) -> Option<&TranscriptHistoryPageState> {
        if target_ranges.is_empty() {
            return None;
        }

        self.pages
            .iter()
            .filter(|page| {
                page.resident
                    && target_ranges
                        .iter()
                        .any(|target| ranges_intersect(&page.range(), target))
            })
            .min_by_key(|page| page_distance_to_range(&page.range(), visible_range))
    }

    fn allocate_page_id(&mut self) -> TranscriptHistoryPageId {
        let id = TranscriptHistoryPageId(self.next_page_id);
        self.next_page_id += 1;
        id
    }

    fn prune_released_page_metadata(&mut self, visible_range: &Range<usize>) {
        let loading_page_id = match self.loading_page {
            Some(LoadingTranscriptHistoryPage::Released { page_id }) => Some(page_id),
            Some(LoadingTranscriptHistoryPage::Indexed { page_id }) => Some(page_id),
            _ => None,
        };
        while self.pages.iter().filter(|page| !page.resident).count()
            > self.residency.policy().max_released_pages()
        {
            let Some(index) = self
                .pages
                .iter()
                .enumerate()
                .filter(|(_, page)| !page.resident && Some(page.id) != loading_page_id)
                .max_by_key(|(_, page)| page_distance_to_range(&page.range(), visible_range))
                .map(|(index, _)| index)
            else {
                break;
            };
            self.pages.remove(index);
        }
    }

    fn resident_page_turn_retention(&self) -> TranscriptResidencyRetention {
        let mut retention = TranscriptResidencyRetention::new();
        for page in &self.pages {
            if page.resident || page.pinned {
                retention.include_turn_ids(&page.turn_ids);
            }
        }
        retention
    }
}

pub(crate) fn initial_thread_history_page_options() -> ThreadTurnsListOptions {
    ThreadTurnsListOptions::page(THREAD_HISTORY_PAGE_LIMIT)
        .with_sort_direction(SortDirection::Desc)
        .with_items_view(TurnItemsView::NotLoaded)
}

pub(crate) fn initial_thread_resident_page_options() -> ThreadTurnsListOptions {
    ThreadTurnsListOptions::page(THREAD_HISTORY_PAGE_LIMIT)
        .with_sort_direction(SortDirection::Desc)
        .with_items_view(TurnItemsView::Full)
}

pub(crate) fn older_thread_history_page_options(
    cursor: impl Into<String>,
) -> ThreadTurnsListOptions {
    initial_thread_history_page_options().with_cursor(cursor)
}

pub(crate) fn thread_history_page_options(cursor: Option<&str>) -> ThreadTurnsListOptions {
    match cursor {
        Some(cursor) => initial_thread_history_page_options().with_cursor(cursor),
        None => initial_thread_history_page_options(),
    }
}

pub(crate) fn thread_resident_history_page_options(cursor: Option<&str>) -> ThreadTurnsListOptions {
    match cursor {
        Some(cursor) => initial_thread_resident_page_options().with_cursor(cursor),
        None => initial_thread_resident_page_options(),
    }
}

pub(crate) fn loaded_page_from_desc_response(
    response: ThreadTurnsListResponse,
) -> LoadedTranscriptHistoryPage {
    LoadedTranscriptHistoryPage {
        turns: response
            .data
            .into_iter()
            .map(normalize_index_only_history_turn)
            .rev()
            .collect(),
        older_cursor: response.next_cursor,
        newer_cursor: response.backwards_cursor,
    }
}

fn normalize_index_only_history_turn(mut turn: TurnInfo) -> TurnInfo {
    turn.items_view = TurnItemsView::NotLoaded;
    turn.items.clear();
    turn
}

fn local_source_visible_range_for_planning_input(
    turns: &[TranscriptResidencyTurnPlanInput],
    source_visible_range: &Range<usize>,
) -> Range<usize> {
    let start = turns.partition_point(|turn| {
        turn.source_position.unwrap_or_default() < source_visible_range.start
    });
    let end = turns.partition_point(|turn| {
        turn.source_position.unwrap_or_default() < source_visible_range.end
    });
    start..end.max(start)
}

pub(crate) fn loaded_full_page_from_desc_response(
    response: ThreadTurnsListResponse,
) -> LoadedTranscriptHistoryPage {
    LoadedTranscriptHistoryPage {
        turns: response.data.into_iter().rev().collect(),
        older_cursor: response.next_cursor,
        newer_cursor: response.backwards_cursor,
    }
}

pub(crate) fn initial_thread_activation_resident_turn_ids(
    page: &LoadedTranscriptHistoryPage,
) -> Vec<String> {
    initial_thread_activation_turn_admission_plan(page).resident_turn_ids
}

pub(crate) fn initial_thread_activation_turn_admission_plan(
    page: &LoadedTranscriptHistoryPage,
) -> TranscriptTurnAdmissionPlan {
    if page.turns.is_empty() {
        return TranscriptTurnAdmissionPlan::default();
    }

    let visible_rows = INITIAL_THREAD_ACTIVATION_VIEWPORT_ROWS.min(page.turns.len());
    let visible_start = page.turns.len().saturating_sub(visible_rows);
    turn_admission_plan_for_page_window(
        page,
        visible_start..page.turns.len(),
        visible_rows
            .max(1)
            .saturating_mul(TRANSCRIPT_RESIDENCY_ESTIMATED_ROW_HEIGHT),
        TranscriptResidencyTargetPolicy::new(),
        Vec::<String>::new(),
    )
}

pub(crate) fn resident_turn_ids_for_page_window<I, S>(
    page: &LoadedTranscriptHistoryPage,
    visible_range: Range<usize>,
    viewport_height: usize,
    policy: TranscriptResidencyTargetPolicy,
    pinned_turn_ids: I,
) -> Vec<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    turn_admission_plan_for_page_window(
        page,
        visible_range,
        viewport_height,
        policy,
        pinned_turn_ids,
    )
    .resident_turn_ids
}

pub(crate) fn turn_admission_plan_for_page_window<I, S>(
    page: &LoadedTranscriptHistoryPage,
    visible_range: Range<usize>,
    viewport_height: usize,
    policy: TranscriptResidencyTargetPolicy,
    pinned_turn_ids: I,
) -> TranscriptTurnAdmissionPlan
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    if page.turns.is_empty() {
        return TranscriptTurnAdmissionPlan::default();
    }

    let turns = page
        .turns
        .iter()
        .enumerate()
        .map(|(index, turn)| {
            TranscriptResidencyTurnPlanInput::new(turn.id.clone())
                .with_source_position(index)
                .with_estimated_height(TRANSCRIPT_RESIDENCY_ESTIMATED_ROW_HEIGHT)
                .with_estimated_resident_bytes(residency::estimate_turn_resident_bytes(turn))
                .with_resident(turn.items_view == TurnItemsView::Full)
                .with_oversized_fallback(transcript_fallback::is_oversized_turn_fallback_marker(
                    turn,
                ))
        })
        .collect::<Vec<_>>();
    let plan = plan_transcript_residency_target(
        TranscriptResidencyTargetInput::new(
            TranscriptResidencyViewport::new(visible_range, viewport_height.max(1)),
            turns,
        )
        .with_pinned_turn_ids(pinned_turn_ids)
        .with_policy(policy),
    );
    TranscriptTurnAdmissionPlan {
        resident_turn_ids: plan.desired_full_turn_ids,
        oversized_turn_fallback_ids: plan.oversized_turn_fallback_ids,
    }
}

pub(crate) fn sanitize_loaded_page_for_resident_turn_ids<I, S>(
    page: &LoadedTranscriptHistoryPage,
    resident_turn_ids: I,
) -> LoadedTranscriptHistoryPage
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let resident_turn_ids = resident_turn_ids
        .into_iter()
        .map(|turn_id| turn_id.as_ref().to_string())
        .collect::<Vec<_>>();
    sanitize_loaded_page_for_turn_admission_plan(
        page,
        &TranscriptTurnAdmissionPlan {
            resident_turn_ids,
            oversized_turn_fallback_ids: Vec::new(),
        },
    )
}

pub(crate) fn sanitize_loaded_page_for_turn_admission_plan(
    page: &LoadedTranscriptHistoryPage,
    admission_plan: &TranscriptTurnAdmissionPlan,
) -> LoadedTranscriptHistoryPage {
    let resident_turn_ids = admission_plan
        .resident_turn_ids
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let oversized_turn_fallback_ids = admission_plan
        .oversized_turn_fallback_ids
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    LoadedTranscriptHistoryPage {
        turns: page
            .turns
            .iter()
            .cloned()
            .map(|turn| {
                if resident_turn_ids.contains(turn.id.as_str()) {
                    turn
                } else if oversized_turn_fallback_ids.contains(turn.id.as_str()) {
                    transcript_fallback::oversized_turn_fallback_marker(turn)
                } else {
                    normalize_index_only_history_turn(turn)
                }
            })
            .collect(),
        older_cursor: page.older_cursor.clone(),
        newer_cursor: page.newer_cursor.clone(),
    }
}

pub(crate) fn validate_resident_history_page(
    page: &LoadedTranscriptHistoryPage,
) -> Result<(), TranscriptResidentPageValidationError> {
    let Some(turn) = page
        .turns
        .iter()
        .find(|turn| turn.items_view != TurnItemsView::Full)
    else {
        return Ok(());
    };

    Err(TranscriptResidentPageValidationError {
        turn_id: turn.id.clone(),
        items_view: turn.items_view,
    })
}

#[allow(dead_code)]
pub(crate) fn load_older_thread_history_page<B>(
    backend: &mut B,
    thread_id: &str,
    cursor: &str,
    timeout: Duration,
) -> Result<LoadedTranscriptHistoryPage, B::Error>
where
    B: TranscriptHistoryBackend,
{
    let options = older_thread_history_page_options(cursor);
    backend
        .list_thread_turns(thread_id, &options, timeout)
        .map(loaded_page_from_desc_response)
}

pub(crate) fn load_thread_resident_history_page<B>(
    backend: &mut B,
    thread_id: &str,
    cursor: Option<&str>,
    timeout: Duration,
) -> Result<LoadedTranscriptHistoryPage, TranscriptResidentHistoryPageError<B::Error>>
where
    B: TranscriptHistoryBackend,
{
    let options = thread_resident_history_page_options(cursor);
    let page = backend
        .list_thread_turns(thread_id, &options, timeout)
        .map_err(TranscriptResidentHistoryPageError::Backend)
        .map(loaded_full_page_from_desc_response)?;
    validate_resident_history_page(&page)
        .map_err(TranscriptResidentHistoryPageError::Incomplete)?;
    Ok(page)
}

impl TranscriptHistoryPageRequest {
    pub(crate) fn cursor(&self) -> Option<&str> {
        match self {
            Self::Older { cursor } => Some(cursor.as_str()),
            Self::Indexed { cursor, .. } => cursor.as_deref(),
            Self::Released { cursor, .. } => cursor.as_deref(),
        }
    }
}

impl TranscriptHistoryPageState {
    fn range(&self) -> Range<usize> {
        self.start..self.start + self.len
    }
}

fn expand_range(range: &Range<usize>, margin: usize) -> Range<usize> {
    range.start.saturating_sub(margin)..range.end.saturating_add(margin)
}

fn ranges_intersect(left: &Range<usize>, right: &Range<usize>) -> bool {
    left.start < right.end && right.start < left.end
}

fn page_distance_to_range(page: &Range<usize>, range: &Range<usize>) -> usize {
    if ranges_intersect(page, range) {
        0
    } else if page.end <= range.start {
        range.start - page.end
    } else {
        page.start.saturating_sub(range.end)
    }
}

fn page_has_resident_turns(page: &LoadedTranscriptHistoryPage) -> bool {
    page.turns
        .iter()
        .any(|turn| turn.items_view == TurnItemsView::Full)
}
