use std::collections::{HashMap, HashSet};

use beryl_model::{conversation::ConversationThreadId, workspace::WorkspaceId};

use crate::member_thread_inventory::{
    MemberThreadInventoryGroup, MemberThreadInventoryMemberKey, MemberThreadInventorySnapshot,
    MemberThreadInventoryThread,
};

use super::ThreadSelectorColumnKey;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ThreadSelectorProjection {
    members: Vec<ThreadSelectorProjectionMember>,
    member_indices: HashMap<MemberThreadInventoryMemberKey, usize>,
    member_key_by_thread: HashMap<ConversationThreadId, MemberThreadInventoryMemberKey>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ThreadSelectorProjectionThread {
    thread_id: ConversationThreadId,
    title: String,
    execution_target: WorkspaceId,
    created_at_millis: i64,
    updated_at_millis: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ThreadSelectorProjectionMember {
    threads_by_id: HashMap<ConversationThreadId, ThreadSelectorProjectionThread>,
    rows_by_parent: HashMap<Option<ConversationThreadId>, Vec<ConversationThreadId>>,
    direct_child_counts: HashMap<ConversationThreadId, usize>,
    child_count_digit_count_by_parent: HashMap<Option<ConversationThreadId>, usize>,
    valid_parent_by_thread: HashMap<ConversationThreadId, Option<ConversationThreadId>>,
}

impl ThreadSelectorProjection {
    pub(crate) fn new(snapshot: &MemberThreadInventorySnapshot) -> Self {
        let mut members = Vec::new();
        let mut member_indices = HashMap::new();
        let mut member_key_by_thread = HashMap::new();

        for group in snapshot.groups() {
            let index = members.len();
            let member = ThreadSelectorProjectionMember::new(group);
            for thread_id in member.thread_ids() {
                member_key_by_thread
                    .entry(thread_id.clone())
                    .or_insert_with(|| group.key().clone());
            }
            member_indices.entry(group.key().clone()).or_insert(index);
            members.push(member);
        }

        Self {
            members,
            member_indices,
            member_key_by_thread,
        }
    }

    pub(crate) fn row_ids_for_column(
        &self,
        column_key: &ThreadSelectorColumnKey,
    ) -> &[ConversationThreadId] {
        match column_key {
            ThreadSelectorColumnKey::Members => &[],
            ThreadSelectorColumnKey::Threads {
                member_key,
                parent_thread_id,
            } => self
                .member(member_key)
                .and_then(|member| member.row_ids_for_parent(parent_thread_id.as_ref()))
                .unwrap_or(&[]),
        }
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn thread_rows_for_column(
        &self,
        column_key: &ThreadSelectorColumnKey,
    ) -> Vec<&ThreadSelectorProjectionThread> {
        let ThreadSelectorColumnKey::Threads { member_key, .. } = column_key else {
            return Vec::new();
        };
        let Some(member) = self.member(member_key) else {
            return Vec::new();
        };

        self.row_ids_for_column(column_key)
            .iter()
            .filter_map(|thread_id| member.thread(thread_id))
            .collect()
    }

    pub(crate) fn direct_child_count(
        &self,
        member_key: &MemberThreadInventoryMemberKey,
        thread_id: &ConversationThreadId,
    ) -> usize {
        self.member(member_key)
            .map(|member| member.direct_child_count(thread_id))
            .unwrap_or(0)
    }

    pub(crate) fn child_count_digit_count_for_column(
        &self,
        column_key: &ThreadSelectorColumnKey,
    ) -> Option<usize> {
        let ThreadSelectorColumnKey::Threads {
            member_key,
            parent_thread_id,
        } = column_key
        else {
            return None;
        };

        self.member(member_key)
            .and_then(|member| member.child_count_digit_count(parent_thread_id.as_ref()))
    }

    pub(crate) fn thread_exists_in_column(
        &self,
        column_key: &ThreadSelectorColumnKey,
        thread_id: &ConversationThreadId,
    ) -> bool {
        self.row_ids_for_column(column_key)
            .iter()
            .any(|row_id| row_id == thread_id)
    }

    pub(crate) fn member_key_for_thread(
        &self,
        thread_id: &ConversationThreadId,
    ) -> Option<MemberThreadInventoryMemberKey> {
        self.member_key_by_thread.get(thread_id).cloned()
    }

    pub(crate) fn thread_path_for_thread(
        &self,
        member_key: &MemberThreadInventoryMemberKey,
        thread_id: &ConversationThreadId,
    ) -> Option<Vec<ConversationThreadId>> {
        self.member(member_key)?.thread_path_for_thread(thread_id)
    }

    pub(crate) fn thread(
        &self,
        member_key: &MemberThreadInventoryMemberKey,
        thread_id: &ConversationThreadId,
    ) -> Option<&ThreadSelectorProjectionThread> {
        self.member(member_key)?.thread(thread_id)
    }

    fn member(
        &self,
        member_key: &MemberThreadInventoryMemberKey,
    ) -> Option<&ThreadSelectorProjectionMember> {
        self.member_indices
            .get(member_key)
            .and_then(|index| self.members.get(*index))
    }
}

impl ThreadSelectorProjectionThread {
    pub(crate) fn thread_id(&self) -> &ConversationThreadId {
        &self.thread_id
    }

    pub(crate) fn title(&self) -> &str {
        &self.title
    }

    pub(crate) fn execution_target(&self) -> &WorkspaceId {
        &self.execution_target
    }
}

impl ThreadSelectorProjectionMember {
    fn new(group: &MemberThreadInventoryGroup) -> Self {
        let mut thread_ids = Vec::new();
        let mut threads_by_id = HashMap::new();
        let mut raw_parent_by_thread = HashMap::new();

        for thread in group.threads() {
            let thread_id = thread.thread_id().clone();
            if threads_by_id.contains_key(&thread_id) {
                continue;
            }

            thread_ids.push(thread_id.clone());
            raw_parent_by_thread.insert(thread_id.clone(), thread.forked_from_id().cloned());
            threads_by_id.insert(thread_id, ThreadSelectorProjectionThread::new(thread));
        }

        let mut valid_parent_by_thread = HashMap::new();
        let mut seen = HashSet::new();
        for thread_id in &thread_ids {
            let valid_parent = valid_parent_for_thread(thread_id, &raw_parent_by_thread, &mut seen);
            valid_parent_by_thread.insert(thread_id.clone(), valid_parent);
        }

        let mut rows_by_parent: HashMap<Option<ConversationThreadId>, Vec<ConversationThreadId>> =
            HashMap::new();
        for thread_id in &thread_ids {
            let parent_id = valid_parent_by_thread
                .get(thread_id)
                .cloned()
                .unwrap_or(None);
            rows_by_parent
                .entry(parent_id)
                .or_default()
                .push(thread_id.clone());
        }

        let mut subtree_activity_by_thread = HashMap::new();
        for thread_id in &thread_ids {
            compute_subtree_activity(
                thread_id,
                &threads_by_id,
                &rows_by_parent,
                &mut subtree_activity_by_thread,
            );
        }

        for row_ids in rows_by_parent.values_mut() {
            row_ids.sort_by(|left_id, right_id| {
                compare_thread_rows(
                    left_id,
                    right_id,
                    &threads_by_id,
                    &subtree_activity_by_thread,
                )
            });
        }

        let mut direct_child_counts = HashMap::new();
        for thread_id in &thread_ids {
            let parent_key = Some(thread_id.clone());
            direct_child_counts.insert(
                thread_id.clone(),
                rows_by_parent.get(&parent_key).map_or(0, Vec::len),
            );
        }

        let mut child_count_digit_count_by_parent = HashMap::new();
        for (parent_id, row_ids) in &rows_by_parent {
            if let Some(digit_count) = row_ids
                .iter()
                .filter_map(|row_id| {
                    let count = direct_child_counts.get(row_id).copied().unwrap_or(0);
                    (count > 0).then(|| child_count_digit_count(count))
                })
                .max()
            {
                child_count_digit_count_by_parent.insert(parent_id.clone(), digit_count);
            }
        }

        Self {
            threads_by_id,
            rows_by_parent,
            direct_child_counts,
            child_count_digit_count_by_parent,
            valid_parent_by_thread,
        }
    }

    fn thread_ids(&self) -> impl Iterator<Item = &ConversationThreadId> {
        self.threads_by_id.keys()
    }

    fn row_ids_for_parent(
        &self,
        parent_thread_id: Option<&ConversationThreadId>,
    ) -> Option<&[ConversationThreadId]> {
        let parent_key = parent_thread_id.cloned();
        self.rows_by_parent.get(&parent_key).map(Vec::as_slice)
    }

    fn direct_child_count(&self, thread_id: &ConversationThreadId) -> usize {
        self.direct_child_counts
            .get(thread_id)
            .copied()
            .unwrap_or(0)
    }

    fn child_count_digit_count(
        &self,
        parent_thread_id: Option<&ConversationThreadId>,
    ) -> Option<usize> {
        self.child_count_digit_count_by_parent
            .get(&parent_thread_id.cloned())
            .copied()
    }

    fn thread(&self, thread_id: &ConversationThreadId) -> Option<&ThreadSelectorProjectionThread> {
        self.threads_by_id.get(thread_id)
    }

    fn thread_path_for_thread(
        &self,
        thread_id: &ConversationThreadId,
    ) -> Option<Vec<ConversationThreadId>> {
        if !self.threads_by_id.contains_key(thread_id) {
            return None;
        }

        let mut reversed_path = Vec::new();
        let mut cursor = thread_id.clone();
        loop {
            reversed_path.push(cursor.clone());
            let Some(parent_id) = self.valid_parent_by_thread.get(&cursor).cloned().flatten()
            else {
                break;
            };
            cursor = parent_id;
        }

        reversed_path.reverse();
        Some(reversed_path)
    }
}

impl ThreadSelectorProjectionThread {
    fn new(thread: &MemberThreadInventoryThread) -> Self {
        Self {
            thread_id: thread.thread_id().clone(),
            title: thread.title().to_string(),
            execution_target: thread.execution_target().clone(),
            created_at_millis: thread.created_at_millis(),
            updated_at_millis: thread.updated_at_millis(),
        }
    }
}

fn valid_parent_for_thread(
    thread_id: &ConversationThreadId,
    raw_parent_by_thread: &HashMap<ConversationThreadId, Option<ConversationThreadId>>,
    seen: &mut HashSet<ConversationThreadId>,
) -> Option<ConversationThreadId> {
    seen.clear();
    let parent_id = raw_parent_by_thread.get(thread_id).cloned().flatten()?;
    if parent_id == *thread_id || !raw_parent_by_thread.contains_key(&parent_id) {
        return None;
    }

    seen.insert(thread_id.clone());
    let mut cursor = parent_id.clone();
    while let Some(parent_thread_parent_id) = raw_parent_by_thread.get(&cursor) {
        if !seen.insert(cursor.clone()) {
            return None;
        }

        let Some(next_parent_id) = parent_thread_parent_id.clone() else {
            break;
        };
        if next_parent_id == cursor || !raw_parent_by_thread.contains_key(&next_parent_id) {
            break;
        }
        cursor = next_parent_id;
    }

    Some(parent_id)
}

fn compute_subtree_activity(
    thread_id: &ConversationThreadId,
    threads_by_id: &HashMap<ConversationThreadId, ThreadSelectorProjectionThread>,
    rows_by_parent: &HashMap<Option<ConversationThreadId>, Vec<ConversationThreadId>>,
    subtree_activity_by_thread: &mut HashMap<ConversationThreadId, (i64, i64)>,
) -> (i64, i64) {
    if let Some(activity) = subtree_activity_by_thread.get(thread_id).copied() {
        return activity;
    }

    let Some(thread) = threads_by_id.get(thread_id) else {
        return (i64::MIN, i64::MIN);
    };

    let mut activity = (thread.updated_at_millis, thread.created_at_millis);
    let parent_key = Some(thread_id.clone());
    if let Some(child_ids) = rows_by_parent.get(&parent_key) {
        for child_id in child_ids {
            activity = activity.max(compute_subtree_activity(
                child_id,
                threads_by_id,
                rows_by_parent,
                subtree_activity_by_thread,
            ));
        }
    }

    subtree_activity_by_thread.insert(thread_id.clone(), activity);
    activity
}

fn compare_thread_rows(
    left_id: &ConversationThreadId,
    right_id: &ConversationThreadId,
    threads_by_id: &HashMap<ConversationThreadId, ThreadSelectorProjectionThread>,
    subtree_activity_by_thread: &HashMap<ConversationThreadId, (i64, i64)>,
) -> std::cmp::Ordering {
    let left_thread = threads_by_id
        .get(left_id)
        .expect("projection row id should reference a known thread");
    let right_thread = threads_by_id
        .get(right_id)
        .expect("projection row id should reference a known thread");
    let left_activity = subtree_activity_by_thread
        .get(left_id)
        .copied()
        .unwrap_or((left_thread.updated_at_millis, left_thread.created_at_millis));
    let right_activity = subtree_activity_by_thread
        .get(right_id)
        .copied()
        .unwrap_or((
            right_thread.updated_at_millis,
            right_thread.created_at_millis,
        ));

    right_activity
        .cmp(&left_activity)
        .then_with(|| {
            right_thread
                .updated_at_millis
                .cmp(&left_thread.updated_at_millis)
        })
        .then_with(|| {
            right_thread
                .created_at_millis
                .cmp(&left_thread.created_at_millis)
        })
        .then_with(|| left_id.as_str().cmp(right_id.as_str()))
}

fn child_count_digit_count(count: usize) -> usize {
    count.to_string().len()
}
