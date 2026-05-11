use std::{fmt, ops::Range, time::Duration};

use beryl_backend::{
    ManagedBackendError, ManagedBackendSession, SortDirection, ThreadTurnsListOptions,
    ThreadTurnsListResponse, TurnInfo,
};

pub(crate) const THREAD_HISTORY_PAGE_LIMIT: u32 = 80;
pub(crate) const TRANSCRIPT_HISTORY_MAX_RESIDENT_PAGES: usize = 4;
#[allow(dead_code)]
const OLDER_HISTORY_VISIBLE_ROW_THRESHOLD: usize = 2;
const MISSING_HISTORY_VISIBLE_ROW_THRESHOLD: usize = 8;
const HISTORY_CACHE_KEEP_MARGIN_ROWS: usize = THREAD_HISTORY_PAGE_LIMIT as usize;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TranscriptHistoryWindow {
    older_cursor: Option<String>,
    newer_cursor: Option<String>,
    loading_page: Option<LoadingTranscriptHistoryPage>,
    pages: Vec<TranscriptHistoryPageState>,
    next_page_id: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LoadedTranscriptHistoryPage {
    pub turns: Vec<TurnInfo>,
    pub older_cursor: Option<String>,
    pub newer_cursor: Option<String>,
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

impl TranscriptHistoryWindow {
    pub(crate) fn from_latest_page(page: &LoadedTranscriptHistoryPage) -> Self {
        let mut window = Self {
            older_cursor: page.older_cursor.clone(),
            newer_cursor: page.newer_cursor.clone(),
            loading_page: None,
            pages: Vec::new(),
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

    pub(crate) fn begin_loading_older(&mut self) -> Option<String> {
        if self.loading_page.is_some() {
            return None;
        }
        let cursor = self.older_cursor.clone()?;
        self.loading_page = Some(LoadingTranscriptHistoryPage::Older {
            cursor: cursor.clone(),
        });
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
            return Some(TranscriptHistoryPageRequest::Released { page_id, cursor });
        }

        self.begin_loading_older()
            .map(|cursor| TranscriptHistoryPageRequest::Older { cursor })
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

        if added_turn_count > 0 {
            for page in &mut self.pages {
                page.start += added_turn_count;
            }
            let id = self.allocate_page_id();
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
                    resident: true,
                    pinned: false,
                },
            );
        }
    }

    pub(crate) fn fail_loading_older(&mut self) {
        self.loading_page = None;
    }

    pub(crate) fn is_loading_older(&self) -> bool {
        self.loading_page.is_some()
    }

    pub(crate) fn retained_counts(&self) -> TranscriptHistoryRetainedCounts {
        let resident_pages = self.pages.iter().filter(|page| page.resident).count();
        TranscriptHistoryRetainedCounts {
            pages: self.pages.len(),
            resident_pages,
            released_pages: self.pages.len().saturating_sub(resident_pages),
            loading_pages: usize::from(self.loading_page.is_some()),
        }
    }

    #[allow(dead_code)]
    pub(crate) fn has_older_pages(&self) -> bool {
        self.older_cursor.is_some()
    }

    pub(crate) fn current_tail_known(&self) -> bool {
        if self.loading_page.is_some() {
            return false;
        }

        self.pages
            .last()
            .map(|page| page.resident && page.newer_cursor.is_none())
            .unwrap_or_else(|| self.newer_cursor.is_none())
    }

    #[allow(dead_code)]
    pub(crate) fn should_request_older(&self, visible_range: &Range<usize>) -> bool {
        self.has_older_pages()
            && self.loading_page.is_none()
            && visible_range.start <= OLDER_HISTORY_VISIBLE_ROW_THRESHOLD
    }

    pub(crate) fn finish_loading_released_page(
        &mut self,
        page_id: TranscriptHistoryPageId,
        page: &LoadedTranscriptHistoryPage,
    ) -> Option<RestoredTranscriptHistoryPage> {
        self.loading_page = None;
        let page_state = self.pages.iter_mut().find(|page| page.id == page_id)?;
        page_state.resident = true;
        page_state.older_cursor = page.older_cursor.clone();
        page_state.newer_cursor = page.newer_cursor.clone();
        Some(RestoredTranscriptHistoryPage {
            range: page_state.range(),
            turn_ids: page_state.turn_ids.clone(),
        })
    }

    pub(crate) fn release_cold_pages(
        &mut self,
        visible_range: &Range<usize>,
    ) -> Vec<TranscriptHistoryPageRelease> {
        self.release_cold_pages_with_limit(visible_range, TRANSCRIPT_HISTORY_MAX_RESIDENT_PAGES)
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

        let keep_range = expand_range(visible_range, HISTORY_CACHE_KEEP_MARGIN_ROWS);
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
        self.pages.push(TranscriptHistoryPageState {
            id,
            start: 0,
            len: turn_ids.len(),
            turn_ids,
            load_cursor: None,
            older_cursor: page.older_cursor.clone(),
            newer_cursor: page.newer_cursor.clone(),
            resident: true,
            pinned: true,
        });
    }

    fn released_page_near(
        &self,
        visible_range: &Range<usize>,
    ) -> Option<&TranscriptHistoryPageState> {
        let request_range = expand_range(visible_range, MISSING_HISTORY_VISIBLE_ROW_THRESHOLD);
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
}

pub(crate) fn initial_thread_history_page_options() -> ThreadTurnsListOptions {
    ThreadTurnsListOptions::page(THREAD_HISTORY_PAGE_LIMIT).with_sort_direction(SortDirection::Desc)
}

pub(crate) fn older_thread_history_page_options(
    cursor: impl Into<String>,
) -> ThreadTurnsListOptions {
    initial_thread_history_page_options().with_cursor(cursor)
}

pub(crate) fn thread_history_page_options(cursor: Option<&str>) -> ThreadTurnsListOptions {
    match cursor {
        Some(cursor) => older_thread_history_page_options(cursor),
        None => initial_thread_history_page_options(),
    }
}

pub(crate) fn loaded_page_from_desc_response(
    response: ThreadTurnsListResponse,
) -> LoadedTranscriptHistoryPage {
    LoadedTranscriptHistoryPage {
        turns: response.data.into_iter().rev().collect(),
        older_cursor: response.next_cursor,
        newer_cursor: response.backwards_cursor,
    }
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
