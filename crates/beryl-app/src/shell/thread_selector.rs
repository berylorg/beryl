#[cfg(test)]
use std::collections::HashSet;

use beryl_model::{conversation::ConversationThreadId, workspace::WorkspaceId};
use gpui::{Bounds, Pixels, Point};

#[cfg(test)]
use crate::member_thread_inventory::{MemberThreadInventoryGroup, MemberThreadInventoryThread};
use crate::member_thread_inventory::{
    MemberThreadInventoryMemberKey, MemberThreadInventorySnapshot,
};

use super::column_selector::{ColumnSelectorColumn, ColumnSelectorState};

#[path = "thread_selector/projection.rs"]
mod projection;

#[allow(unused_imports)]
pub(crate) use projection::{ThreadSelectorProjection, ThreadSelectorProjectionThread};

#[derive(Clone, Debug)]
pub(crate) struct ThreadSelectorState {
    open: bool,
    anchor_bounds: Option<Bounds<Pixels>>,
    popup_bounds: Option<Bounds<Pixels>>,
    columns: ThreadSelectorColumns,
    projection: ThreadSelectorProjection,
    active_thread_id: Option<ConversationThreadId>,
}

pub(crate) type ThreadSelectorColumns =
    ColumnSelectorState<ThreadSelectorColumnKey, ThreadSelectorSelection, ()>;

