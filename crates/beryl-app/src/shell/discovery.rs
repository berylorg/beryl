use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::Duration,
};

use beryl_backend::{
    ManagedBackendError, ManagedBackendServer, ManagedBackendSession, ManagedBackendStartupStage,
    ThreadInfo, ThreadItem, ThreadSummary, WorkspacePathError, canonicalize_host_path,
    canonicalize_wsl_path,
};
use beryl_model::{
    semantic_graph::SemanticGraph,
    workspace::{BerylWorkspaceId, WorkspaceId},
};
use tracing::warn;

use crate::BerylWorkspacePersistence;
use crate::memory_diagnostics::MemoryMilestone;
use crate::{WorkspaceGraphRevision, WorkspaceGraphStateSnapshot};

use super::execution_detail::TranscriptImagePathResolver;
use super::lifecycle::blocked_state_for_error;
use super::thread_activation::{ExistingThreadActivationError, activate_existing_thread_direct};
use super::thread_selection::{
    KnownThreadSelection, ThreadSelectionRequest, resolve_known_thread_selection,
};
use super::transcript_image_sources::transcript_image_path_resolver_for_turns;
use super::workspace_members::{
    WorkspaceTargetResolutionError, resolve_primary_execution_target,
    validate_primary_execution_target_selection,
};
use super::workspace_persistence_worker::WorkspacePersistenceFlush;
use super::{
    BlockedState, OpenWorkspaceFailure, OpenedWorkspace, RetryTarget, SurfaceNotice,
    WorkspaceOpenIntent, WorkspaceUpdate,
};

#[derive(Clone, Debug)]
pub(super) struct WorkspaceOpenCancellation {
    cancelled: Arc<AtomicBool>,
}

impl WorkspaceOpenCancellation {
    pub(super) fn new() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    pub(super) fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

impl RetryTarget {
    pub(super) fn workspace_label(&self) -> String {
        match self {
            Self::Startup => "startup".to_string(),
            Self::WorkspacePrimary => "primary workspace member".to_string(),
            Self::Workspace(workspace) => workspace.display_label(),
            Self::HostPath(path) => format!("host-windows {path}"),
            Self::WslPath { distro_name, path } => format!("wsl-linux:{distro_name} {path}"),
        }
    }

