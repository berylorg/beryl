use std::{
    collections::{HashMap, HashSet},
    sync::mpsc::{self, Receiver},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use beryl_backend::{
    ManagedBackendClientConnector, ManagedBackendError, ManagedBackendSession, ThreadListBudget,
    ThreadListCollection, ThreadListCollectionError, ThreadListCollectionStatus, ThreadListOptions,
    ThreadListTruncationReason, ThreadSummary,
};
use beryl_model::{
    conversation::{RegisteredConversationThread, WorkspaceConversationState},
    workspace::{BerylWorkspaceId, WorkspaceId},
};
use tracing::warn;

use crate::member_thread_inventory::MemberThreadInventoryEvent;
use crate::member_thread_inventory::{
    MEMBER_THREAD_INVENTORY_MAX_BACKEND_THREADS, MemberThreadInventoryBackendThread,
    MemberThreadInventoryCoverage, MemberThreadInventoryGroup, MemberThreadInventoryMemberKey,
    MemberThreadInventoryMemberKind, MemberThreadInventoryPartialCoverage,
    MemberThreadInventoryRefreshToken, MemberThreadInventorySnapshot,
    build_member_thread_inventory_snapshot_for_backend_threads_with_coverage,
    dedupe_backend_threads_by_runtime_thread_and_cwd,
    enrich_missing_thread_fork_parent_metadata_bounded,
    retain_scoped_backend_threads_for_inventory_members, thread_fork_parent_metadata_read_error,
    truncate_scoped_backend_threads_for_member_thread_inventory,
};

use super::{ShellView, SurfaceNotice, workspace_members};

const MEMBER_THREAD_INVENTORY_MAX_LIST_PAGES: usize = 32;
const MEMBER_THREAD_INVENTORY_MAX_METADATA_READS: usize = 256;
const MEMBER_THREAD_INVENTORY_MAX_NOTICE_BYTES: usize = 512;

#[derive(Clone, Copy)]
struct MemberThreadInventoryJobLimits {
    elapsed_limit: Duration,
    max_list_pages: usize,
    max_list_results: usize,
    max_metadata_reads: usize,
}

struct MemberThreadInventoryJobBudget {
    limits: MemberThreadInventoryJobLimits,
    list_pages_remaining: usize,
    list_results_remaining: usize,
    metadata_reads_remaining: usize,
}

#[derive(Clone)]
struct MemberThreadInventoryTarget {
    key: usize,
    execution_target: WorkspaceId,
}

enum MemberThreadInventoryListAttempt {
    Collected(ThreadListCollection),
    Failed {
        data: Vec<ThreadSummary>,
        pages_collected: usize,
        message: String,
    },
}

trait MemberThreadInventoryOperations {
    fn targets(&self) -> Vec<MemberThreadInventoryTarget>;

    fn connect(
        &mut self,
        target: &MemberThreadInventoryTarget,
        timeout: Duration,
    ) -> Result<(), String>;

    fn list_threads(
        &mut self,
        target: &MemberThreadInventoryTarget,
        options: ThreadListOptions,
        list_budget: ThreadListBudget,
    ) -> MemberThreadInventoryListAttempt;

    fn read_thread_metadata(
        &mut self,
        target: &MemberThreadInventoryTarget,
        thread_id: &str,
        timeout: Duration,
    ) -> Result<ThreadSummary, crate::member_thread_inventory::ThreadForkParentMetadataReadError>;
}

trait MemberThreadInventoryElapsed {
    fn elapsed(&self) -> Duration;
}

struct MonotonicMemberThreadInventoryElapsed {
    started: Instant,
}

trait ManagedBackendInventoryConnector {
    type Session: ManagedBackendInventorySession;

    fn connect_inventory_client(
        &self,
        timeout: Duration,
    ) -> Result<Self::Session, ManagedBackendError>;
}

trait ManagedBackendInventorySession {
    fn list_inventory_threads_bounded(
        &mut self,
        options: ThreadListOptions,
        budget: ThreadListBudget,
    ) -> Result<ThreadListCollection, ThreadListCollectionError>;

    fn read_inventory_thread_metadata(
        &mut self,
        thread_id: &str,
        timeout: Duration,
    ) -> Result<ThreadSummary, ManagedBackendError>;
}

struct ManagedBackendInventoryOperations<C>
where
    C: ManagedBackendInventoryConnector,
{
    connectors: Vec<(WorkspaceId, C)>,
    sessions: HashMap<usize, C::Session>,
}

pub(super) enum MemberThreadInventoryUpdate {
    Finished {
        workspace_id: BerylWorkspaceId,
        token: MemberThreadInventoryRefreshToken,
        result: MemberThreadInventoryResult,
    },
}

pub(super) enum MemberThreadInventoryResult {
    Refreshed {
        snapshot: MemberThreadInventorySnapshot,
        registered_threads: Vec<RegisteredConversationThread>,
    },
    Failed {
        message: String,
    },
}

impl MemberThreadInventoryJobBudget {
    fn new(limits: MemberThreadInventoryJobLimits) -> Self {
        Self {
            limits,
            list_pages_remaining: limits.max_list_pages,
            list_results_remaining: limits.max_list_results,
            metadata_reads_remaining: limits.max_metadata_reads,
        }
    }

    fn remaining_elapsed(&self, elapsed: Duration) -> Option<Duration> {
        self.limits
            .elapsed_limit
            .checked_sub(elapsed)
            .filter(|remaining| !remaining.is_zero())
    }

    fn remaining_list_budget(&self, elapsed: Duration) -> Option<ThreadListBudget> {
        ThreadListBudget::new(
            self.remaining_elapsed(elapsed)?,
            self.list_pages_remaining,
            self.list_results_remaining,
        )
        .ok()
    }

    fn has_list_capacity(&self) -> bool {
        self.list_pages_remaining > 0 && self.list_results_remaining > 0
    }

    fn consume_list(&mut self, pages: usize, results: usize) {
        self.list_pages_remaining = self.list_pages_remaining.saturating_sub(pages);
        self.list_results_remaining = self.list_results_remaining.saturating_sub(results);
    }

    fn consume_metadata_reads(&mut self, reads: usize) {
        self.metadata_reads_remaining = self.metadata_reads_remaining.saturating_sub(reads);
    }
}

impl MemberThreadInventoryJobLimits {
    fn production(elapsed_limit: Duration) -> Self {
        Self {
            elapsed_limit,
            max_list_pages: MEMBER_THREAD_INVENTORY_MAX_LIST_PAGES,
            max_list_results: MEMBER_THREAD_INVENTORY_MAX_BACKEND_THREADS,
            max_metadata_reads: MEMBER_THREAD_INVENTORY_MAX_METADATA_READS,
        }
    }
}

impl MemberThreadInventoryElapsed for MonotonicMemberThreadInventoryElapsed {
    fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }
}