pub(crate) type ThreadSelectorColumnState =
    ColumnSelectorColumn<ThreadSelectorColumnKey, ThreadSelectorSelection, ()>;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum ThreadSelectorColumnKey {
    Members,
    Threads {
        member_key: MemberThreadInventoryMemberKey,
        parent_thread_id: Option<ConversationThreadId>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ThreadSelectorSelection {
    Member(MemberThreadInventoryMemberKey),
    Thread(ConversationThreadId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ThreadSelectorThreadRowState {
    pub(crate) selected: bool,
    pub(crate) active: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ThreadSelectorActivationTarget {
    pub(crate) thread_id: ConversationThreadId,
    pub(crate) label: String,
    pub(crate) execution_target: WorkspaceId,
}

impl ThreadSelectorColumnKey {
    pub(crate) fn root_threads(member_key: MemberThreadInventoryMemberKey) -> Self {
        Self::Threads {
            member_key,
            parent_thread_id: None,
        }
    }

    fn child_threads(
        member_key: MemberThreadInventoryMemberKey,
        parent_thread_id: ConversationThreadId,
    ) -> Self {
        Self::Threads {
            member_key,
            parent_thread_id: Some(parent_thread_id),
        }
    }

    fn thread_member_key(&self) -> Option<&MemberThreadInventoryMemberKey> {
        match self {
            Self::Members => None,
            Self::Threads { member_key, .. } => Some(member_key),
        }
    }
}

impl Default for ThreadSelectorState {
    fn default() -> Self {
        Self {
            open: false,
            anchor_bounds: None,
            popup_bounds: None,
            columns: ThreadSelectorColumns::new(),
            projection: ThreadSelectorProjection::default(),
            active_thread_id: None,
        }
    }
}

impl ThreadSelectorState {
    pub(crate) fn is_open(&self) -> bool {
        self.open
    }

    pub(crate) fn columns(&self) -> &[ThreadSelectorColumnState] {
        self.columns.columns()
    }

    pub(crate) fn projection(&self) -> &ThreadSelectorProjection {
        &self.projection
    }

    pub(crate) fn anchor_bounds(&self) -> Option<Bounds<Pixels>> {
        self.anchor_bounds
    }

    pub(crate) fn toggle(
        &mut self,
        snapshot: &MemberThreadInventorySnapshot,
        active_thread_id: Option<ConversationThreadId>,
    ) -> bool {
        if self.open {
            self.close();
            false
        } else {
            self.open(snapshot, active_thread_id);
            true
        }
    }

    pub(crate) fn open(
        &mut self,
        snapshot: &MemberThreadInventorySnapshot,
        active_thread_id: Option<ConversationThreadId>,
    ) {
        self.open = true;
        self.popup_bounds = None;
        self.active_thread_id = active_thread_id;
        self.projection = ThreadSelectorProjection::new(snapshot);
        self.columns = initial_thread_selector_columns(snapshot);
        self.select_active_thread_path(snapshot);
    }

    pub(crate) fn close(&mut self) {
        self.open = false;
        self.popup_bounds = None;
        self.columns.clear();
        self.projection = ThreadSelectorProjection::default();
    }

    pub(crate) fn set_anchor_bounds(&mut self, bounds: Option<Bounds<Pixels>>) {
        self.anchor_bounds = bounds;
    }

    pub(crate) fn set_popup_bounds(&mut self, bounds: Option<Bounds<Pixels>>) {
        self.popup_bounds = bounds;
    }

    pub(crate) fn mark_active_thread(&mut self, active_thread_id: Option<ConversationThreadId>) {
        self.active_thread_id = active_thread_id;
    }

    pub(crate) fn reconcile_snapshot(&mut self, snapshot: &MemberThreadInventorySnapshot) {
        if !self.open {
            return;
        }

        let previous_member_selection = self.columns().iter().find_map(|column| {
            matches!(column.root_key(), ThreadSelectorColumnKey::Members)
                .then(|| column.selection())
                .flatten()
                .and_then(ThreadSelectorSelection::member_key)
                .cloned()
        });
        let previous_thread_selections = self
            .columns()
            .iter()
            .filter_map(|column| column.selection())
            .filter_map(ThreadSelectorSelection::thread_id)
            .cloned()
            .collect::<Vec<_>>();
        let had_selection =
            previous_member_selection.is_some() || !previous_thread_selections.is_empty();

        self.projection = ThreadSelectorProjection::new(snapshot);
        self.columns = initial_thread_selector_columns(snapshot);

        self.restore_selection_path(
            snapshot,
            previous_member_selection,
            &previous_thread_selections,
        );

        if !had_selection && !self.has_selection() {
            self.select_active_thread_path(snapshot);
        }
    }

    pub(crate) fn select_member(
        &mut self,
        column_index: usize,
        member_key: MemberThreadInventoryMemberKey,
    ) -> bool {
        self.columns.select_row(
            column_index,
            ThreadSelectorSelection::Member(member_key.clone()),
            Some(ThreadSelectorColumnKey::root_threads(member_key)),
        )
    }

    pub(crate) fn select_thread(
        &mut self,
        column_index: usize,
        thread_id: ConversationThreadId,
    ) -> bool {
        let next_root = self
            .columns()
            .get(column_index)
            .and_then(|column| column.root_key().thread_member_key())
            .cloned()
            .and_then(|member_key| {
                let child_column =
                    ThreadSelectorColumnKey::child_threads(member_key, thread_id.clone());
                (!self.projection.row_ids_for_column(&child_column).is_empty())
                    .then_some(child_column)
            });

        self.columns.select_row(
            column_index,
            ThreadSelectorSelection::Thread(thread_id),
            next_root,
        )
    }

    pub(crate) fn thread_row_state(
        &self,
        column_index: usize,
        thread_id: &ConversationThreadId,
    ) -> ThreadSelectorThreadRowState {
        let selected = self
            .columns()
            .get(column_index)
            .and_then(|column| column.selection())
            == Some(&ThreadSelectorSelection::Thread(thread_id.clone()));
        let active = self.active_thread_id.as_ref() == Some(thread_id);

        ThreadSelectorThreadRowState { selected, active }
    }

    pub(crate) fn selected_activation_target(&self) -> Option<ThreadSelectorActivationTarget> {
        let (member_key, thread_id) = self.columns().iter().rev().find_map(|column| {
            let member_key = column.root_key().thread_member_key()?;
            let thread_id = column.selection()?.thread_id()?;
            Some((member_key, thread_id))
        })?;

        self.projection
            .thread(member_key, thread_id)
            .map(|thread| ThreadSelectorActivationTarget {
                thread_id: thread.thread_id().clone(),
                label: thread.title().to_string(),
                execution_target: thread.execution_target().clone(),
            })
    }

    fn restore_selection_path(
        &mut self,
        snapshot: &MemberThreadInventorySnapshot,
        previous_member_selection: Option<MemberThreadInventoryMemberKey>,
        previous_thread_selections: &[ConversationThreadId],
    ) -> bool {
        let Some(member_key) = previous_member_selection
            .filter(|member_key| snapshot.group(member_key).is_some())
            .or_else(|| {
                previous_thread_selections
                    .first()
                    .and_then(|thread_id| self.projection.member_key_for_thread(thread_id))
            })
        else {
            return false;
        };

        let mut changed = false;
        let mut thread_column_index = 0;
        match snapshot.groups() {
            [group] if group.key() == &member_key => {}
            [..] => {
                if self.columns().first().is_none_or(|column| {
                    !matches!(column.root_key(), ThreadSelectorColumnKey::Members)
                }) {
                    return false;
                }
                changed |= self.select_member(0, member_key.clone());
                thread_column_index = 1;
            }
        }

        for thread_id in previous_thread_selections {
            let Some(column_key) = self
                .columns()
                .get(thread_column_index)
                .map(|column| column.root_key().clone())
            else {
                break;
            };
            if !self
                .projection
                .thread_exists_in_column(&column_key, thread_id)
            {
                break;
            }
            changed |= self.select_thread(thread_column_index, thread_id.clone());
            thread_column_index += 1;
        }

        changed
    }

    fn has_selection(&self) -> bool {
        self.columns()
            .iter()
            .any(|column| column.selection().is_some())
    }

    fn select_active_thread_path(&mut self, snapshot: &MemberThreadInventorySnapshot) -> bool {
        let Some(active_thread_id) = self.active_thread_id.clone() else {
            return false;
        };

        self.select_thread_path(snapshot, &active_thread_id)
    }

    fn select_thread_path(
        &mut self,
        snapshot: &MemberThreadInventorySnapshot,
        thread_id: &ConversationThreadId,
    ) -> bool {
        let Some(member_key) = self.projection.member_key_for_thread(thread_id) else {
            return false;
        };

        let Some(thread_path) = self
            .projection
            .thread_path_for_thread(&member_key, thread_id)
        else {
            return false;
        };

        if snapshot.groups().len() == 1 {
            let expected_root = ThreadSelectorColumnKey::root_threads(member_key);
            if self.columns.len() != 1
                || self
                    .columns()
                    .first()
                    .is_none_or(|column| column.root_key() != &expected_root)
            {
                self.columns = initial_thread_selector_columns(snapshot);
            }
            let mut changed = false;
            for (column_index, path_thread_id) in thread_path.into_iter().enumerate() {
                changed |= self.select_thread(column_index, path_thread_id);
            }
            return changed;
        }

        if self
            .columns()
            .first()
            .is_none_or(|column| !matches!(column.root_key(), ThreadSelectorColumnKey::Members))
        {
            self.columns = initial_thread_selector_columns(snapshot);
        }

        let mut changed = self.select_member(0, member_key);
        for (path_index, path_thread_id) in thread_path.into_iter().enumerate() {
            changed |= self.select_thread(path_index + 1, path_thread_id);
        }
        changed
    }

    pub(crate) fn should_dismiss_for_mouse_down(&self, position: Point<Pixels>) -> bool {
        self.open
            && !self
                .popup_bounds
                .is_some_and(|bounds| bounds.contains(&position))
            && !self
                .anchor_bounds
                .is_some_and(|bounds| bounds.contains(&position))
    }
}

impl ThreadSelectorSelection {
    pub(crate) fn member_key(&self) -> Option<&MemberThreadInventoryMemberKey> {
        match self {
            Self::Member(member_key) => Some(member_key),
            Self::Thread(_) => None,
        }
    }

    pub(crate) fn thread_id(&self) -> Option<&ConversationThreadId> {
        match self {
            Self::Member(_) => None,
            Self::Thread(thread_id) => Some(thread_id),
        }
    }
}

pub(crate) fn initial_thread_selector_columns(
    snapshot: &MemberThreadInventorySnapshot,
) -> ThreadSelectorColumns {
    let mut columns = ThreadSelectorColumns::new();
    match snapshot.groups() {
        [group] => columns.push_root(ThreadSelectorColumnKey::root_threads(group.key().clone())),
        [] | [_, ..] => columns.push_root(ThreadSelectorColumnKey::Members),
    }
    columns
}

#[cfg(test)]
#[allow(dead_code)]
pub(crate) fn thread_rows_for_column<'a>(
    snapshot: &'a MemberThreadInventorySnapshot,
    column_key: &ThreadSelectorColumnKey,
) -> Vec<&'a MemberThreadInventoryThread> {
    match column_key {
        ThreadSelectorColumnKey::Members => Vec::new(),
        ThreadSelectorColumnKey::Threads {
            member_key,
            parent_thread_id,
        } => snapshot
            .group(member_key)
            .map(|group| thread_rows_for_parent(group, parent_thread_id.as_ref()))
            .unwrap_or_default(),
    }
}

#[cfg(test)]
#[allow(dead_code)]
pub(crate) fn thread_direct_child_count(
    snapshot: &MemberThreadInventorySnapshot,
    member_key: &MemberThreadInventoryMemberKey,
    thread_id: &ConversationThreadId,
) -> usize {
    thread_rows_for_column(
        snapshot,
        &ThreadSelectorColumnKey::child_threads(member_key.clone(), thread_id.clone()),
    )
    .len()
}

#[cfg(test)]
#[allow(dead_code)]
fn thread_rows_for_parent<'a>(
    group: &'a MemberThreadInventoryGroup,
    parent_thread_id: Option<&ConversationThreadId>,
) -> Vec<&'a MemberThreadInventoryThread> {
    let all_threads = unique_group_threads(group);
    let mut rows = all_threads
        .iter()
        .copied()
        .filter(|thread| valid_parent_id_for_thread(&all_threads, thread) == parent_thread_id)
        .collect::<Vec<_>>();

    rows.sort_by(|left, right| {
        let left_activity = subtree_activity(&all_threads, left.thread_id());
        let right_activity = subtree_activity(&all_threads, right.thread_id());
        right_activity
            .cmp(&left_activity)
            .then_with(|| right.updated_at_millis().cmp(&left.updated_at_millis()))
            .then_with(|| right.created_at_millis().cmp(&left.created_at_millis()))
            .then_with(|| left.thread_id().as_str().cmp(right.thread_id().as_str()))
    });
    rows
}

#[cfg(test)]
#[allow(dead_code)]
fn unique_group_threads(group: &MemberThreadInventoryGroup) -> Vec<&MemberThreadInventoryThread> {
    let mut seen = HashSet::new();
    group
        .threads()
        .iter()
        .filter(|thread| seen.insert(thread.thread_id().clone()))
        .collect()
}

#[cfg(test)]
#[allow(dead_code)]
fn thread_row_exists_in_column(
    snapshot: &MemberThreadInventorySnapshot,
    column_key: &ThreadSelectorColumnKey,
    thread_id: &ConversationThreadId,
) -> bool {
    thread_rows_for_column(snapshot, column_key)
        .iter()
        .any(|thread| thread.thread_id() == thread_id)
}

#[cfg(test)]
#[allow(dead_code)]
fn valid_parent_id_for_thread<'a>(
    all_threads: &[&'a MemberThreadInventoryThread],
    thread: &'a MemberThreadInventoryThread,
) -> Option<&'a ConversationThreadId> {
    let parent_id = thread.forked_from_id()?;
    if parent_id == thread.thread_id() || thread_by_id(all_threads, parent_id).is_none() {
        return None;
    }

    let mut seen = HashSet::new();
    seen.insert(thread.thread_id().clone());
    let mut cursor = parent_id.clone();
    while let Some(parent_thread) = thread_by_id(all_threads, &cursor) {
        if !seen.insert(parent_thread.thread_id().clone()) {
            return None;
        }

        let Some(next_parent_id) = parent_thread.forked_from_id() else {
            break;
        };
        if next_parent_id == parent_thread.thread_id()
            || thread_by_id(all_threads, next_parent_id).is_none()
        {
            break;
        }
        cursor = next_parent_id.clone();
    }

    Some(parent_id)
}

