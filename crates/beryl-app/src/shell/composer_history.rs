use std::collections::HashMap;

use super::composer_draft::{AcceptedComposerDraft, ComposerDraft};

const DEFAULT_COMPOSER_HISTORY_CAPACITY: usize = 100;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) enum ComposerHistoryScope {
    Thread(String),
    PendingNewThread(u64),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum ComposerHistoryBrowseResult {
    Accepted(AcceptedComposerDraft),
    Draft(ComposerDraft),
}

#[derive(Clone, Debug)]
pub(super) struct ComposerHistoryState {
    capacity: usize,
    lanes: HashMap<ComposerHistoryScope, ComposerHistoryLane>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct ComposerHistoryLane {
    entries: Vec<AcceptedComposerDraft>,
    browse: Option<ComposerHistoryBrowse>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ComposerHistoryBrowse {
    cursor: usize,
    original_draft: ComposerDraft,
}

impl Default for ComposerHistoryState {
    fn default() -> Self {
        Self::with_capacity(DEFAULT_COMPOSER_HISTORY_CAPACITY)
    }
}

impl ComposerHistoryState {
    pub(super) fn with_capacity(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            lanes: HashMap::new(),
        }
    }

    pub(super) fn record_accepted(
        &mut self,
        scope: ComposerHistoryScope,
        draft: AcceptedComposerDraft,
    ) {
        let capacity = self.capacity;
        let lane = self.lanes.entry(scope).or_default();
        lane.browse = None;
        push_accepted_entry(lane, draft, capacity);
    }

    pub(super) fn browse_previous(
        &mut self,
        scope: ComposerHistoryScope,
        current_draft: ComposerDraft,
    ) -> Option<ComposerHistoryBrowseResult> {
        let lane = self.lanes.get_mut(&scope)?;
        if lane.entries.is_empty() {
            return None;
        }

        match lane.browse.as_mut() {
            Some(browse) if browse.cursor > 0 => {
                browse.cursor -= 1;
                Some(ComposerHistoryBrowseResult::Accepted(
                    lane.entries[browse.cursor].clone(),
                ))
            }
            Some(_) => None,
            None => {
                let cursor = lane.entries.len() - 1;
                lane.browse = Some(ComposerHistoryBrowse {
                    cursor,
                    original_draft: current_draft,
                });
                Some(ComposerHistoryBrowseResult::Accepted(
                    lane.entries[cursor].clone(),
                ))
            }
        }
    }

    pub(super) fn browse_next(
        &mut self,
        scope: ComposerHistoryScope,
    ) -> Option<ComposerHistoryBrowseResult> {
        let lane = self.lanes.get_mut(&scope)?;
        let browse = lane.browse.as_mut()?;
        if browse.cursor + 1 < lane.entries.len() {
            browse.cursor += 1;
            return Some(ComposerHistoryBrowseResult::Accepted(
                lane.entries[browse.cursor].clone(),
            ));
        }

        let browse = lane.browse.take()?;
        Some(ComposerHistoryBrowseResult::Draft(browse.original_draft))
    }

    pub(super) fn bind_pending_new_thread_to_thread(
        &mut self,
        pending_scope_id: u64,
        thread_id: impl Into<String>,
    ) {
        let pending_scope = ComposerHistoryScope::PendingNewThread(pending_scope_id);
        let Some(pending_lane) = self.lanes.remove(&pending_scope) else {
            return;
        };

        let thread_scope = ComposerHistoryScope::Thread(thread_id.into());
        let capacity = self.capacity;
        let thread_lane = self.lanes.entry(thread_scope).or_default();
        thread_lane.browse = None;
        for entry in pending_lane.entries {
            push_accepted_entry(thread_lane, entry, capacity);
        }
    }
}

fn push_accepted_entry(
    lane: &mut ComposerHistoryLane,
    draft: AcceptedComposerDraft,
    capacity: usize,
) {
    if lane.entries.last() == Some(&draft) {
        return;
    }

    lane.entries.push(draft);
    let overflow = lane.entries.len().saturating_sub(capacity);
    if overflow > 0 {
        lane.entries.drain(0..overflow);
    }
}