impl ManagedBackendInventoryConnector for ManagedBackendClientConnector {
    type Session = ManagedBackendSession;

    fn connect_inventory_client(
        &self,
        timeout: Duration,
    ) -> Result<Self::Session, ManagedBackendError> {
        self.connect_client(timeout)
    }
}

impl ManagedBackendInventorySession for ManagedBackendSession {
    fn list_inventory_threads_bounded(
        &mut self,
        options: ThreadListOptions,
        budget: ThreadListBudget,
    ) -> Result<ThreadListCollection, ThreadListCollectionError> {
        self.list_threads_bounded(options, budget)
    }

    fn read_inventory_thread_metadata(
        &mut self,
        thread_id: &str,
        timeout: Duration,
    ) -> Result<ThreadSummary, ManagedBackendError> {
        self.read_thread_metadata(thread_id, timeout)
    }
}

impl<C> ManagedBackendInventoryOperations<C>
where
    C: ManagedBackendInventoryConnector,
{
    fn new(connectors: Vec<(WorkspaceId, C)>) -> Self {
        Self {
            connectors,
            sessions: HashMap::new(),
        }
    }
}

impl<C> MemberThreadInventoryOperations for ManagedBackendInventoryOperations<C>
where
    C: ManagedBackendInventoryConnector,
{
    fn targets(&self) -> Vec<MemberThreadInventoryTarget> {
        self.connectors
            .iter()
            .enumerate()
            .map(|(key, (execution_target, _))| MemberThreadInventoryTarget {
                key,
                execution_target: execution_target.clone(),
            })
            .collect()
    }

    fn connect(
        &mut self,
        target: &MemberThreadInventoryTarget,
        timeout: Duration,
    ) -> Result<(), String> {
        let Some((_, connector)) = self.connectors.get(target.key) else {
            return Err(
                "Beryl lost a managed backend inventory target before dispatch.".to_string(),
            );
        };
        let session = connector
            .connect_inventory_client(timeout)
            .map_err(|error| format!("Beryl could not connect to the managed backend: {error}"))?;
        self.sessions.insert(target.key, session);
        Ok(())
    }

    fn list_threads(
        &mut self,
        target: &MemberThreadInventoryTarget,
        options: ThreadListOptions,
        list_budget: ThreadListBudget,
    ) -> MemberThreadInventoryListAttempt {
        let session = self
            .sessions
            .get_mut(&target.key)
            .expect("inventory runner connects a target before listing it");
        let attempt = match session.list_inventory_threads_bounded(options, list_budget) {
            Ok(collection) => MemberThreadInventoryListAttempt::Collected(collection),
            Err(error) => MemberThreadInventoryListAttempt::Failed {
                data: error.data,
                pages_collected: error.pages_collected,
                message: format!(
                    "Beryl could not refresh the workspace thread inventory: {}",
                    error.source
                ),
            },
        };
        attempt
    }

    fn read_thread_metadata(
        &mut self,
        target: &MemberThreadInventoryTarget,
        thread_id: &str,
        timeout: Duration,
    ) -> Result<ThreadSummary, crate::member_thread_inventory::ThreadForkParentMetadataReadError>
    {
        let Some(session) = self.sessions.get_mut(&target.key) else {
            return Err(
                crate::member_thread_inventory::ThreadForkParentMetadataReadError::fatal(
                    "Beryl lost a managed backend inventory session before lineage enrichment.",
                ),
            );
        };
        session
            .read_inventory_thread_metadata(thread_id, timeout)
            .map_err(|error| thread_fork_parent_metadata_read_error(thread_id, error))
    }
}

