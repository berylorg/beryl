use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use beryl_backend::{JsonRpcError, ManagedBackendError, ThreadSummary};
use beryl_model::{
    conversation::{
        ConversationThreadId, RegisteredConversationThread, WorkspaceConversationState,
    },
    workspace::{BerylWorkspaceId, RuntimeMode, WorkspaceId, WorkspaceMemberId},
};
use tracing::warn;

pub(crate) const MEMBER_THREAD_INVENTORY_MAX_BACKEND_THREADS: usize = 2048;
const MEMBER_THREAD_INVENTORY_MAX_PARTIAL_REASONS: usize = 8;
const MEMBER_THREAD_INVENTORY_MAX_PARTIAL_REASON_BYTES: usize = 512;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum MemberThreadInventoryMemberKey {
    Explicit(WorkspaceMemberId),
    ImplicitHome,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum MemberThreadInventoryMemberKind {
    Explicit,
    ImplicitHome,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MemberThreadInventorySnapshot {
    workspace_id: BerylWorkspaceId,
    refreshed_at_millis: u64,
    coverage: MemberThreadInventoryCoverage,
    groups: Vec<MemberThreadInventoryGroup>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct MemberThreadInventoryPartialCoverage {
    row_coverage_truncated: bool,
    lineage_incomplete: bool,
    reasons: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) enum MemberThreadInventoryCoverage {
    #[default]
    Complete,
    Partial(MemberThreadInventoryPartialCoverage),
}

impl MemberThreadInventoryPartialCoverage {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn with_row_coverage_truncated(mut self, reason: impl Into<String>) -> Self {
        self.row_coverage_truncated = true;
        self.push_reason(reason);
        self
    }

    pub(crate) fn with_lineage_incomplete(mut self, reason: impl Into<String>) -> Self {
        self.lineage_incomplete = true;
        self.push_reason(reason);
        self
    }

    pub(crate) fn row_coverage_truncated(&self) -> bool {
        self.row_coverage_truncated
    }

    pub(crate) fn lineage_incomplete(&self) -> bool {
        self.lineage_incomplete
    }

    pub(crate) fn reasons(&self) -> &[String] {
        &self.reasons
    }

    pub(crate) fn merge(&mut self, other: Self) {
        self.row_coverage_truncated |= other.row_coverage_truncated;
        self.lineage_incomplete |= other.lineage_incomplete;
        for reason in other.reasons {
            self.push_reason(reason);
        }
    }

    fn push_reason(&mut self, reason: impl Into<String>) {
        let mut reason = reason.into();
        truncate_string_to_utf8_boundary(
            &mut reason,
            MEMBER_THREAD_INVENTORY_MAX_PARTIAL_REASON_BYTES,
        );
        if self.reasons.len() < MEMBER_THREAD_INVENTORY_MAX_PARTIAL_REASONS
            && !self.reasons.contains(&reason)
        {
            self.reasons.push(reason);
        }
    }
}

fn truncate_string_to_utf8_boundary(value: &mut String, max_bytes: usize) {
    if value.len() <= max_bytes {
        return;
    }
    let mut boundary = max_bytes;
    while !value.is_char_boundary(boundary) {
        boundary -= 1;
    }
    value.truncate(boundary);
}

impl MemberThreadInventoryCoverage {
    pub(crate) fn partial(&self) -> Option<&MemberThreadInventoryPartialCoverage> {
        match self {
            Self::Complete => None,
            Self::Partial(partial) => Some(partial),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MemberThreadInventoryGroup {
    key: MemberThreadInventoryMemberKey,
    kind: MemberThreadInventoryMemberKind,
    label: String,
    runtime: RuntimeMode,
    canonical_path: Option<PathBuf>,
    threads: Vec<MemberThreadInventoryThread>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MemberThreadInventoryThread {
    thread_id: ConversationThreadId,
    forked_from_id: Option<ConversationThreadId>,
    title: String,
    execution_target: WorkspaceId,
    preview: String,
    backend_name: Option<String>,
    created_at_millis: i64,
    updated_at_millis: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MemberThreadInventoryBackendThread {
    runtime: RuntimeMode,
    summary: ThreadSummary,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MemberThreadInventoryEvent {
    MemberSetChanged,
    SelectorFreshnessRequested,
    BackendTargetOpening,
    BackendTargetAvailable,
    InventoryContentsChanged,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct MemberThreadInventoryRefreshToken {
    generation: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MemberThreadInventoryState {
    snapshot: MemberThreadInventorySnapshot,
    generation: u64,
    refresh_needed: bool,
    refreshing: bool,
    last_error: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct MemberThreadInventoryRetainedCounts {
    pub(crate) groups: usize,
    pub(crate) threads: usize,
    pub(crate) payload_bytes: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ThreadForkParentMetadataReadError {
    ThreadUnavailable(String),
    Fatal(String),
}

impl MemberThreadInventorySnapshot {
    pub(crate) fn empty_for_workspace(
        workspace_id: BerylWorkspaceId,
        workspace_state: &WorkspaceConversationState,
    ) -> Self {
        Self {
            workspace_id,
            refreshed_at_millis: 0,
            coverage: MemberThreadInventoryCoverage::Complete,
            groups: empty_groups_for_workspace_state(workspace_state),
        }
    }

    #[allow(dead_code)]
    pub(crate) fn new(
        workspace_id: BerylWorkspaceId,
        refreshed_at_millis: u64,
        groups: Vec<MemberThreadInventoryGroup>,
    ) -> Self {
        Self::new_with_coverage(
            workspace_id,
            refreshed_at_millis,
            MemberThreadInventoryCoverage::Complete,
            groups,
        )
    }

    pub(crate) fn new_with_coverage(
        workspace_id: BerylWorkspaceId,
        refreshed_at_millis: u64,
        coverage: MemberThreadInventoryCoverage,
        groups: Vec<MemberThreadInventoryGroup>,
    ) -> Self {
        Self {
            workspace_id,
            refreshed_at_millis,
            coverage,
            groups,
        }
    }

    pub(crate) fn groups(&self) -> &[MemberThreadInventoryGroup] {
        &self.groups
    }

    pub(crate) fn workspace_id(&self) -> &BerylWorkspaceId {
        &self.workspace_id
    }

    pub(crate) fn refreshed_at_millis(&self) -> u64 {
        self.refreshed_at_millis
    }

    #[allow(dead_code)]
    pub(crate) fn coverage(&self) -> &MemberThreadInventoryCoverage {
        &self.coverage
    }

    pub(crate) fn partial_status_message(&self) -> Option<String> {
        let partial = self.coverage.partial()?;
        let scope = match (
            partial.row_coverage_truncated(),
            partial.lineage_incomplete(),
        ) {
            (true, true) => "Thread rows and lineage are incomplete.",
            (true, false) => "Thread rows are incomplete.",
            (false, true) => "Thread lineage is incomplete.",
            (false, false) => "Thread inventory is incomplete.",
        };
        match partial.reasons().first() {
            Some(reason) => Some(format!("{scope} {reason}")),
            None => Some(scope.to_string()),
        }
    }

    pub(crate) fn retained_counts(&self) -> MemberThreadInventoryRetainedCounts {
        let threads = self
            .groups
            .iter()
            .map(|group| group.threads.len())
            .sum::<usize>();
        let payload_bytes = self
            .groups
            .iter()
            .map(|group| {
                group.label.len()
                    + group
                        .canonical_path
                        .as_ref()
                        .map_or(0, |path| path.to_string_lossy().len())
                    + group
                        .threads
                        .iter()
                        .map(|thread| {
                            thread.thread_id.as_str().len()
                                + thread
                                    .forked_from_id
                                    .as_ref()
                                    .map_or(0, |id| id.as_str().len())
                                + thread.title.len()
                                + thread.execution_target.display_label().len()
                                + thread.preview.len()
                                + thread.backend_name.as_ref().map_or(0, String::len)
                        })
                        .sum::<usize>()
            })
            .sum::<usize>()
            + self
                .coverage
                .partial()
                .map_or(0, |partial| partial.reasons().iter().map(String::len).sum());
        MemberThreadInventoryRetainedCounts {
            groups: self.groups.len(),
            threads,
            payload_bytes,
        }
    }

    pub(crate) fn group(
        &self,
        key: &MemberThreadInventoryMemberKey,
    ) -> Option<&MemberThreadInventoryGroup> {
        self.groups.iter().find(|group| group.key() == key)
    }

    pub(crate) fn update_thread_backend_name(
        &mut self,
        workspace_state: &WorkspaceConversationState,
        thread_id: &ConversationThreadId,
        backend_name: Option<&str>,
    ) -> bool {
        let backend_name = unsuppressed_inventory_backend_name(
            workspace_state,
            thread_id,
            normalized_thread_title(backend_name),
        );
        let mut changed = false;
        for group in &mut self.groups {
            for thread in &mut group.threads {
                if thread.thread_id() == thread_id {
                    changed |= thread.update_backend_name(workspace_state, backend_name.clone());
                }
            }
        }
        changed
    }

    pub(crate) fn refresh_thread_titles(
        &mut self,
        workspace_state: &WorkspaceConversationState,
    ) -> bool {
        let mut changed = false;
        for group in &mut self.groups {
            for thread in &mut group.threads {
                changed |= thread.suppress_ignored_backend_name(workspace_state);
                changed |= thread.update_title(workspace_state);
            }
        }
        changed
    }
}

impl MemberThreadInventoryState {
    pub(crate) fn new(
        workspace_id: BerylWorkspaceId,
        workspace_state: &WorkspaceConversationState,
    ) -> Self {
        Self {
            snapshot: MemberThreadInventorySnapshot::empty_for_workspace(
                workspace_id,
                workspace_state,
            ),
            generation: 0,
            refresh_needed: workspace_state.selected_runtime().is_some(),
            refreshing: false,
            last_error: None,
        }
    }

    pub(crate) fn snapshot(&self) -> &MemberThreadInventorySnapshot {
        &self.snapshot
    }

    pub(crate) fn refreshing(&self) -> bool {
        self.refreshing
    }

    pub(crate) fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    pub(crate) fn needs_refresh(&self) -> bool {
        self.refresh_needed && !self.refreshing
    }

    pub(crate) fn refresh_token(&self) -> MemberThreadInventoryRefreshToken {
        MemberThreadInventoryRefreshToken {
            generation: self.generation,
        }
    }

    pub(crate) fn begin_refresh(&mut self) -> MemberThreadInventoryRefreshToken {
        self.refreshing = true;
        self.refresh_needed = false;
        self.last_error = None;
        self.refresh_token()
    }

    #[allow(dead_code)]
    pub(crate) fn finish_refresh(
        &mut self,
        snapshot: MemberThreadInventorySnapshot,
        workspace_state: &WorkspaceConversationState,
    ) {
        let token = self.refresh_token();
        self.refresh_needed = false;
        let _ = self.finish_refresh_for_token(token, snapshot, workspace_state);
    }

    pub(crate) fn finish_refresh_for_token(
        &mut self,
        token: MemberThreadInventoryRefreshToken,
        mut snapshot: MemberThreadInventorySnapshot,
        workspace_state: &WorkspaceConversationState,
    ) -> bool {
        if token != self.refresh_token() {
            return false;
        }

        let refresh_requested_after_begin = self.refresh_needed;
        snapshot.refresh_thread_titles(workspace_state);
        let has_refreshable_groups = !snapshot.groups().is_empty();
        self.snapshot = snapshot;
        self.refresh_needed = refresh_requested_after_begin && has_refreshable_groups;
        self.refreshing = false;
        self.last_error = None;
        true
    }

    pub(crate) fn fail_refresh_for_token(
        &mut self,
        token: MemberThreadInventoryRefreshToken,
        error: impl Into<String>,
    ) -> bool {
        if token != self.refresh_token() {
            return false;
        }

        let refresh_requested_after_begin = self.refresh_needed;
        self.refreshing = false;
        self.refresh_needed = refresh_requested_after_begin && !self.snapshot.groups().is_empty();
        self.last_error = Some(error.into());
        true
    }

    #[allow(dead_code)]
    pub(crate) fn abandon_refresh_for_backend_reopen(&mut self, error: impl Into<String>) {
        self.refreshing = false;
        self.refresh_needed = !self.snapshot.groups().is_empty();
        self.last_error = Some(error.into());
    }

    pub(crate) fn prepare_for_backend_reopen(&mut self) {
        if self.refreshing {
            self.refresh_needed = !self.snapshot.groups().is_empty();
        }
        self.refreshing = false;
        self.generation = self.generation.wrapping_add(1);
    }

    pub(crate) fn mark_refresh_needed(&mut self) {
        if !self.snapshot.groups().is_empty() {
            self.refresh_needed = true;
        }
    }

    pub(crate) fn update_thread_backend_name(
        &mut self,
        workspace_state: &WorkspaceConversationState,
        thread_id: &ConversationThreadId,
        backend_name: Option<&str>,
    ) -> bool {
        self.snapshot
            .update_thread_backend_name(workspace_state, thread_id, backend_name)
    }

    pub(crate) fn reset_for_workspace_state(
        &mut self,
        workspace_id: BerylWorkspaceId,
        workspace_state: &WorkspaceConversationState,
    ) {
        let generation = self.generation.wrapping_add(1);
        *self = Self::new(workspace_id, workspace_state);
        self.generation = generation;
    }

    pub(crate) fn rekey_workspace_id(&mut self, workspace_id: BerylWorkspaceId) {
        self.snapshot.workspace_id = workspace_id;
    }

    pub(crate) fn apply_event(
        &mut self,
        event: MemberThreadInventoryEvent,
        workspace_id: BerylWorkspaceId,
        workspace_state: &WorkspaceConversationState,
    ) {
        match event {
            MemberThreadInventoryEvent::MemberSetChanged => {
                self.reset_for_workspace_state(workspace_id, workspace_state);
            }
            MemberThreadInventoryEvent::SelectorFreshnessRequested
            | MemberThreadInventoryEvent::BackendTargetAvailable
            | MemberThreadInventoryEvent::InventoryContentsChanged => {
                self.mark_refresh_needed();
            }
            MemberThreadInventoryEvent::BackendTargetOpening => {
                self.prepare_for_backend_reopen();
            }
        }
    }
}

impl MemberThreadInventoryGroup {
    pub(crate) fn new(
        key: MemberThreadInventoryMemberKey,
        kind: MemberThreadInventoryMemberKind,
        label: impl Into<String>,
        runtime: RuntimeMode,
        canonical_path: Option<PathBuf>,
        threads: Vec<MemberThreadInventoryThread>,
    ) -> Self {
        Self {
            key,
            kind,
            label: label.into(),
            runtime,
            canonical_path,
            threads,
        }
    }

    pub(crate) fn key(&self) -> &MemberThreadInventoryMemberKey {
        &self.key
    }

    pub(crate) fn kind(&self) -> &MemberThreadInventoryMemberKind {
        &self.kind
    }

    pub(crate) fn label(&self) -> &str {
        &self.label
    }

    pub(crate) fn runtime(&self) -> &RuntimeMode {
        &self.runtime
    }

    pub(crate) fn canonical_path(&self) -> Option<&Path> {
        self.canonical_path.as_deref()
    }

    pub(crate) fn threads(&self) -> &[MemberThreadInventoryThread] {
        &self.threads
    }
}

impl MemberThreadInventoryThread {
    pub(crate) fn thread_id(&self) -> &ConversationThreadId {
        &self.thread_id
    }

    pub(crate) fn forked_from_id(&self) -> Option<&ConversationThreadId> {
        self.forked_from_id.as_ref()
    }

    pub(crate) fn created_at_millis(&self) -> i64 {
        self.created_at_millis
    }

    pub(crate) fn updated_at_millis(&self) -> i64 {
        self.updated_at_millis
    }

    pub(crate) fn title(&self) -> &str {
        &self.title
    }

    pub(crate) fn execution_target(&self) -> &WorkspaceId {
        &self.execution_target
    }

    pub(crate) fn to_registered_thread(&self) -> RegisteredConversationThread {
        RegisteredConversationThread::new(
            self.thread_id.clone(),
            self.execution_target.clone(),
            self.preview.clone(),
            self.backend_name.clone(),
            self.created_at_millis,
            self.updated_at_millis,
        )
    }

    fn update_backend_name(
        &mut self,
        workspace_state: &WorkspaceConversationState,
        backend_name: Option<String>,
    ) -> bool {
        let title = self.resolved_title(workspace_state, backend_name.as_deref());
        if self.backend_name == backend_name && self.title == title {
            return false;
        }

        self.backend_name = backend_name;
        self.title = title;
        true
    }

    fn update_title(&mut self, workspace_state: &WorkspaceConversationState) -> bool {
        let title = self.resolved_title(workspace_state, self.backend_name.as_deref());
        if self.title == title {
            return false;
        }

        self.title = title;
        true
    }

    fn suppress_ignored_backend_name(
        &mut self,
        workspace_state: &WorkspaceConversationState,
    ) -> bool {
        if !workspace_state
            .thread_registration(&self.thread_id)
            .is_some_and(|thread| {
                thread.ignores_backend_name_for_automatic_title(self.backend_name.as_deref())
            })
        {
            return false;
        }

        self.update_backend_name(workspace_state, None)
    }

    fn resolved_title(
        &self,
        workspace_state: &WorkspaceConversationState,
        backend_name: Option<&str>,
    ) -> String {
        resolved_thread_title(
            workspace_state,
            &self.thread_id,
            &self.execution_target,
            &self.preview,
            backend_name,
            self.created_at_millis,
            self.updated_at_millis,
        )
    }
}

impl MemberThreadInventoryBackendThread {
    pub(crate) fn new(runtime: RuntimeMode, summary: ThreadSummary) -> Self {
        Self { runtime, summary }
    }

    pub(crate) fn runtime(&self) -> &RuntimeMode {
        &self.runtime
    }

    pub(crate) fn summary(&self) -> &ThreadSummary {
        &self.summary
    }

    pub(crate) fn summary_mut(&mut self) -> &mut ThreadSummary {
        &mut self.summary
    }
}

#[allow(dead_code)]
pub(crate) fn build_member_thread_inventory_snapshot(
    workspace_id: BerylWorkspaceId,
    workspace_state: &WorkspaceConversationState,
    members: Vec<MemberThreadInventoryGroup>,
    backend_threads: Vec<ThreadSummary>,
    refreshed_at_millis: u64,
) -> MemberThreadInventorySnapshot {
    let default_runtime = members
        .first()
        .map(|member| member.runtime().clone())
        .unwrap_or(RuntimeMode::HostWindows);
    let backend_threads = backend_threads
        .into_iter()
        .map(|summary| MemberThreadInventoryBackendThread::new(default_runtime.clone(), summary))
        .collect();
    build_member_thread_inventory_snapshot_for_backend_threads(
        workspace_id,
        workspace_state,
        members,
        backend_threads,
        refreshed_at_millis,
    )
}

pub(crate) fn build_member_thread_inventory_snapshot_for_backend_threads(
    workspace_id: BerylWorkspaceId,
    workspace_state: &WorkspaceConversationState,
    members: Vec<MemberThreadInventoryGroup>,
    backend_threads: Vec<MemberThreadInventoryBackendThread>,
    refreshed_at_millis: u64,
) -> MemberThreadInventorySnapshot {
    build_member_thread_inventory_snapshot_for_backend_threads_with_coverage(
        workspace_id,
        workspace_state,
        members,
        backend_threads,
        refreshed_at_millis,
        MemberThreadInventoryCoverage::Complete,
    )
}

pub(crate) fn build_member_thread_inventory_snapshot_for_backend_threads_with_coverage(
    workspace_id: BerylWorkspaceId,
    workspace_state: &WorkspaceConversationState,
    members: Vec<MemberThreadInventoryGroup>,
    mut backend_threads: Vec<MemberThreadInventoryBackendThread>,
    refreshed_at_millis: u64,
    coverage: MemberThreadInventoryCoverage,
) -> MemberThreadInventorySnapshot {
    dedupe_backend_threads_by_runtime_thread_and_cwd(&mut backend_threads);
    let mut groups = members
        .into_iter()
        .map(|mut group| {
            group.threads = backend_threads
                .iter()
                .filter(|thread| {
                    thread.runtime() == group.runtime()
                        && group
                            .canonical_path()
                            .is_some_and(|path| thread.summary().cwd.as_path() == path)
                })
                .map(|thread| thread_from_summary(workspace_state, &group, thread.summary()))
                .collect();
            group.threads.sort_by(|left, right| {
                right
                    .updated_at_millis
                    .cmp(&left.updated_at_millis)
                    .then_with(|| right.created_at_millis.cmp(&left.created_at_millis))
                    .then_with(|| left.thread_id.as_str().cmp(right.thread_id.as_str()))
            });
            group
        })
        .collect::<Vec<_>>();

    groups.sort_by(|left, right| member_group_sort_key(left).cmp(&member_group_sort_key(right)));
    MemberThreadInventorySnapshot::new_with_coverage(
        workspace_id,
        refreshed_at_millis,
        coverage,
        groups,
    )
}

pub(crate) fn dedupe_backend_threads_by_runtime_thread_and_cwd(
    backend_threads: &mut Vec<MemberThreadInventoryBackendThread>,
) {
    let mut seen = HashSet::new();
    backend_threads.retain(|thread| {
        seen.insert((
            thread.runtime().clone(),
            thread.summary().id.clone(),
            thread.summary().cwd.clone(),
        ))
    });
}

#[allow(dead_code)]
pub(crate) fn prepare_backend_threads_for_member_thread_inventory<R>(
    backend_threads: &mut Vec<ThreadSummary>,
    members: &[MemberThreadInventoryGroup],
    read_metadata: R,
) -> Result<(), String>
where
    R: FnMut(&str) -> Result<ThreadSummary, ThreadForkParentMetadataReadError>,
{
    retain_backend_threads_for_inventory_members(backend_threads, members);
    truncate_backend_threads_for_member_thread_inventory(backend_threads);
    enrich_missing_thread_fork_parent_metadata(backend_threads, read_metadata)
}

#[allow(dead_code)]
pub(crate) fn retain_backend_threads_for_inventory_members(
    backend_threads: &mut Vec<ThreadSummary>,
    members: &[MemberThreadInventoryGroup],
) {
    backend_threads.retain(|thread| {
        members.iter().any(|member| {
            member
                .canonical_path()
                .is_some_and(|path| thread.cwd.as_path() == path)
        })
    });
}

#[allow(dead_code)]
pub(crate) fn truncate_backend_threads_for_member_thread_inventory(
    backend_threads: &mut Vec<ThreadSummary>,
) {
    if backend_threads.len() <= MEMBER_THREAD_INVENTORY_MAX_BACKEND_THREADS {
        return;
    }

    backend_threads.sort_by(|left, right| {
        right
            .updated_at
            .cmp(&left.updated_at)
            .then_with(|| right.created_at.cmp(&left.created_at))
            .then_with(|| left.id.cmp(&right.id))
    });
    backend_threads.truncate(MEMBER_THREAD_INVENTORY_MAX_BACKEND_THREADS);
}

pub(crate) fn retain_scoped_backend_threads_for_inventory_members(
    backend_threads: &mut Vec<MemberThreadInventoryBackendThread>,
    members: &[MemberThreadInventoryGroup],
) {
    backend_threads.retain(|thread| {
        members.iter().any(|member| {
            thread.runtime() == member.runtime()
                && member
                    .canonical_path()
                    .is_some_and(|path| thread.summary().cwd.as_path() == path)
        })
    });
}

pub(crate) fn truncate_scoped_backend_threads_for_member_thread_inventory(
    backend_threads: &mut Vec<MemberThreadInventoryBackendThread>,
) {
    if backend_threads.len() <= MEMBER_THREAD_INVENTORY_MAX_BACKEND_THREADS {
        return;
    }

    backend_threads.sort_by(|left, right| {
        right
            .summary()
            .updated_at
            .cmp(&left.summary().updated_at)
            .then_with(|| right.summary().created_at.cmp(&left.summary().created_at))
            .then_with(|| left.summary().id.cmp(&right.summary().id))
            .then_with(|| {
                left.runtime()
                    .display_name()
                    .cmp(&right.runtime().display_name())
            })
    });
    backend_threads.truncate(MEMBER_THREAD_INVENTORY_MAX_BACKEND_THREADS);
}

#[allow(dead_code)]
pub(crate) fn enrich_missing_thread_fork_parent_metadata<R>(
    backend_threads: &mut [ThreadSummary],
    mut read_metadata: R,
) -> Result<(), String>
where
    R: FnMut(&str) -> Result<ThreadSummary, ThreadForkParentMetadataReadError>,
{
    for thread in backend_threads
        .iter_mut()
        .filter(|thread| thread.forked_from_id.is_none())
    {
        let metadata = match read_metadata(&thread.id) {
            Ok(metadata) => metadata,
            Err(ThreadForkParentMetadataReadError::ThreadUnavailable(message)) => {
                warn!(
                    thread_id = %thread.id,
                    error = %message,
                    "could not enrich thread fork parent metadata"
                );
                continue;
            }
            Err(ThreadForkParentMetadataReadError::Fatal(message)) => return Err(message),
        };

        if metadata.id != thread.id {
            return Err(format!(
                "Beryl could not enrich thread lineage because metadata-only thread/read for {} returned {}.",
                thread.id, metadata.id
            ));
        }

        if metadata.forked_from_id.is_some() {
            thread.forked_from_id = metadata.forked_from_id;
        }
    }

    Ok(())
}

pub(crate) fn enrich_missing_thread_fork_parent_metadata_bounded<R>(
    backend_threads: &mut [ThreadSummary],
    max_reads: usize,
    mut read_metadata: R,
) -> (usize, Option<MemberThreadInventoryPartialCoverage>)
where
    R: FnMut(&str) -> Result<ThreadSummary, ThreadForkParentMetadataReadError>,
{
    let mut reads = 0;
    let mut partial = MemberThreadInventoryPartialCoverage::new();
    let mut incomplete = false;

    for thread in backend_threads
        .iter_mut()
        .filter(|thread| thread.forked_from_id.is_none())
    {
        if reads == max_reads {
            incomplete = true;
            partial = partial.with_lineage_incomplete(
                "Fork-parent metadata-read budget was exhausted before all retained rows were enriched.",
            );
            break;
        }
        reads += 1;

        let metadata = match read_metadata(&thread.id) {
            Ok(metadata) => metadata,
            Err(ThreadForkParentMetadataReadError::ThreadUnavailable(message)) => {
                incomplete = true;
                partial = partial.with_lineage_incomplete(format!(
                    "Fork-parent metadata for {} was unavailable: {message}",
                    thread.id
                ));
                continue;
            }
            Err(ThreadForkParentMetadataReadError::Fatal(message)) => {
                incomplete = true;
                partial = partial.with_lineage_incomplete(message);
                break;
            }
        };

        if metadata.id != thread.id {
            incomplete = true;
            partial = partial.with_lineage_incomplete(format!(
                "Metadata-only thread/read for {} returned mismatched thread id {}; the result was not trusted.",
                thread.id, metadata.id
            ));
            break;
        }
        if metadata.cwd != thread.cwd {
            incomplete = true;
            partial = partial.with_lineage_incomplete(format!(
                "Metadata-only thread/read for {} returned mismatched working directory {}; the result was not trusted.",
                thread.id,
                metadata.cwd.display()
            ));
            break;
        }

        if metadata.forked_from_id.is_some() {
            thread.forked_from_id = metadata.forked_from_id;
        }
    }

    (reads, incomplete.then_some(partial))
}

impl ThreadForkParentMetadataReadError {
    pub(crate) fn thread_unavailable(message: impl Into<String>) -> Self {
        Self::ThreadUnavailable(message.into())
    }

    pub(crate) fn fatal(message: impl Into<String>) -> Self {
        Self::Fatal(message.into())
    }
}

pub(crate) fn thread_fork_parent_metadata_read_error(
    thread_id: &str,
    error: ManagedBackendError,
) -> ThreadForkParentMetadataReadError {
    match &error {
        ManagedBackendError::RequestFailed { method, error }
            if method == "thread/read"
                && thread_read_error_is_thread_specific(error, thread_id) =>
        {
            ThreadForkParentMetadataReadError::thread_unavailable(error.to_string())
        }
        _ => ThreadForkParentMetadataReadError::fatal(format!(
            "Beryl could not refresh the workspace thread inventory: {error}"
        )),
    }
}

fn thread_read_error_is_thread_specific(error: &JsonRpcError, thread_id: &str) -> bool {
    const JSONRPC_METHOD_NOT_FOUND: i64 = -32601;
    const JSONRPC_INVALID_PARAMS: i64 = -32602;

    if matches!(
        error.code,
        JSONRPC_METHOD_NOT_FOUND | JSONRPC_INVALID_PARAMS
    ) {
        return false;
    }

    error.message.contains(thread_id)
        || error
            .data
            .as_ref()
            .is_some_and(|data| data.to_string().contains(thread_id))
}

pub(crate) fn empty_groups_for_workspace_state(
    workspace_state: &WorkspaceConversationState,
) -> Vec<MemberThreadInventoryGroup> {
    let Some(runtime) = workspace_state.selected_runtime().cloned() else {
        return Vec::new();
    };

    if !workspace_state.has_available_explicit_members() {
        return vec![MemberThreadInventoryGroup::new(
            MemberThreadInventoryMemberKey::ImplicitHome,
            MemberThreadInventoryMemberKind::ImplicitHome,
            "Implicit home",
            runtime,
            None,
            Vec::new(),
        )];
    }

    workspace_state
        .available_explicit_members()
        .map(|member| {
            MemberThreadInventoryGroup::new(
                MemberThreadInventoryMemberKey::Explicit(member.id().clone()),
                MemberThreadInventoryMemberKind::Explicit,
                member.canonical_path().display().to_string(),
                member.runtime_mode().clone(),
                Some(member.canonical_path().to_path_buf()),
                Vec::new(),
            )
        })
        .collect()
}

fn thread_from_summary(
    workspace_state: &WorkspaceConversationState,
    group: &MemberThreadInventoryGroup,
    summary: &ThreadSummary,
) -> MemberThreadInventoryThread {
    let thread_id = ConversationThreadId::new(summary.id.clone());
    let execution_target = WorkspaceId::from_parts(group.runtime().clone(), summary.cwd.clone());
    let registered_thread = workspace_state.thread_registration(&thread_id);
    let summary_backend_name = if registered_thread.is_some_and(|thread| {
        thread.ignores_backend_name_for_automatic_title(summary.name.as_deref())
    }) {
        None
    } else {
        normalized_thread_title(summary.name.as_deref())
    };
    let backend_name = summary_backend_name.or_else(|| {
        registered_thread
            .and_then(RegisteredConversationThread::backend_name)
            .map(str::to_string)
    });
    let title = resolved_thread_title(
        workspace_state,
        &thread_id,
        &execution_target,
        &summary.preview,
        backend_name.as_deref(),
        summary.created_at,
        summary.updated_at,
    );

    MemberThreadInventoryThread {
        thread_id,
        forked_from_id: summary
            .forked_from_id
            .as_ref()
            .map(|thread_id| ConversationThreadId::new(thread_id.clone())),
        title,
        execution_target,
        preview: summary.preview.clone(),
        backend_name,
        created_at_millis: summary.created_at,
        updated_at_millis: summary.updated_at,
    }
}

pub(crate) fn resolved_thread_title(
    workspace_state: &WorkspaceConversationState,
    thread_id: &ConversationThreadId,
    execution_target: &WorkspaceId,
    preview: &str,
    backend_name: Option<&str>,
    created_at_millis: i64,
    updated_at_millis: i64,
) -> String {
    workspace_state
        .thread_registration(thread_id)
        .and_then(|thread| {
            thread
                .title_with_backend_name(backend_name)
                .map(str::to_string)
        })
        .or_else(|| {
            RegisteredConversationThread::new(
                thread_id.clone(),
                execution_target.clone(),
                preview.to_string(),
                backend_name.map(str::to_string),
                created_at_millis,
                updated_at_millis,
            )
            .title()
            .map(str::to_string)
        })
        .unwrap_or_else(|| "Untitled thread".to_string())
}

fn normalized_thread_title(title: Option<&str>) -> Option<String> {
    title
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .map(str::to_string)
}

fn unsuppressed_inventory_backend_name(
    workspace_state: &WorkspaceConversationState,
    thread_id: &ConversationThreadId,
    backend_name: Option<String>,
) -> Option<String> {
    if workspace_state
        .thread_registration(thread_id)
        .is_some_and(|thread| {
            thread.ignores_backend_name_for_automatic_title(backend_name.as_deref())
        })
    {
        None
    } else {
        backend_name
    }
}

fn member_group_sort_key(group: &MemberThreadInventoryGroup) -> (u8, String) {
    match group.kind() {
        MemberThreadInventoryMemberKind::ImplicitHome => (0, group.label().to_string()),
        MemberThreadInventoryMemberKind::Explicit => (1, group.label().to_string()),
    }
}
