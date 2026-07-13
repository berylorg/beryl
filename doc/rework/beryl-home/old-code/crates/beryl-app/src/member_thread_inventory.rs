use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use beryl_model::{
    conversation::{
        ConversationThreadId, ConversationThreadMemberBinding, ConversationThreadTitleSource,
        RegisteredConversationThread, SyndicConversationId, SyndicConversationViewId,
        WorkspaceConversationState,
    },
    workspace::{BerylWorkspaceId, RuntimeMode, WorkspaceId, WorkspaceMemberId},
};
use syndic_storage::{
    ConversationViewSummary, MAX_CONVERSATION_SUMMARY_READ_LIMIT, StoreOpenOptions, SyndicStore,
    ThreadViewId,
};

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
    groups: Vec<MemberThreadInventoryGroup>,
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
    syndic_conversation_id: SyndicConversationId,
    syndic_view_id: SyndicConversationViewId,
    forked_from_id: Option<ConversationThreadId>,
    title: String,
    execution_target: WorkspaceId,
    preview: String,
    created_at_millis: i64,
    updated_at_millis: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MemberThreadInventoryEvent {
    MemberSetChanged,
    BackendTargetOpening,
    BackendTargetAvailable,
    InventoryContentsChanged,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MemberThreadInventoryState {
    snapshot: MemberThreadInventorySnapshot,
    refreshing: bool,
    needs_refresh: bool,
    last_error: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct MemberThreadInventoryRetainedCounts {
    pub(crate) groups: usize,
    pub(crate) threads: usize,
    pub(crate) payload_bytes: usize,
}

impl MemberThreadInventorySnapshot {
    pub(crate) fn empty_for_workspace(
        workspace_id: BerylWorkspaceId,
        workspace_state: &WorkspaceConversationState,
    ) -> Self {
        Self {
            workspace_id,
            refreshed_at_millis: 0,
            groups: empty_groups_for_workspace_state(workspace_state),
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
                        .map(MemberThreadInventoryThread::retained_payload_bytes)
                        .sum::<usize>()
            })
            .sum();
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
            refreshing: false,
            needs_refresh: true,
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
        self.needs_refresh
    }

    pub(crate) fn prepare_for_backend_reopen(&mut self) {}

    pub(crate) fn begin_refresh(&mut self) -> bool {
        if self.refreshing || !self.needs_refresh {
            return false;
        }
        self.refreshing = true;
        true
    }

    pub(crate) fn apply_refresh_success(
        &mut self,
        snapshot: MemberThreadInventorySnapshot,
    ) -> bool {
        let changed = self.snapshot != snapshot
            || self.refreshing
            || self.needs_refresh
            || self.last_error.is_some();
        self.snapshot = snapshot;
        self.refreshing = false;
        self.needs_refresh = false;
        self.last_error = None;
        changed
    }

    pub(crate) fn apply_refresh_failure(&mut self, error: impl Into<String>) -> bool {
        let error = error.into();
        let changed =
            self.refreshing || !self.needs_refresh || self.last_error.as_ref() != Some(&error);
        self.refreshing = false;
        self.needs_refresh = true;
        self.last_error = Some(error);
        changed
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
                self.snapshot = MemberThreadInventorySnapshot::empty_for_workspace(
                    workspace_id,
                    workspace_state,
                );
                self.needs_refresh = true;
            }
            MemberThreadInventoryEvent::BackendTargetOpening
            | MemberThreadInventoryEvent::BackendTargetAvailable
            | MemberThreadInventoryEvent::InventoryContentsChanged => {
                self.needs_refresh = true;
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
    pub(crate) fn new(
        thread_id: ConversationThreadId,
        syndic_conversation_id: SyndicConversationId,
        syndic_view_id: SyndicConversationViewId,
        forked_from_id: Option<ConversationThreadId>,
        title: impl Into<String>,
        execution_target: WorkspaceId,
        preview: impl Into<String>,
        created_at_millis: i64,
        updated_at_millis: i64,
    ) -> Self {
        Self {
            thread_id,
            syndic_conversation_id,
            syndic_view_id,
            forked_from_id,
            title: title.into(),
            execution_target,
            preview: preview.into(),
            created_at_millis,
            updated_at_millis,
        }
    }

    pub(crate) fn thread_id(&self) -> &ConversationThreadId {
        &self.thread_id
    }

    pub(crate) fn syndic_conversation_id(&self) -> &SyndicConversationId {
        &self.syndic_conversation_id
    }

    pub(crate) fn syndic_view_id(&self) -> &SyndicConversationViewId {
        &self.syndic_view_id
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
        let mut thread = RegisteredConversationThread::new(
            self.thread_id.clone(),
            self.execution_target.clone(),
            self.preview.clone(),
            self.created_at_millis,
            self.updated_at_millis,
        );
        if let Some(parent_thread_id) = self.forked_from_id.as_ref() {
            thread = thread.with_branch_parent_thread_id(parent_thread_id.clone());
        }
        thread = thread.with_syndic_view_registration(
            self.syndic_conversation_id.clone(),
            self.syndic_view_id.clone(),
        );
        thread
    }

    fn retained_payload_bytes(&self) -> usize {
        self.thread_id.as_str().len()
            + self.syndic_conversation_id.as_str().len()
            + self.syndic_view_id.as_str().len()
            + self
                .forked_from_id
                .as_ref()
                .map_or(0, |id| id.as_str().len())
            + self.title.len()
            + self.execution_target.display_label().len()
            + self.preview.len()
    }
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

pub(crate) fn build_workspace_syndic_catalog_snapshot(
    storage_dir: &Path,
    workspace_id: BerylWorkspaceId,
    workspace_state: &WorkspaceConversationState,
) -> Result<MemberThreadInventorySnapshot, String> {
    let registrations = workspace_state.catalog_threads().collect::<Vec<_>>();
    let mut groups = empty_groups_for_workspace_state(workspace_state);
    if registrations.is_empty() {
        return Ok(MemberThreadInventorySnapshot {
            workspace_id,
            refreshed_at_millis: current_unix_millis(),
            groups,
        });
    }

    let view_ids = registrations
        .iter()
        .filter_map(|thread| thread.syndic_view_id())
        .map(|view_id| ThreadViewId::from(view_id.as_str().to_string()))
        .collect::<Vec<_>>();
    let store = SyndicStore::open(storage_dir, StoreOpenOptions::default())
        .map_err(|error| format!("Syndic catalog storage unavailable: {error}"))?;
    let mut summaries = Vec::new();
    for chunk in view_ids.chunks(MAX_CONVERSATION_SUMMARY_READ_LIMIT) {
        summaries.extend(
            store
                .conversation_view_summaries(chunk, chunk.len())
                .map_err(|error| format!("Syndic catalog summaries unavailable: {error}"))?,
        );
    }
    summaries.sort_by(|left, right| {
        right
            .updated_at_ms
            .cmp(&left.updated_at_ms)
            .then_with(|| right.created_at_ms.cmp(&left.created_at_ms))
            .then_with(|| left.view_id.to_string().cmp(&right.view_id.to_string()))
    });
    let summaries_by_view = summaries
        .into_iter()
        .take(MAX_CONVERSATION_SUMMARY_READ_LIMIT)
        .map(|summary| (summary.view_id.to_string(), summary))
        .collect::<HashMap<_, _>>();
    let thread_id_by_view = registrations
        .iter()
        .filter_map(|thread| {
            Some((
                thread.syndic_view_id()?.as_str().to_string(),
                thread.thread_id().clone(),
            ))
        })
        .collect::<HashMap<_, _>>();

    for thread in registrations {
        let Some(summary) = thread
            .syndic_view_id()
            .and_then(|view_id| summaries_by_view.get(view_id.as_str()))
        else {
            continue;
        };
        if !summary_matches_registration(thread, summary) {
            continue;
        }
        let Some(group_key) = group_key_for_registered_thread(workspace_state, thread) else {
            continue;
        };
        let Some(group) = groups.iter_mut().find(|group| group.key() == &group_key) else {
            continue;
        };
        let Some(conversation_id) = thread.syndic_conversation_id().cloned() else {
            continue;
        };
        let Some(view_id) = thread.syndic_view_id().cloned() else {
            continue;
        };
        let parent_thread_id = summary
            .branch
            .as_ref()
            .and_then(|branch| thread_id_by_view.get(&branch.parent_view_id.to_string()))
            .cloned();
        group.threads.push(MemberThreadInventoryThread::new(
            thread.thread_id().clone(),
            conversation_id,
            view_id,
            parent_thread_id,
            catalog_title(thread, summary),
            thread.execution_target().clone(),
            "",
            millis_u64_to_i64(summary.created_at_ms),
            millis_u64_to_i64(summary.updated_at_ms),
        ));
    }

    for group in &mut groups {
        group.threads.sort_by(|left, right| {
            right
                .updated_at_millis()
                .cmp(&left.updated_at_millis())
                .then_with(|| right.created_at_millis().cmp(&left.created_at_millis()))
                .then_with(|| left.thread_id().as_str().cmp(right.thread_id().as_str()))
        });
    }

    Ok(MemberThreadInventorySnapshot {
        workspace_id,
        refreshed_at_millis: current_unix_millis(),
        groups,
    })
}

pub(crate) fn resolved_thread_title(
    workspace_state: &WorkspaceConversationState,
    thread_id: &ConversationThreadId,
) -> String {
    workspace_state
        .thread_registration(thread_id)
        .and_then(workspace_owned_title)
        .map(str::to_string)
        .unwrap_or_else(|| "Untitled thread".to_string())
}

fn summary_matches_registration(
    thread: &RegisteredConversationThread,
    summary: &ConversationViewSummary,
) -> bool {
    thread
        .syndic_conversation_id()
        .is_some_and(|conversation_id| {
            conversation_id.as_str() == summary.conversation_id.to_string()
        })
        && thread
            .syndic_view_id()
            .is_some_and(|view_id| view_id.as_str() == summary.view_id.to_string())
}

fn group_key_for_registered_thread(
    workspace_state: &WorkspaceConversationState,
    thread: &RegisteredConversationThread,
) -> Option<MemberThreadInventoryMemberKey> {
    match thread.member_binding()? {
        ConversationThreadMemberBinding::Explicit {
            member_id,
            execution_target,
        } => workspace_state
            .available_explicit_members()
            .any(|member| {
                member.id() == member_id
                    && member.runtime_mode() == execution_target.runtime_mode()
                    && member.canonical_path() == execution_target.canonical_path()
                    && thread.execution_target() == execution_target
            })
            .then(|| MemberThreadInventoryMemberKey::Explicit(member_id.clone())),
        ConversationThreadMemberBinding::ImplicitHome { execution_target } => (!workspace_state
            .has_available_explicit_members()
            && workspace_state.selected_runtime() == Some(execution_target.runtime_mode())
            && thread.execution_target() == execution_target)
            .then_some(MemberThreadInventoryMemberKey::ImplicitHome),
    }
}

fn catalog_title(
    thread: &RegisteredConversationThread,
    summary: &ConversationViewSummary,
) -> String {
    workspace_owned_title(thread)
        .map(str::to_string)
        .or_else(|| {
            summary
                .title_candidates
                .iter()
                .map(|candidate| candidate.title.trim())
                .find(|title| !title.is_empty())
                .map(str::to_string)
        })
        .unwrap_or_else(|| "Untitled thread".to_string())
}

fn workspace_owned_title(thread: &RegisteredConversationThread) -> Option<&str> {
    let title = thread.gui_title()?;
    match title.source() {
        ConversationThreadTitleSource::Manual
        | ConversationThreadTitleSource::FirstCompletedTurn => Some(title.text()),
    }
}

fn millis_u64_to_i64(value: u64) -> i64 {
    value.min(i64::MAX as u64) as i64
}

fn current_unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}