pub(super) fn spawn_member_thread_inventory_worker(
    connectors: Vec<(WorkspaceId, ManagedBackendClientConnector)>,
    workspace_id: BerylWorkspaceId,
    token: MemberThreadInventoryRefreshToken,
    workspace_state: WorkspaceConversationState,
    timeout: Duration,
) -> Receiver<MemberThreadInventoryUpdate> {
    spawn_member_thread_inventory_worker_with(
        connectors,
        workspace_id,
        token,
        workspace_state,
        MemberThreadInventoryJobLimits::production(timeout),
        || MonotonicMemberThreadInventoryElapsed {
            started: Instant::now(),
        },
    )
}

fn spawn_member_thread_inventory_worker_with<C, E, F>(
    connectors: Vec<(WorkspaceId, C)>,
    workspace_id: BerylWorkspaceId,
    token: MemberThreadInventoryRefreshToken,
    workspace_state: WorkspaceConversationState,
    limits: MemberThreadInventoryJobLimits,
    make_elapsed: F,
) -> Receiver<MemberThreadInventoryUpdate>
where
    C: ManagedBackendInventoryConnector + Send + 'static,
    C::Session: Send + 'static,
    E: MemberThreadInventoryElapsed + Send + 'static,
    F: FnOnce() -> E + Send + 'static,
{
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let mut operations = ManagedBackendInventoryOperations::new(connectors);
        let elapsed = make_elapsed();
        let result = run_member_thread_inventory(
            &mut operations,
            &elapsed,
            &workspace_id,
            workspace_state,
            limits,
        );
        let _ = sender.send(MemberThreadInventoryUpdate::Finished {
            workspace_id,
            token,
            result,
        });
    });
    receiver
}

