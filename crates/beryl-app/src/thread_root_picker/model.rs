use std::ops::Range;

pub const PICKER_PAGE_ROWS: usize = 32;
pub const PICKER_MAX_RESIDENT_PAGES: usize = 24;
pub const PICKER_MAX_REALIZED_ROWS: usize = 24;
pub const PICKER_OVERSCAN_ROWS: usize = 4;
pub const PICKER_QUERY_BYTES: usize = beryl_state::CATALOG_QUERY_MAX_BYTES;

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct PickerCollectionKey(pub String);

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct PickerRowKey(pub String);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PickerRow {
    pub key: PickerRowKey,
    pub primary: String,
    pub secondary: String,
    pub status: String,
    pub tooltip: Option<String>,
    pub unavailable_reason: Option<String>,
    pub current: bool,
    pub activation_pending: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PickerPageRequest {
    pub collection_key: PickerCollectionKey,
    pub query_revision: u64,
    pub range: Range<usize>,
    pub request_id: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PickerPage {
    pub request: PickerPageRequest,
    pub total_count: usize,
    pub rows: Vec<PickerRow>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PickerPageOutcome {
    Success(PickerPage),
    Failed {
        request: PickerPageRequest,
        message: String,
    },
    Cancelled(PickerPageRequest),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PickerEvent {
    QueryChanged {
        collection_key: PickerCollectionKey,
        query_revision: u64,
        query: String,
    },
    RequestPage(PickerPageRequest),
    Activate(PickerRowKey),
    Dismiss,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PickerNavigation {
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PickerNavigationTarget {
    pub collection_key: PickerCollectionKey,
    pub query_revision: u64,
    pub position: usize,
    pub request_id: u64,
    pub expected_key: Option<PickerRowKey>,
}

#[derive(Clone, Debug)]
pub struct PickerDiagnostics {
    pub collection_key: PickerCollectionKey,
    pub query_revision: u64,
    pub total_count: usize,
    pub resident_page_count: usize,
    pub resident_row_count: usize,
    pub pending_page_count: usize,
    pub visible_range: Range<usize>,
    pub realized_range: Range<usize>,
    pub realized_row_count: usize,
    pub focused_key: Option<PickerRowKey>,
    pub pending_navigation: Option<PickerNavigationTarget>,
    pub scroll_offset: f32,
    pub collection_failed: bool,
}

pub struct PickerCollection {
    pub(super) key: PickerCollectionKey,
    pub(super) revision: u64,
    pub(super) total: usize,
    pub(super) pages: Vec<PickerPage>,
    pub(super) requests: Vec<PickerPageRequest>,
    pub(super) next_request: u64,
    pub(super) focused: Option<(PickerRowKey, usize)>,
    pub(super) target: Option<PickerNavigationTarget>,
    pub(super) failure: Option<String>,
    pub(super) admitted_revision: Option<u64>,
}

impl PickerCollection {
    pub fn new(key: PickerCollectionKey, revision: u64, total: usize) -> Self {
        Self {
            key,
            revision,
            total,
            pages: Vec::with_capacity(PICKER_MAX_RESIDENT_PAGES),
            requests: Vec::with_capacity(2),
            next_request: 1,
            focused: None,
            target: None,
            failure: None,
            admitted_revision: None,
        }
    }

    pub fn replace(&mut self, key: PickerCollectionKey, revision: u64, total: usize) {
        let same_collection = self.key == key;
        self.key = key;
        self.revision = revision;
        if !same_collection {
            self.pages.clear();
            self.focused = None;
            self.total = total;
        }
        self.requests.clear();
        self.target = None;
        self.failure = None;
        self.admitted_revision = None;
    }

    pub fn focused_key(&self) -> Option<&PickerRowKey> {
        self.focused.as_ref().map(|(key, _)| key)
    }
    pub fn focused_position(&self) -> Option<usize> {
        self.focused.as_ref().map(|(_, position)| *position)
    }
    pub fn resolve_removed_focus(
        &mut self,
        position: Option<usize>,
    ) -> (Option<usize>, Option<PickerPageRequest>) {
        self.target = None;
        match position {
            Some(position) => self.target_position(position, None),
            None => {
                self.focused = None;
                (None, None)
            }
        }
    }
    pub fn pending_requests(&self) -> &[PickerPageRequest] {
        &self.requests
    }
    pub fn resident_page_count(&self) -> usize {
        self.pages.len()
    }
    pub fn resident_row_count(&self) -> usize {
        self.pages.iter().map(|page| page.rows.len()).sum()
    }
    pub fn total_count(&self) -> usize {
        self.total
    }
    pub fn retain_window(&mut self, range: Range<usize>) {
        if self.is_current() {
            self.pages.retain(|page| {
                page.request.range.start < range.end
                    && page.request.range.start.saturating_add(page.rows.len()) > range.start
            });
        }
    }
    pub fn row(&self, position: usize) -> Option<&PickerRow> {
        self.pages.iter().find_map(|page| {
            position
                .checked_sub(page.request.range.start)
                .and_then(|index| page.rows.get(index))
        })
    }
    pub fn is_current(&self) -> bool {
        self.admitted_revision == Some(self.revision)
    }

    pub fn request(&mut self, position: usize) -> Option<PickerPageRequest> {
        let start = position;
        let end = start.checked_add(PICKER_PAGE_ROWS)?;
        if self.is_current() && (position >= self.total || self.row(position).is_some()) {
            return None;
        }
        if self
            .requests
            .iter()
            .any(|request| request.range.contains(&position))
        {
            return None;
        }
        if self.requests.len() == 2 {
            let obsolete = self.requests.remove(0);
            if self
                .target
                .as_ref()
                .is_some_and(|target| target.request_id == obsolete.request_id)
            {
                self.target = None;
            }
        }
        let request_id = self.next_request;
        self.next_request = self.next_request.checked_add(1)?;
        let request = PickerPageRequest {
            collection_key: self.key.clone(),
            query_revision: self.revision,
            range: start..end,
            request_id,
        };
        self.requests.push(request.clone());
        Some(request)
    }

    pub fn cancel_navigation(&mut self) {
        self.target = None;
    }

    pub fn navigate(
        &mut self,
        direction: PickerNavigation,
        visible_rows: usize,
    ) -> (Option<usize>, Option<PickerPageRequest>) {
        if self.total == 0 {
            self.target = None;
            return (None, None);
        }
        let initial = self.target.is_none() && self.focused.is_none();
        let current = self
            .target
            .as_ref()
            .map(|target| target.position)
            .or_else(|| self.focused.as_ref().map(|(_, position)| *position))
            .unwrap_or(0);
        let position = match direction {
            PickerNavigation::Up => current.saturating_sub(1),
            PickerNavigation::Down if initial => 0,
            PickerNavigation::Down => current.saturating_add(1).min(self.total - 1),
            PickerNavigation::Home => 0,
            PickerNavigation::End => self.total - 1,
            PickerNavigation::PageUp => current.saturating_sub(visible_rows.max(1)),
            PickerNavigation::PageDown => current
                .saturating_add(visible_rows.max(1))
                .min(self.total - 1),
        };
        self.target = None;
        self.target_position(position, None)
    }

    pub fn restore_focus_position(
        &mut self,
        position: usize,
    ) -> (Option<usize>, Option<PickerPageRequest>) {
        let expected = self.focused_key().cloned();
        self.target_position(position, expected)
    }

    fn target_position(
        &mut self,
        position: usize,
        expected_key: Option<PickerRowKey>,
    ) -> (Option<usize>, Option<PickerPageRequest>) {
        if self.is_current() {
            if let Some(row) = self.row(position) {
                if expected_key.as_ref().is_none_or(|key| key == &row.key) {
                    self.focused = Some((row.key.clone(), position));
                    return (Some(position), None);
                }
            }
        }
        let emitted = self.request(position);
        if let Some(request) = self
            .requests
            .iter()
            .find(|request| request.range.contains(&position))
        {
            self.target = Some(PickerNavigationTarget {
                collection_key: self.key.clone(),
                query_revision: self.revision,
                position,
                request_id: request.request_id,
                expected_key,
            });
        }
        (None, emitted)
    }

    pub fn focus_position(&mut self, position: usize) -> bool {
        self.target = None;
        if !self.is_current() {
            return false;
        }
        let Some(row) = self.row(position) else {
            return false;
        };
        self.focused = Some((row.key.clone(), position));
        true
    }

    pub fn activation(&self, key: &PickerRowKey) -> Option<PickerEvent> {
        if !self.is_current() {
            return None;
        }
        self.pages
            .iter()
            .flat_map(|page| &page.rows)
            .find(|row| {
                &row.key == key && row.unavailable_reason.is_none() && !row.activation_pending
            })
            .map(|row| PickerEvent::Activate(row.key.clone()))
    }

    pub fn settle(
        &mut self,
        outcome: PickerPageOutcome,
        traversal_owns_focus: bool,
    ) -> Option<usize> {
        let request = match &outcome {
            PickerPageOutcome::Success(page) => &page.request,
            PickerPageOutcome::Failed { request, .. } | PickerPageOutcome::Cancelled(request) => {
                request
            }
        };
        if request.collection_key != self.key || request.query_revision != self.revision {
            return None;
        }
        let index = self
            .requests
            .iter()
            .position(|pending| pending == request)?;
        self.requests.remove(index);
        let target_matches = self
            .target
            .as_ref()
            .is_some_and(|target| target.request_id == request.request_id);
        match outcome {
            PickerPageOutcome::Success(page) => {
                let max_len = page
                    .total_count
                    .saturating_sub(page.request.range.start)
                    .min(PICKER_PAGE_ROWS);
                let valid = page.request.range.end.checked_sub(page.request.range.start)
                    == Some(PICKER_PAGE_ROWS)
                    && page.rows.len() <= max_len
                    && (max_len == 0 || !page.rows.is_empty())
                    && (!self.is_current() || page.total_count == self.total)
                    && page.rows.iter().enumerate().all(|(i, row)| {
                        !page.rows[..i].iter().any(|prior| prior.key == row.key)
                            && (!self.is_current()
                                || self.pages.iter().all(|resident| {
                                    resident.rows.iter().enumerate().all(|(offset, prior)| {
                                        prior.key != row.key
                                            || resident.request.range.start + offset
                                                == page.request.range.start + i
                                    })
                                }))
                    });
                if !valid {
                    self.failure = Some("The collection page was invalid.".into());
                    if target_matches {
                        self.target = None;
                    }
                    return None;
                }
                if !self.is_current() {
                    self.pages.clear();
                }
                self.total = page.total_count;
                self.admitted_revision = Some(self.revision);
                self.failure = None;
                self.pages
                    .retain(|resident| resident.request.range.start != page.request.range.start);
                if self.pages.len() == PICKER_MAX_RESIDENT_PAGES {
                    self.pages.remove(0);
                }
                self.pages.push(page);
                if let Some((key, position)) = &mut self.focused {
                    if let Some(found) = self.pages.iter().find_map(|page| {
                        page.rows
                            .iter()
                            .position(|row| &row.key == key)
                            .map(|offset| page.request.range.start + offset)
                    }) {
                        *position = found;
                    }
                }
                if target_matches {
                    let target = self.target.take()?;
                    if traversal_owns_focus {
                        if let Some(row) = self.row(target.position) {
                            if target
                                .expected_key
                                .as_ref()
                                .is_none_or(|key| key == &row.key)
                            {
                                self.focused = Some((row.key.clone(), target.position));
                                return Some(target.position);
                            }
                        }
                    }
                }
            }
            PickerPageOutcome::Failed { message, .. } => {
                self.failure = Some(message);
                if target_matches {
                    self.target = None;
                }
            }
            PickerPageOutcome::Cancelled(_) => {
                if target_matches {
                    self.target = None;
                }
            }
        }
        None
    }
}
