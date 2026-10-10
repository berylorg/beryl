use beryl_model::SyndicThreadId;
use std::collections::VecDeque;

pub const LINEAGE_PAGE_SLOTS: u64 = 32;
pub const LINEAGE_RESIDENT_PAGES: usize = 24;
pub const LINEAGE_PENDING_PAGES: usize = 2;
pub const LINEAGE_PAGE_BYTES: usize = 256 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LineageQuery {
    pub revision: u64,
    pub selected: SyndicThreadId,
    pub parent_count: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LineageBreadcrumb {
    pub thread: SyndicThreadId,
    pub title: String,
    pub reason: Option<String>,
}

impl LineageBreadcrumb {
    pub fn presentation_bytes(&self) -> usize {
        std::mem::size_of::<Self>() + self.title.len() + self.reason.as_ref().map_or(0, String::len)
    }

    fn bounded(&self) -> bool {
        self.title.len() <= 512
            && self
                .reason
                .as_ref()
                .is_none_or(|reason| reason.chars().take(513).count() <= 512)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LineagePageRequest {
    pub query: LineageQuery,
    pub request_id: u64,
    pub start: u64,
    pub end: u64,
}

#[derive(Clone, Debug)]
pub struct LineagePage {
    pub request: LineagePageRequest,
    pub rows: Vec<LineageBreadcrumb>,
    pub next_ordinal: u64,
}

#[derive(Clone, Debug)]
pub enum LineageEvent {
    RequestPage(LineagePageRequest),
    Activate {
        query: LineageQuery,
        thread: SyndicThreadId,
    },
}

#[derive(Clone, Copy, Debug)]
pub enum LineageMovement {
    Left,
    Right,
    Home,
    End,
}

#[derive(Clone, Copy)]
pub(super) struct LogicalFocus {
    pub(super) query: LineageQuery,
    pub(super) thread: SyndicThreadId,
    pub(super) ordinal: u64,
}

struct ResidentPage {
    start: u64,
    slots: Vec<Option<LineageBreadcrumb>>,
}

pub(super) struct LineageModel {
    pub(super) query: LineageQuery,
    pages: VecDeque<ResidentPage>,
    pub(super) requests: Vec<LineagePageRequest>,
    sequence: u64,
    pub(super) focus: Option<LogicalFocus>,
    pub(super) target: Option<(u64, Option<SyndicThreadId>)>,
    pub(super) failure: bool,
    pub(super) paused: bool,
}

impl LineageModel {
    pub(super) fn new(query: LineageQuery) -> Self {
        Self {
            query,
            pages: VecDeque::new(),
            requests: Vec::new(),
            sequence: 0,
            focus: None,
            target: None,
            failure: false,
            paused: false,
        }
    }

    pub(super) fn replace(&mut self, query: LineageQuery) {
        let retained = (self.query.selected == query.selected)
            .then(|| {
                match self.target {
                    Some(target) => target.1.is_some().then_some(target),
                    None => self.focus.map(|focus| (focus.ordinal, Some(focus.thread))),
                }
                .filter(|(ordinal, _)| *ordinal < query.parent_count)
            })
            .flatten();
        self.query = query;
        self.pages.clear();
        self.requests.clear();
        self.focus = None;
        self.target = retained;
        self.failure = false;
        self.paused = false;
    }

    pub(super) fn row(&self, ordinal: u64) -> Option<&LineageBreadcrumb> {
        let base = ordinal / LINEAGE_PAGE_SLOTS * LINEAGE_PAGE_SLOTS;
        self.pages
            .iter()
            .find(|page| page.start == base)?
            .slots
            .get((ordinal - base) as usize)?
            .as_ref()
    }

    pub(super) fn request(&mut self, ordinal: u64) -> Option<LineagePageRequest> {
        if ordinal >= self.query.parent_count
            || self.row(ordinal).is_some()
            || self.failure
            || self.paused
            || self.requests.len() >= LINEAGE_PENDING_PAGES
        {
            return None;
        }
        let base = ordinal / LINEAGE_PAGE_SLOTS * LINEAGE_PAGE_SLOTS;
        let end = base
            .saturating_add(LINEAGE_PAGE_SLOTS)
            .min(self.query.parent_count);
        if self
            .requests
            .iter()
            .any(|request| request.start / LINEAGE_PAGE_SLOTS == base / LINEAGE_PAGE_SLOTS)
        {
            return None;
        }
        let start = (base..end).find(|position| self.row(*position).is_none())?;
        self.sequence = self.sequence.checked_add(1)?;
        let request = LineagePageRequest {
            query: self.query,
            request_id: self.sequence,
            start,
            end,
        };
        self.requests.push(request);
        Some(request)
    }

    pub(super) fn settle(
        &mut self,
        request: LineagePageRequest,
        result: Option<LineagePage>,
        failed: bool,
    ) -> bool {
        if request.query != self.query || !self.requests.contains(&request) {
            return false;
        }
        self.requests.retain(|pending| pending != &request);
        let Some(page) = result else {
            self.failure |= failed;
            self.paused = true;
            return true;
        };
        if page.request != request
            || page.rows.is_empty()
            || page.rows.len() > 32
            || page.next_ordinal
                != request
                    .start
                    .checked_add(page.rows.len() as u64)
                    .unwrap_or(u64::MAX)
            || page.next_ordinal > request.end
            || request.end > self.query.parent_count
            || page
                .rows
                .iter()
                .any(|row| !row.bounded() || row.thread == self.query.selected)
            || page
                .rows
                .iter()
                .map(LineageBreadcrumb::presentation_bytes)
                .sum::<usize>()
                > LINEAGE_PAGE_BYTES
            || page.rows.iter().enumerate().any(|(index, row)| {
                page.rows[..index]
                    .iter()
                    .any(|prior| prior.thread == row.thread)
            })
        {
            self.failure = true;
            return false;
        }
        let base = request.start / LINEAGE_PAGE_SLOTS * LINEAGE_PAGE_SLOTS;
        let mut resident = self
            .pages
            .iter()
            .position(|page| page.start == base)
            .and_then(|index| self.pages.remove(index))
            .unwrap_or_else(|| ResidentPage {
                start: base,
                slots: vec![
                    None;
                    (self.query.parent_count - base).min(LINEAGE_PAGE_SLOTS) as usize
                ],
            });
        for (offset, row) in page.rows.into_iter().enumerate() {
            resident.slots[(request.start - base) as usize + offset] = Some(row);
        }
        self.pages.push_back(resident);
        while self.pages.len() > LINEAGE_RESIDENT_PAGES {
            self.pages.pop_front();
        }
        if let Some((ordinal, expected)) = self.target {
            if let Some(row) = self.row(ordinal) {
                self.focus = expected
                    .filter(|key| *key != row.thread)
                    .is_none()
                    .then_some(LogicalFocus {
                        query: self.query,
                        thread: row.thread,
                        ordinal,
                    });
                self.target = None;
            }
        }
        true
    }

    pub(super) fn focus_position(&mut self, ordinal: u64) {
        if ordinal >= self.query.parent_count {
            return;
        }
        if let Some(row) = self.row(ordinal) {
            self.focus = Some(LogicalFocus {
                query: self.query,
                thread: row.thread,
                ordinal,
            });
            self.target = None;
        } else {
            self.target = Some((ordinal, None));
        }
    }

    pub(super) fn movement(&mut self, movement: LineageMovement) -> Option<u64> {
        if self.query.parent_count == 0 {
            return None;
        }
        let current = self
            .target
            .map(|(ordinal, _)| ordinal)
            .or(self.focus.map(|focus| focus.ordinal));
        let ordinal = match movement {
            LineageMovement::Home => 0,
            LineageMovement::End => self.query.parent_count - 1,
            LineageMovement::Left => current.unwrap_or(1).saturating_sub(1),
            LineageMovement::Right => current.map_or(0, |ordinal| {
                ordinal.saturating_add(1).min(self.query.parent_count - 1)
            }),
        };
        self.focus_position(ordinal);
        Some(ordinal)
    }

    pub(super) fn resident_count(&self) -> usize {
        self.pages.len()
    }
}