fn run_member_thread_inventory<O, E>(
    operations: &mut O,
    elapsed: &E,
    workspace_id: &BerylWorkspaceId,
    workspace_state: WorkspaceConversationState,
    limits: MemberThreadInventoryJobLimits,
) -> MemberThreadInventoryResult
where
    O: MemberThreadInventoryOperations,
    E: MemberThreadInventoryElapsed,
{
    let members = match resolved_inventory_members(&workspace_state) {
        Ok(members) => members,
        Err(message) => {
            return MemberThreadInventoryResult::Failed { message };
        }
    };
    let mut targets = operations.targets();
    targets.sort_by(|left, right| {
        left.execution_target
            .runtime_mode()
            .display_name()
            .cmp(&right.execution_target.runtime_mode().display_name())
            .then_with(|| {
                left.execution_target
                    .canonical_path()
                    .cmp(right.execution_target.canonical_path())
            })
    });

    let mut budget = MemberThreadInventoryJobBudget::new(limits);
    let mut backend_threads = Vec::new();
    let mut sessions = Vec::new();
    let mut trustworthy_pages = 0usize;
    let mut partial = MemberThreadInventoryPartialCoverage::new();
    let mut is_partial = false;
    let mut listed_runtimes = HashSet::new();
    for target in targets {
        let runtime = target.execution_target.runtime_mode().clone();
        if !listed_runtimes.insert(runtime.clone()) {
            continue;
        }
        let runtime_members = members
            .iter()
            .filter(|member| member.runtime() == &runtime)
            .cloned()
            .collect::<Vec<_>>();
        if runtime_members.is_empty() {
            continue;
        }
        let cwd_filters = runtime_members
            .iter()
            .filter_map(|member| member.canonical_path().map(std::path::Path::to_path_buf))
            .collect::<Vec<_>>();
        if cwd_filters.is_empty() {
            continue;
        }

        if !budget.has_list_capacity() {
            if trustworthy_pages == 0 {
                return MemberThreadInventoryResult::Failed {
                    message: "Beryl could not accept a trustworthy thread-list page before the inventory budget expired.".to_string(),
                };
            }
            partial = partial.with_row_coverage_truncated(
                "The inventory page or result budget was exhausted before every runtime target was listed.",
            );
            is_partial = true;
            break;
        }
        let Some(connect_timeout) = budget.remaining_elapsed(elapsed.elapsed()) else {
            if trustworthy_pages == 0 {
                return MemberThreadInventoryResult::Failed {
                    message: "Beryl could not refresh the workspace thread inventory before its elapsed-time budget expired.".to_string(),
                };
            }
            partial = partial.with_row_coverage_truncated(
                "The inventory elapsed-time budget expired before every runtime target was listed.",
            );
            is_partial = true;
            break;
        };
        if let Err(message) = operations.connect(&target, connect_timeout) {
            if trustworthy_pages == 0 {
                return MemberThreadInventoryResult::Failed { message };
            }
            partial = partial.with_row_coverage_truncated(message);
            is_partial = true;
            break;
        }
        let Some(list_budget) = budget.remaining_list_budget(elapsed.elapsed()) else {
            if trustworthy_pages == 0 {
                return MemberThreadInventoryResult::Failed {
                    message: "Beryl could not accept a trustworthy thread-list page before the inventory budget expired.".to_string(),
                };
            }
            partial = partial.with_row_coverage_truncated(
                "The inventory page, result, or elapsed-time budget was exhausted before every runtime target was listed.",
            );
            is_partial = true;
            break;
        };
        let list_options = ThreadListOptions::page(100)
            .with_cwds(cwd_filters)
            .updated_descending();
        let collection = match operations.list_threads(&target, list_options, list_budget) {
            MemberThreadInventoryListAttempt::Collected(collection) => collection,
            MemberThreadInventoryListAttempt::Failed {
                data,
                pages_collected,
                message,
            } => {
                let pages = pages_collected;
                let results = data.len();
                budget.consume_list(pages, results);
                if !thread_rows_match_members(&data, &runtime_members) {
                    let scope_message = format!(
                        "The {runtime_name} backend returned thread rows outside the requested workspace-member paths; that target result was not trusted.",
                        runtime_name = runtime.display_name(),
                    );
                    if trustworthy_pages == 0 {
                        return MemberThreadInventoryResult::Failed {
                            message: scope_message,
                        };
                    }
                    partial = partial.with_row_coverage_truncated(scope_message);
                    partial = partial.with_row_coverage_truncated(message);
                    is_partial = true;
                    break;
                }
                trustworthy_pages += pages;
                backend_threads.extend(data.into_iter().map(|summary| {
                    MemberThreadInventoryBackendThread::new(runtime.clone(), summary)
                }));
                if trustworthy_pages == 0 {
                    return MemberThreadInventoryResult::Failed { message };
                }
                partial = partial.with_row_coverage_truncated(message);
                is_partial = true;
                sessions.push((runtime, target));
                break;
            }
        };
        let status = collection.status;
        let pages = collection.pages_collected;
        let results = collection.data.len();
        budget.consume_list(pages, results);
        if !thread_rows_match_members(&collection.data, &runtime_members) {
            let message = format!(
                "The {runtime_name} backend returned thread rows outside the requested workspace-member paths; that target result was not trusted.",
                runtime_name = runtime.display_name(),
            );
            if trustworthy_pages == 0 {
                return MemberThreadInventoryResult::Failed { message };
            }
            partial = partial.with_row_coverage_truncated(message);
            is_partial = true;
            break;
        }
        trustworthy_pages += pages;
        backend_threads.extend(
            collection
                .data
                .into_iter()
                .map(|summary| MemberThreadInventoryBackendThread::new(runtime.clone(), summary)),
        );
        sessions.push((runtime, target));

        if let ThreadListCollectionStatus::Truncated(reason) = status {
            partial = partial.with_row_coverage_truncated(thread_list_truncation_message(reason));
            is_partial = true;
            break;
        }
    }

    if trustworthy_pages == 0 {
        return MemberThreadInventoryResult::Failed {
            message: "Beryl could not accept a trustworthy thread-list page for this workspace."
                .to_string(),
        };
    }

    retain_scoped_backend_threads_for_inventory_members(&mut backend_threads, &members);
    dedupe_backend_threads_by_runtime_thread_and_cwd(&mut backend_threads);
    truncate_scoped_backend_threads_for_member_thread_inventory(&mut backend_threads);

    for (runtime, target) in sessions {
        let positions = backend_threads
            .iter()
            .enumerate()
            .filter(|(_, thread)| thread.runtime() == &runtime)
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        let mut summaries = positions
            .iter()
            .map(|index| backend_threads[*index].summary().clone())
            .collect::<Vec<_>>();
        let max_reads = budget.metadata_reads_remaining;
        let (reads, lineage_partial) = enrich_missing_thread_fork_parent_metadata_bounded(
            &mut summaries,
            max_reads,
            |thread_id| {
                let Some(read_timeout) = budget.remaining_elapsed(elapsed.elapsed()) else {
                    return Err(
                        crate::member_thread_inventory::ThreadForkParentMetadataReadError::fatal(
                            "The inventory elapsed-time budget expired during fork-parent enrichment.",
                        ),
                    );
                };
                operations.read_thread_metadata(&target, thread_id, read_timeout)
            },
        );
        budget.consume_metadata_reads(reads);
        for (index, summary) in positions.into_iter().zip(summaries) {
            *backend_threads[index].summary_mut() = summary;
        }
        if let Some(lineage_partial) = lineage_partial {
            partial.merge(lineage_partial);
            is_partial = true;
            if budget.remaining_elapsed(elapsed.elapsed()).is_none()
                || budget.metadata_reads_remaining == 0
            {
                break;
            }
        }
    }

    let coverage = if is_partial {
        MemberThreadInventoryCoverage::Partial(partial)
    } else {
        MemberThreadInventoryCoverage::Complete
    };
    let snapshot = build_member_thread_inventory_snapshot_for_backend_threads_with_coverage(
        workspace_id.clone(),
        &workspace_state,
        members,
        backend_threads,
        current_unix_millis(),
        coverage,
    );
    let registered_threads = snapshot
        .groups()
        .iter()
        .flat_map(|group| group.threads().iter())
        .map(|thread| thread.to_registered_thread())
        .collect();

    MemberThreadInventoryResult::Refreshed {
        snapshot,
        registered_threads,
    }
}

