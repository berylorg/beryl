use std::collections::HashMap;

use super::composer_draft::{AcceptedComposerDraft, ComposerDraft, ComposerDraftRetainedCounts};

const DEFAULT_COMPOSER_HISTORY_CAPACITY: usize = 100;
const DEFAULT_COMPOSER_HISTORY_MAX_LANES: usize = 64;
pub(super) const DEFAULT_COMPOSER_HISTORY_MAX_IMAGE_BYTES: usize = 64 * 1024 * 1024;

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
    max_lanes: usize,
    max_image_bytes: usize,
    access_tick: u64,
    lanes: HashMap<ComposerHistoryScope, ComposerHistoryLane>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct ComposerHistoryLane {
    entries: Vec<AcceptedComposerDraft>,
    browse: Option<ComposerHistoryBrowse>,
    last_used: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ComposerHistoryBrowse {
    cursor: usize,
    original_draft: ComposerDraft,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct ComposerHistoryRetainedCounts {
    pub(super) lanes: usize,
    pub(super) entries: usize,
    pub(super) active_browses: usize,
    pub(super) display_text_bytes: usize,
    pub(super) part_text_bytes: usize,
    pub(super) image_count: usize,
    pub(super) image_bytes: usize,
    pub(super) image_label_bytes: usize,
    pub(super) image_asset_id_bytes: usize,
    pub(super) atom_count: usize,
    pub(super) atom_bytes: usize,
    pub(super) occurrence_count: usize,
    pub(super) occurrence_label_bytes: usize,
    pub(super) scope_bytes: usize,
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
            max_lanes: DEFAULT_COMPOSER_HISTORY_MAX_LANES,
            max_image_bytes: DEFAULT_COMPOSER_HISTORY_MAX_IMAGE_BYTES,
            access_tick: 0,
            lanes: HashMap::new(),
        }
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(super) fn with_limits(capacity: usize, max_lanes: usize, max_image_bytes: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            max_lanes: max_lanes.max(1),
            max_image_bytes: max_image_bytes.max(1),
            access_tick: 0,
            lanes: HashMap::new(),
        }
    }

    pub(super) fn record_accepted(
        &mut self,
        scope: ComposerHistoryScope,
        draft: AcceptedComposerDraft,
    ) {
        self.access_tick = self.access_tick.saturating_add(1);
        let capacity = self.capacity;
        let lane = self.lanes.entry(scope).or_default();
        lane.last_used = self.access_tick;
        lane.browse = None;
        push_accepted_entry(lane, draft, capacity);
        self.prune_if_needed();
    }

    pub(super) fn browse_previous(
        &mut self,
        scope: ComposerHistoryScope,
        current_draft: ComposerDraft,
    ) -> Option<ComposerHistoryBrowseResult> {
        self.access_tick = self.access_tick.saturating_add(1);
        let lane = self.lanes.get_mut(&scope)?;
        lane.last_used = self.access_tick;
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
        self.access_tick = self.access_tick.saturating_add(1);
        let lane = self.lanes.get_mut(&scope)?;
        lane.last_used = self.access_tick;
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
        self.access_tick = self.access_tick.saturating_add(1);
        let thread_lane = self.lanes.entry(thread_scope).or_default();
        thread_lane.last_used = self.access_tick;
        thread_lane.browse = None;
        for entry in pending_lane.entries {
            push_accepted_entry(thread_lane, entry, capacity);
        }
        self.prune_if_needed();
    }

    pub(super) fn retained_counts(&self) -> ComposerHistoryRetainedCounts {
        let mut counts = ComposerHistoryRetainedCounts {
            lanes: self.lanes.len(),
            scope_bytes: self
                .lanes
                .keys()
                .map(composer_history_scope_retained_bytes)
                .sum(),
            ..ComposerHistoryRetainedCounts::default()
        };

        for lane in self.lanes.values() {
            counts.entries = counts.entries.saturating_add(lane.entries.len());
            counts.active_browses = counts
                .active_browses
                .saturating_add(usize::from(lane.browse.is_some()));
            for entry in &lane.entries {
                counts.add_draft_counts(entry.retained_counts());
            }
            if let Some(browse) = lane.browse.as_ref() {
                counts.add_draft_counts(browse.original_draft.retained_counts());
            }
        }

        counts
    }

    fn prune_if_needed(&mut self) {
        while self.lanes.len() > self.max_lanes
            || self.retained_image_bytes() > self.max_image_bytes
        {
            let Some(scope) = self
                .lanes
                .iter()
                .min_by_key(|(_, lane)| lane.last_used)
                .map(|(scope, _)| scope.clone())
            else {
                break;
            };
            let mut remove_lane = false;
            if let Some(lane) = self.lanes.get_mut(&scope) {
                lane.browse = None;
                if !lane.entries.is_empty() {
                    lane.entries.remove(0);
                }
                remove_lane = lane.entries.is_empty();
            }
            if remove_lane || self.lanes.len() > self.max_lanes {
                self.lanes.remove(&scope);
            }
        }
    }

    fn retained_image_bytes(&self) -> usize {
        self.lanes
            .values()
            .map(|lane| {
                lane.entries
                    .iter()
                    .map(|entry| entry.retained_counts().image_bytes)
                    .sum::<usize>()
                    .saturating_add(
                        lane.browse
                            .as_ref()
                            .map(|browse| browse.original_draft.retained_counts().image_bytes)
                            .unwrap_or_default(),
                    )
            })
            .sum()
    }
}

impl ComposerHistoryRetainedCounts {
    fn add_draft_counts(&mut self, draft: ComposerDraftRetainedCounts) {
        self.display_text_bytes = self
            .display_text_bytes
            .saturating_add(draft.display_text_bytes);
        self.part_text_bytes = self.part_text_bytes.saturating_add(draft.part_text_bytes);
        self.image_count = self.image_count.saturating_add(draft.image_count);
        self.image_bytes = self.image_bytes.saturating_add(draft.image_bytes);
        self.image_label_bytes = self
            .image_label_bytes
            .saturating_add(draft.image_label_bytes);
        self.image_asset_id_bytes = self
            .image_asset_id_bytes
            .saturating_add(draft.image_asset_id_bytes);
        self.atom_count = self.atom_count.saturating_add(draft.atom_count);
        self.atom_bytes = self.atom_bytes.saturating_add(draft.atom_bytes);
        self.occurrence_count = self.occurrence_count.saturating_add(draft.occurrence_count);
        self.occurrence_label_bytes = self
            .occurrence_label_bytes
            .saturating_add(draft.occurrence_label_bytes);
    }
}

fn composer_history_scope_retained_bytes(scope: &ComposerHistoryScope) -> usize {
    match scope {
        ComposerHistoryScope::Thread(thread_id) => thread_id.len(),
        ComposerHistoryScope::PendingNewThread(_) => 0,
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