#[cfg(test)]
#[allow(dead_code)]
fn subtree_activity(
    all_threads: &[&MemberThreadInventoryThread],
    thread_id: &ConversationThreadId,
) -> (i64, i64) {
    let mut visited = HashSet::new();
    subtree_activity_inner(all_threads, thread_id, &mut visited)
}

#[cfg(test)]
#[allow(dead_code)]
fn subtree_activity_inner(
    all_threads: &[&MemberThreadInventoryThread],
    thread_id: &ConversationThreadId,
    visited: &mut HashSet<ConversationThreadId>,
) -> (i64, i64) {
    if !visited.insert(thread_id.clone()) {
        return (i64::MIN, i64::MIN);
    }

    let Some(thread) = thread_by_id(all_threads, thread_id) else {
        return (i64::MIN, i64::MIN);
    };

    let mut activity = (thread.updated_at_millis(), thread.created_at_millis());
    for child in all_threads {
        if valid_parent_id_for_thread(all_threads, child) == Some(thread_id) {
            activity = activity.max(subtree_activity_inner(
                all_threads,
                child.thread_id(),
                visited,
            ));
        }
    }
    activity
}

#[cfg(test)]
#[allow(dead_code)]
fn thread_by_id<'a>(
    all_threads: &[&'a MemberThreadInventoryThread],
    thread_id: &ConversationThreadId,
) -> Option<&'a MemberThreadInventoryThread> {
    all_threads
        .iter()
        .copied()
        .find(|thread| thread.thread_id() == thread_id)
}

#[cfg(test)]
#[allow(dead_code)]
fn member_key_for_thread(
    snapshot: &MemberThreadInventorySnapshot,
    thread_id: &ConversationThreadId,
) -> Option<MemberThreadInventoryMemberKey> {
    snapshot.groups().iter().find_map(|group| {
        group
            .threads()
            .iter()
            .any(|thread| thread.thread_id() == thread_id)
            .then(|| group.key().clone())
    })
}

#[cfg(test)]
#[allow(dead_code)]
fn thread_path_for_thread(
    snapshot: &MemberThreadInventorySnapshot,
    member_key: &MemberThreadInventoryMemberKey,
    thread_id: &ConversationThreadId,
) -> Option<Vec<ConversationThreadId>> {
    let group = snapshot.group(member_key)?;
    let all_threads = unique_group_threads(group);
    let mut reversed_path = Vec::new();
    let mut cursor = thread_id.clone();

    loop {
        let thread = thread_by_id(&all_threads, &cursor)?;
        reversed_path.push(thread.thread_id().clone());
        let Some(parent_id) = valid_parent_id_for_thread(&all_threads, thread) else {
            break;
        };
        cursor = parent_id.clone();
    }

    reversed_path.reverse();
    Some(reversed_path)
}