fn resolved_inventory_members(
    workspace_state: &WorkspaceConversationState,
) -> Result<Vec<MemberThreadInventoryGroup>, String> {
    let Some(runtime) = workspace_state.selected_runtime().cloned() else {
        return Ok(Vec::new());
    };

    if !workspace_state.has_available_explicit_members() {
        let canonical_path =
            workspace_members::resolve_runtime_home_directory(&runtime).map_err(|error| {
                format!("Beryl could not resolve the implicit home member for inventory: {error}")
            })?;
        return Ok(vec![MemberThreadInventoryGroup::new(
            MemberThreadInventoryMemberKey::ImplicitHome,
            MemberThreadInventoryMemberKind::ImplicitHome,
            "Implicit home",
            runtime.clone(),
            Some(canonical_path),
            Vec::new(),
        )]);
    }

    Ok(workspace_state
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
        .collect())
}

fn current_unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn thread_list_truncation_message(reason: ThreadListTruncationReason) -> &'static str {
    match reason {
        ThreadListTruncationReason::ElapsedTime => {
            "The inventory elapsed-time budget expired before all thread rows were listed."
        }
        ThreadListTruncationReason::PageLimit => {
            "The inventory page budget was exhausted before all thread rows were listed."
        }
        ThreadListTruncationReason::ResultLimit => {
            "The inventory result budget was exhausted before all thread rows were listed."
        }
    }
}