    pub(super) fn workspace(&self) -> WorkspaceId {
        match self {
            Self::Startup => WorkspaceId::host_windows(""),
            Self::WorkspacePrimary => WorkspaceId::host_windows(""),
            Self::Workspace(workspace) => workspace.clone(),
            Self::HostPath(path) => WorkspaceId::host_windows(path.clone()),
            Self::WslPath { distro_name, path } => {
                WorkspaceId::wsl_linux(distro_name.clone(), path.clone())
            }
        }
    }
}

impl BlockedState {
    pub(super) fn failure_summary(&self) -> super::FailureSummary {
        super::FailureSummary {
            stage: self.stage,
            title: self.title,
            summary: self.summary.clone(),
        }
    }
}

pub(super) fn open_workspace_worker(
    workspace_persistence: BerylWorkspacePersistence,
    workspace_id: BerylWorkspaceId,
    target: RetryTarget,
    thread_selection: ThreadSelectionRequest,
    intent: WorkspaceOpenIntent,
    cancellation: WorkspaceOpenCancellation,
    workspace_persistence_flush: WorkspacePersistenceFlush,
    timeout: Duration,
    sender: mpsc::Sender<WorkspaceUpdate>,
) {
    MemoryMilestone::new("workspace_open_worker_start")
        .workspace_id(workspace_id.as_str())
        .runtime(target.workspace().runtime_mode().display_name())
        .log();
    if cancellation.is_cancelled() {
        MemoryMilestone::new("workspace_open_worker_cancelled")
            .workspace_id(workspace_id.as_str())
            .note("before_flush")
            .log();
        return;
    }

    if let Err(error) = workspace_persistence_flush.wait(timeout) {
        MemoryMilestone::new("workspace_persistence_flush_failed")
            .workspace_id(workspace_id.as_str())
            .log();
        let _ = sender.send(WorkspaceUpdate::Finished(Err(OpenWorkspaceFailure {
            stage: None,
            title: "Workspace state could not be flushed",
            summary:
                "Beryl could not finish saving pending workspace state before opening the backend."
                    .to_string(),
            detail: error,
            next_steps: vec![
                "Retry after pending workspace persistence has completed.".to_string(),
                "Check the logs for workspace persistence errors.".to_string(),
            ],
        })));
        return;
    }
    MemoryMilestone::new("workspace_persistence_flush_done")
        .workspace_id(workspace_id.as_str())
        .log();

    let _ = sender.send(WorkspaceUpdate::Detail(
        "Canonicalizing the selected execution target".to_string(),
    ));
    MemoryMilestone::new("workspace_metadata_load_start")
        .workspace_id(workspace_id.as_str())
        .runtime(target.workspace().runtime_mode().display_name())
        .log();
    let execution_target =
        match resolve_workspace_target(&workspace_persistence, &workspace_id, &target) {
            Ok(execution_target) => {
                MemoryMilestone::new("workspace_metadata_load_done")
                    .workspace_id(workspace_id.as_str())
                    .runtime(execution_target.runtime_mode().display_name())
                    .log();
                let _ = sender.send(WorkspaceUpdate::ResolvedExecutionTarget(
                    execution_target.clone(),
                ));
                execution_target
            }
            Err(error) => {
                MemoryMilestone::new("workspace_metadata_load_failed")
                    .workspace_id(workspace_id.as_str())
                    .log();
                let _ = sender.send(WorkspaceUpdate::Finished(Err(error)));
                return;
            }
        };

    if cancellation.is_cancelled() {
        MemoryMilestone::new("workspace_open_worker_cancelled")
            .workspace_id(workspace_id.as_str())
            .note("after_metadata")
            .log();
        return;
    }

    if intent == WorkspaceOpenIntent::UseAsPrimaryMember {
        let workspace_state = match workspace_persistence.load_workspace_state(&workspace_id) {
            Ok(state) => state,
            Err(error) => {
                let _ = sender.send(WorkspaceUpdate::Finished(Err(OpenWorkspaceFailure {
                    stage: None,
                    title: "Workspace state could not be loaded",
                    summary: "Beryl could not validate the selected workspace member before opening the backend.".to_string(),
                    detail: error.to_string(),
                    next_steps: vec![
                        "Verify that the semantic workspace state is readable.".to_string(),
                        "Retry after restoring workspace storage access.".to_string(),
                    ],
                })));
                return;
            }
        };
        if let Err(error) =
            validate_primary_execution_target_selection(&workspace_state, &execution_target)
        {
            let _ = sender.send(WorkspaceUpdate::Finished(Err(OpenWorkspaceFailure {
                stage: None,
                title: "Workspace member selection is invalid",
                summary: "Beryl could not use the selected execution target as the workspace's primary member.".to_string(),
                detail: error.to_string(),
                next_steps: vec![
                    "Choose a member inside the selected runtime environment.".to_string(),
                    "Detach overlapping members before changing to a conflicting path.".to_string(),
                ],
            })));
            return;
        }
    }

    let mut last_stage = ManagedBackendStartupStage::LaunchProcess;
    MemoryMilestone::new("backend_launch_start")
        .workspace_id(workspace_id.as_str())
        .runtime(execution_target.runtime_mode().display_name())
        .log();
    let result = match ManagedBackendServer::launch_and_probe_with_progress(
        execution_target.runtime_mode().clone(),
        execution_target.canonical_path().to_path_buf(),
        timeout,
        |progress| {
            last_stage = progress.stage();
            let _ = sender.send(WorkspaceUpdate::Progress(progress));
        },
    ) {
        Ok((mut server, mut session, report)) => {
            MemoryMilestone::new("backend_launch_probe_done")
                .workspace_id(workspace_id.as_str())
                .runtime(execution_target.runtime_mode().display_name())
                .backend_pid(server.process_id())
                .log();
            if cancellation.is_cancelled() {
                MemoryMilestone::new("workspace_open_worker_cancelled")
                    .workspace_id(workspace_id.as_str())
                    .note("after_backend_launch")
                    .log();
                shutdown_cancelled_open_server(&mut server, &execution_target);
                return;
            }

            let _ = sender.send(WorkspaceUpdate::Detail(
                "Checking hard stop capability support".to_string(),
            ));
            let hard_stop_capabilities = match session.probe_hard_stop_capabilities(timeout) {
                Ok(report) => report.capabilities(),
                Err(error) => {
                    tracing::warn!(
                        error = %error,
                        "hard-stop capability probe failed; disabling optional hard-stop targets"
                    );
                    beryl_backend::HardStopCapabilities::default()
                }
            };
            if cancellation.is_cancelled() {
                shutdown_cancelled_open_server(&mut server, &execution_target);
                return;
            }

            let mut known_threads =
                if matches!(thread_selection, ThreadSelectionRequest::Exact { .. }) {
                    let _ = sender.send(WorkspaceUpdate::Detail(
                        "Opening the selected conversation thread".to_string(),
                    ));
                    Vec::new()
                } else {
                    let _ = sender.send(WorkspaceUpdate::Detail(
                        "Loading existing conversation threads for this execution target"
                            .to_string(),
                    ));
                    load_workspace_threads(&mut session, &execution_target, timeout)
                };
            if cancellation.is_cancelled() {
                shutdown_cancelled_open_server(&mut server, &execution_target);
                return;
            }

            if matches!(thread_selection, ThreadSelectionRequest::Exact { .. })
                || !known_threads.is_empty()
            {
                let _ = sender.send(WorkspaceUpdate::Detail(
                    "Loading the selected conversation history".to_string(),
                ));
            }
            let selected_thread_history = load_selected_thread_history(
                &mut session,
                &workspace_persistence,
                &workspace_id,
                &execution_target,
                &known_threads,
                &thread_selection,
                timeout,
            );
            if let Some(thread) = selected_thread_history.thread_history.as_ref() {
                let item_count = thread
                    .turns
                    .iter()
                    .map(|turn| turn.items.len())
                    .sum::<usize>();
                let generated_image_count = thread
                    .turns
                    .iter()
                    .flat_map(|turn| turn.items.iter())
                    .filter(|item| matches!(item, ThreadItem::ImageGeneration(_)))
                    .count();
                MemoryMilestone::new("workspace_initial_thread_history_loaded")
                    .workspace_id(workspace_id.as_str())
                    .thread_id(thread.summary().id.as_str())
                    .history_counts(thread.turns.len(), item_count, generated_image_count)
                    .log();
            }
            if cancellation.is_cancelled() {
                MemoryMilestone::new("workspace_open_worker_cancelled")
                    .workspace_id(workspace_id.as_str())
                    .note("after_thread_history")
                    .log();
                shutdown_cancelled_open_server(&mut server, &execution_target);
                return;
            }

            if known_threads.is_empty()
                && let Some(thread) = selected_thread_history.thread_history.as_ref()
            {
                known_threads.push(thread.summary());
            }
            let _ = sender.send(WorkspaceUpdate::Detail(
                "Loading the persisted semantic graph for this workspace".to_string(),
            ));
            let (graph_snapshot, graph_warning) =
                load_workspace_graph(&workspace_persistence, &workspace_id);
            MemoryMilestone::new("workspace_graph_load_done")
                .workspace_id(workspace_id.as_str())
                .log();

            Ok(OpenedWorkspace {
                execution_target,
                server,
                report,
                hard_stop_capabilities,
                known_threads,
                selected_thread_id: selected_thread_history.selected_thread_id,
                selected_thread_history: selected_thread_history.thread_history,
                selected_thread_history_window: selected_thread_history.thread_history_window,
                selected_thread_image_resolver: selected_thread_history.image_resolver,
                selected_thread_session_metadata: selected_thread_history.thread_session_metadata,
                surface_notice: selected_thread_history.surface_notice,
                graph: graph_snapshot.graph,
                graph_revision: graph_snapshot.revision,
                graph_warning,
            })
        }
        Err(error) => {
            MemoryMilestone::new("backend_launch_failed")
                .workspace_id(workspace_id.as_str())
                .runtime(execution_target.runtime_mode().display_name())
                .note(last_stage.display_label())
                .log();
            Err(workspace_failure_from_backend(error, last_stage))
        }
    };

    if cancellation.is_cancelled() {
        MemoryMilestone::new("workspace_open_worker_cancelled")
            .workspace_id(workspace_id.as_str())
            .note("before_result_send")
            .log();
        if let Ok(opened) = result {
            shutdown_cancelled_opened_workspace(opened);
        }
        return;
    }

    if let Err(error) = sender.send(WorkspaceUpdate::Finished(result))
        && let WorkspaceUpdate::Finished(Ok(opened)) = error.0
    {
        shutdown_cancelled_opened_workspace(opened);
    }
}

fn shutdown_cancelled_open_server(
    server: &mut ManagedBackendServer,
    execution_target: &WorkspaceId,
) {
    if let Err(error) = server.shutdown() {
        warn!(
            workspace = %execution_target.display_label(),
            error = %error,
            "failed to shut down managed backend after workspace open was cancelled"
        );
    }
}

fn shutdown_cancelled_opened_workspace(mut opened: OpenedWorkspace) {
    shutdown_cancelled_open_server(&mut opened.server, &opened.execution_target);
}

fn load_workspace_graph(
    persistence: &BerylWorkspacePersistence,
    workspace_id: &BerylWorkspaceId,
) -> (WorkspaceGraphStateSnapshot, Option<String>) {
    match persistence.load_workspace_graph_state_snapshot(workspace_id) {
        Ok(snapshot) => (snapshot, None),
        Err(error) => {
            warn!(
                workspace_id = workspace_id.as_str(),
                error = %error,
                "failed to preload persisted semantic graph state"
            );
            (
                WorkspaceGraphStateSnapshot::new(
                    SemanticGraph::default(),
                    WorkspaceGraphRevision::default(),
                ),
                Some(error.to_string()),
            )
        }
    }
}

fn load_workspace_threads(
    session: &mut ManagedBackendSession,
    workspace: &WorkspaceId,
    timeout: Duration,
) -> Vec<ThreadSummary> {
    match session.list_threads(timeout) {
        Ok(mut threads) => {
            threads.retain(|thread| thread.cwd == workspace.canonical_path());
            threads.sort_by(|left, right| {
                right
                    .updated_at
                    .cmp(&left.updated_at)
                    .then_with(|| right.created_at.cmp(&left.created_at))
            });
            threads
        }
        Err(error) => {
            warn!(
                workspace = %workspace.display_label(),
                error = %error,
                "failed to seed known workspace threads"
            );
            Vec::new()
        }
    }
}

fn load_selected_thread_history(
    session: &mut ManagedBackendSession,
    persistence: &BerylWorkspacePersistence,
    workspace_id: &BerylWorkspaceId,
    execution_target: &WorkspaceId,
    known_threads: &[ThreadSummary],
    thread_selection: &ThreadSelectionRequest,
    timeout: Duration,
) -> SelectedThreadHistory {
    if let ThreadSelectionRequest::Exact { thread_id, label } = thread_selection {
        return match activate_existing_thread_direct(
            session,
            execution_target,
            thread_id,
            label,
            timeout,
        ) {
            Ok(activation) => SelectedThreadHistory {
                selected_thread_id: Some(thread_id.clone()),
                thread_session_metadata: Some(activation.session_metadata),
                image_resolver: selected_thread_image_resolver(
                    session,
                    persistence,
                    workspace_id,
                    execution_target,
                    &activation.thread.turns,
                    timeout,
                ),
                thread_history: Some(activation.thread),
                thread_history_window: Some(activation.history_window),
                surface_notice: None,
            },
            Err(ExistingThreadActivationError::RequiresRebind { detail }) => {
                SelectedThreadHistory {
                    selected_thread_id: None,
                    thread_session_metadata: None,
                    image_resolver: TranscriptImagePathResolver::default(),
                    thread_history: None,
                    thread_history_window: None,
                    surface_notice: Some(SurfaceNotice::new("Thread requires rebind", detail)),
                }
            }
            Err(ExistingThreadActivationError::Failed { message }) => SelectedThreadHistory {
                selected_thread_id: None,
                thread_session_metadata: None,
                image_resolver: TranscriptImagePathResolver::default(),
                thread_history: None,
                thread_history_window: None,
                surface_notice: Some(SurfaceNotice::new("Thread activation failed", message)),
            },
        };
    }

    let selection =
        resolve_known_thread_selection(known_threads, execution_target, thread_selection);

    match selection {
        KnownThreadSelection::Selected { thread_id, strict } => {
            let label = known_threads
                .iter()
                .find(|thread| thread.id == thread_id)
                .and_then(|thread| thread.name.as_deref())
                .unwrap_or(&thread_id);
            match activate_existing_thread_direct(
                session,
                execution_target,
                &thread_id,
                label,
                timeout,
            ) {
                Ok(activation) => SelectedThreadHistory {
                    selected_thread_id: Some(thread_id),
                    thread_session_metadata: Some(activation.session_metadata),
                    image_resolver: selected_thread_image_resolver(
                        session,
                        persistence,
                        workspace_id,
                        execution_target,
                        &activation.thread.turns,
                        timeout,
                    ),
                    thread_history: Some(activation.thread),
                    thread_history_window: Some(activation.history_window),
                    surface_notice: None,
                },
                Err(ExistingThreadActivationError::RequiresRebind { detail }) => {
                    warn!(
                        workspace = %execution_target.display_label(),
                        thread_id = %thread_id,
                        error = %detail,
                        "failed to preload selected workspace thread history"
                    );
                    SelectedThreadHistory {
                        selected_thread_id: (!strict).then_some(thread_id),
                        thread_session_metadata: None,
                        image_resolver: TranscriptImagePathResolver::default(),
                        thread_history: None,
                        thread_history_window: None,
                        surface_notice: strict
                            .then(|| SurfaceNotice::new("Thread requires rebind", detail)),
                    }
                }
                Err(ExistingThreadActivationError::Failed { message }) => {
                    warn!(
                        workspace = %execution_target.display_label(),
                        thread_id = %thread_id,
                        error = %message,
                        "failed to preload selected workspace thread history"
                    );
                    SelectedThreadHistory {
                        selected_thread_id: (!strict).then_some(thread_id),
                        thread_session_metadata: None,
                        image_resolver: TranscriptImagePathResolver::default(),
                        thread_history: None,
                        thread_history_window: None,
                        surface_notice: strict
                            .then(|| SurfaceNotice::new("Thread activation failed", message)),
                    }
                }
            }
        }
        KnownThreadSelection::None => SelectedThreadHistory {
            selected_thread_id: None,
            thread_session_metadata: None,
            image_resolver: TranscriptImagePathResolver::default(),
            thread_history: None,
            thread_history_window: None,
            surface_notice: None,
        },
    }
}

fn selected_thread_image_resolver(
    session: &mut ManagedBackendSession,
    persistence: &BerylWorkspacePersistence,
    workspace_id: &BerylWorkspaceId,
    execution_target: &WorkspaceId,
    turns: &[beryl_backend::TurnInfo],
    timeout: Duration,
) -> TranscriptImagePathResolver {
    match transcript_image_path_resolver_for_turns(
        persistence,
        workspace_id,
        execution_target.runtime_mode(),
        turns,
        session,
        timeout,
    ) {
        Ok(resolver) => resolver,
        Err(error) => {
            warn!(
                workspace_id = workspace_id.as_str(),
                error = %error,
                "failed to prepare transcript image source resolver for selected thread"
            );
            TranscriptImagePathResolver::default()
        }
    }
}

struct SelectedThreadHistory {
    selected_thread_id: Option<String>,
    thread_session_metadata: Option<beryl_backend::ThreadSessionMetadata>,
    image_resolver: TranscriptImagePathResolver,
    thread_history: Option<ThreadInfo>,
    thread_history_window: Option<super::transcript_history::TranscriptHistoryWindow>,
    surface_notice: Option<SurfaceNotice>,
}

fn resolve_workspace_target(
    persistence: &BerylWorkspacePersistence,
    workspace_id: &BerylWorkspaceId,
    target: &RetryTarget,
) -> Result<WorkspaceId, OpenWorkspaceFailure> {
    match target {
        RetryTarget::Startup => Err(OpenWorkspaceFailure {
            stage: None,
            title: "Startup retry target is not a backend workspace",
            summary: "Beryl cannot open a managed backend directly from the startup retry target."
                .to_string(),
            detail: "Retry startup discovery instead of backend workspace activation.".to_string(),
            next_steps: vec!["Use the startup retry action.".to_string()],
        }),
        RetryTarget::WorkspacePrimary => {
            let workspace_state =
                persistence
                    .load_workspace_state(workspace_id)
                    .map_err(|error| OpenWorkspaceFailure {
                        stage: None,
                        title: "Workspace state could not be loaded",
                        summary: "Beryl could not read the workspace runtime and member selection."
                            .to_string(),
                        detail: error.to_string(),
                        next_steps: vec![
                            "Verify that the semantic workspace state is readable.".to_string(),
                            "Retry after restoring workspace storage access.".to_string(),
                        ],
                    })?;
            resolve_primary_execution_target(&workspace_state)
                .map_err(workspace_failure_from_primary_target_error)?
                .ok_or_else(|| OpenWorkspaceFailure {
                    stage: None,
                    title: "Workspace runtime environment is not selected",
                    summary: "Beryl cannot open a managed backend until the workspace has a selected runtime environment.".to_string(),
                    detail: "Select a runtime environment or attach a workspace member before opening this semantic workspace.".to_string(),
                    next_steps: vec![
                        "Attach a host-Windows or WSL-Linux member for this workspace.".to_string(),
                        "Retry after the workspace has a primary member.".to_string(),
                    ],
                })
        }
        RetryTarget::Workspace(workspace) => Ok(workspace.clone()),
        RetryTarget::HostPath(path) => canonicalize_host_path(PathBuf::from(path).as_path())
            .map(WorkspaceId::host_windows)
            .map_err(workspace_failure_from_path_error),
        RetryTarget::WslPath { distro_name, path } => {
            canonicalize_wsl_path(distro_name, PathBuf::from(path).as_path())
                .map(|path| WorkspaceId::wsl_linux(distro_name.clone(), path))
                .map_err(workspace_failure_from_path_error)
        }
    }
}

fn workspace_failure_from_path_error(error: WorkspacePathError) -> OpenWorkspaceFailure {
    OpenWorkspaceFailure {
        stage: None,
        title: "Workspace path is invalid for the selected runtime",
        summary:
            "Beryl could not resolve the selected workspace into the canonical identity required for this runtime mode."
                .to_string(),
        detail: error.to_string(),
        next_steps: vec![
            "Verify that the workspace path exists.".to_string(),
            "If you selected WSL-Linux, re-check the distro and path.".to_string(),
            "Retry after correcting the workspace path.".to_string(),
        ],
    }
}

fn workspace_failure_from_primary_target_error(
    error: WorkspaceTargetResolutionError,
) -> OpenWorkspaceFailure {
    OpenWorkspaceFailure {
        stage: None,
        title: "Primary workspace member could not be resolved",
        summary: error.open_failure_summary().to_string(),
        detail: error.to_string(),
        next_steps: vec![
            "Verify that the selected runtime environment can resolve its home directory."
                .to_string(),
            "Retry after correcting the runtime environment or attached members.".to_string(),
        ],
    }
}

fn workspace_failure_from_backend(
    error: ManagedBackendError,
    stage: ManagedBackendStartupStage,
) -> OpenWorkspaceFailure {
    let blocked = blocked_state_for_error(error, 0, stage);
    OpenWorkspaceFailure {
        stage: blocked.stage,
        title: blocked.title,
        summary: blocked.summary,
        detail: blocked.detail,
        next_steps: blocked.next_steps,
    }
}
