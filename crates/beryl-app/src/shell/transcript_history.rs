use std::{fmt, ops::Range, time::Duration};

use beryl_backend::{
    ManagedBackendError, ManagedBackendSession, SortDirection, ThreadTurnsListOptions,
    ThreadTurnsListResponse, TurnInfo, TurnItemsView,
};

#[path = "transcript_history/residency.rs"]
mod residency;

#[allow(unused_imports)]
pub(crate) use residency::{
    TranscriptResidencyBudgetReason, TranscriptResidencyPinKind, TranscriptResidencyPolicy,
    TranscriptResidencyReleaseCounts, TranscriptResidencyRequestPriority,
    TranscriptResidencyRetainedCounts, TranscriptResidencyRetention, TranscriptResidencyState,
    TranscriptTurnIndexRecord,
};

pub(crate) const THREAD_HISTORY_PAGE_LIMIT: u32 = 80;

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

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TranscriptHistoryPageRequest {
    Older {
        cursor: String,
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
            let resident = page_has_full_turns(page);
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
        page_state.resident = page_has_full_turns(page);
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
        let resident = page_has_full_turns(page);
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

    fn allocate_page_id(&mut self) -> TranscriptHistoryPageId {
        let id = TranscriptHistoryPageId(self.next_page_id);
        self.next_page_id += 1;
        id
    }

    fn prune_released_page_metadata(&mut self, visible_range: &Range<usize>) {
        let loading_page_id = match self.loading_page {
            Some(LoadingTranscriptHistoryPage::Released { page_id }) => Some(page_id),
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

pub(crate) fn loaded_full_page_from_desc_response(
    response: ThreadTurnsListResponse,
) -> LoadedTranscriptHistoryPage {
    LoadedTranscriptHistoryPage {
        turns: response.data.into_iter().rev().collect(),
        older_cursor: response.next_cursor,
        newer_cursor: response.backwards_cursor,
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

pub(crate) fn load_thread_history_page<B>(
    backend: &mut B,
    thread_id: &str,
    cursor: Option<&str>,
    timeout: Duration,
) -> Result<LoadedTranscriptHistoryPage, B::Error>
where
    B: TranscriptHistoryBackend,
{
    let options = thread_history_page_options(cursor);
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

fn page_has_full_turns(page: &LoadedTranscriptHistoryPage) -> bool {
    !page.turns.is_empty()
        && page
            .turns
            .iter()
            .all(|turn| turn.items_view == TurnItemsView::Full)
}