fn thread_rows_match_members(
    rows: &[beryl_backend::ThreadSummary],
    members: &[MemberThreadInventoryGroup],
) -> bool {
    rows.iter().all(|thread| {
        members.iter().any(|member| {
            member
                .canonical_path()
                .is_some_and(|path| path == thread.cwd.as_path())
        })
    })
}

fn bounded_inventory_notice(mut message: String) -> String {
    if message.len() <= MEMBER_THREAD_INVENTORY_MAX_NOTICE_BYTES {
        return message;
    }
    let mut boundary = MEMBER_THREAD_INVENTORY_MAX_NOTICE_BYTES;
    while !message.is_char_boundary(boundary) {
        boundary -= 1;
    }
    message.truncate(boundary);
    message
}

fn member_thread_inventory_result_is_current(
    active_workspace_id: Option<&BerylWorkspaceId>,
    active_token: Option<MemberThreadInventoryRefreshToken>,
    result_workspace_id: &BerylWorkspaceId,
    result_token: MemberThreadInventoryRefreshToken,
) -> bool {
    active_workspace_id == Some(result_workspace_id) && active_token == Some(result_token)
}

impl ShellView {
    pub(super) fn poll_member_thread_inventory_updates(&mut self) -> bool {
        let Some(receiver) = self.member_thread_inventory_receiver.as_ref() else {
            return false;
        };

        match receiver.try_recv() {
            Ok(MemberThreadInventoryUpdate::Finished {
                workspace_id,
                token,
                result,
            }) => {
                self.member_thread_inventory_receiver = None;
                self.finish_member_thread_inventory_refresh(&workspace_id, token, result);
                true
            }
            Err(mpsc::TryRecvError::Empty) => false,
            Err(mpsc::TryRecvError::Disconnected) => {
                self.member_thread_inventory_receiver = None;
                if let Some(surface) = self.conversation_surface_mut() {
                    let token = surface.member_thread_inventory().refresh_token();
                    surface
                        .member_thread_inventory_mut()
                        .fail_refresh_for_token(
                            token,
                            "Beryl lost the background thread inventory refresh task.",
                        );
                }
                true
            }
        }
    }

    pub(super) fn begin_member_thread_inventory_refresh_if_needed(&mut self) -> bool {
        if self.member_thread_inventory_receiver.is_some()
            || self.workspace_receiver.is_some()
            || self.thread_activation_receiver.is_some()
            || self.transcript_branch_receiver.is_some()
            || self.thread_history_page_receiver.is_some()
            || self.turn_receiver.is_some()
            || !self.turn_steering_receivers.is_empty()
            || self.workspace_picker_action_in_flight()
            || self.workspace_title_receiver.is_some()
        {
            return false;
        }
        if self.conversation_surface().is_some_and(|surface| {
            surface.graph_overlay().visible() || surface.pending_thread_activation_label().is_some()
        }) {
            return false;
        }

        let Some((workspace_id, workspace_state)) = self.loaded_workspace().and_then(|loaded| {
            loaded.selected_runtime().map(|_| {
                (
                    loaded.workspace.id().clone(),
                    loaded.workspace_state.clone(),
                )
            })
        }) else {
            return false;
        };
        if !self
            .conversation_surface()
            .is_some_and(|surface| surface.member_thread_inventory().needs_refresh())
        {
            return false;
        }

        let connectors = self.backend_client_connectors();
        if connectors.is_empty() {
            return false;
        }
        let Some(token) = self
            .conversation_surface_mut()
            .map(|surface| surface.member_thread_inventory_mut().begin_refresh())
        else {
            return false;
        };
        self.member_thread_inventory_receiver = Some(spawn_member_thread_inventory_worker(
            connectors,
            workspace_id,
            token,
            workspace_state,
            self.bootstrap.probe_timeout(),
        ));
        true
    }

    fn finish_member_thread_inventory_refresh(
        &mut self,
        workspace_id: &BerylWorkspaceId,
        token: MemberThreadInventoryRefreshToken,
        result: MemberThreadInventoryResult,
    ) {
        let active_workspace_id = self.loaded_workspace().map(|loaded| loaded.workspace.id());
        let active_token = self
            .conversation_surface()
            .map(|surface| surface.member_thread_inventory().refresh_token());
        if !member_thread_inventory_result_is_current(
            active_workspace_id,
            active_token,
            workspace_id,
            token,
        ) {
            return;
        }

        match result {
            MemberThreadInventoryResult::Refreshed {
                snapshot,
                registered_threads,
            } => {
                let registered_threads = registered_threads
                    .into_iter()
                    .map(|mut thread| {
                        if self.thread_ignores_backend_name_for_automatic_title(
                            thread.thread_id().as_str(),
                            thread.backend_name(),
                        ) {
                            thread.set_backend_name(None);
                        }
                        thread
                    })
                    .collect::<Vec<_>>();
                let mut touched_manifest = false;
                let Some(workspace_state) = self.loaded_workspace_mut().map(|loaded| {
                    for thread in registered_threads {
                        touched_manifest |= loaded.workspace_state.remember_thread(thread);
                    }
                    loaded.workspace_state.clone()
                }) else {
                    return;
                };
                if touched_manifest {
                    self.persist_current_workspace_state(true);
                }
                let partial_status = snapshot.partial_status_message();
                if let Some(surface) = self.conversation_surface_mut() {
                    if surface
                        .member_thread_inventory_mut()
                        .finish_refresh_for_token(token, snapshot, &workspace_state)
                    {
                        surface.reconcile_thread_selector_state();
                        if let Some(message) = partial_status.map(bounded_inventory_notice) {
                            surface.set_notice(SurfaceNotice::new(
                                "Thread inventory is incomplete",
                                message,
                            ));
                        }
                    }
                }
            }
            MemberThreadInventoryResult::Failed { message } => {
                let message = bounded_inventory_notice(message);
                warn!(error = %message, "member-thread inventory refresh failed");
                if let Some(surface) = self.conversation_surface_mut() {
                    if surface
                        .member_thread_inventory_mut()
                        .fail_refresh_for_token(token, message.clone())
                    {
                        surface.set_notice(SurfaceNotice::new(
                            "Thread inventory refresh failed",
                            message,
                        ));
                    }
                }
                self.block_if_backend_process_dead(
                    "Managed backend disconnected during thread inventory refresh",
                    "The backend process exited while Beryl was refreshing the workspace thread inventory.",
                    "Beryl could not refresh the workspace thread inventory because the managed backend process is no longer alive.",
                );
            }
        }
    }

    pub(super) fn reset_member_thread_inventory_for_workspace_state(&mut self) {
        self.apply_member_thread_inventory_event(MemberThreadInventoryEvent::MemberSetChanged);
    }

    pub(super) fn mark_member_thread_inventory_refresh_needed(&mut self) {
        self.apply_member_thread_inventory_event(
            MemberThreadInventoryEvent::InventoryContentsChanged,
        );
    }

    pub(super) fn apply_member_thread_inventory_event(
        &mut self,
        event: MemberThreadInventoryEvent,
    ) {
        let Some((workspace_id, workspace_state)) = self.loaded_workspace().map(|loaded| {
            (
                loaded.workspace.id().clone(),
                loaded.workspace_state.clone(),
            )
        }) else {
            return;
        };
        if matches!(
            event,
            MemberThreadInventoryEvent::MemberSetChanged
                | MemberThreadInventoryEvent::BackendTargetOpening
        ) {
            self.member_thread_inventory_receiver = None;
        }
        if let Some(surface) = self.conversation_surface_mut() {
            surface.member_thread_inventory_mut().apply_event(
                event,
                workspace_id,
                &workspace_state,
            );
            surface.reconcile_thread_selector_state();
        }
    }
}

#[cfg(test)]
#[path = "member_thread_inventory_runner_tests.rs"]
mod runner_tests;
