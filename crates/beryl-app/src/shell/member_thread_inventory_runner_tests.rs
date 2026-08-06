use std::{
    cell::Cell,
    collections::VecDeque,
    path::{Path, PathBuf},
    rc::Rc,
    sync::{Arc, Mutex},
    time::Duration,
};

use beryl_backend::{
    ThreadListBudget, ThreadListCollection, ThreadListCollectionStatus, ThreadListOptions,
    ThreadSummary,
};
use beryl_model::{
    conversation::WorkspaceConversationState,
    workspace::{BerylWorkspaceId, WorkspaceId},
};

use super::*;

#[derive(Clone)]
struct ScriptedElapsed {
    now: Rc<Cell<Duration>>,
}

enum ScriptedListAttempt {
    Collected {
        data: Vec<ThreadSummary>,
        pages_collected: usize,
        status: ThreadListCollectionStatus,
        advance: Duration,
    },
    Failed {
        data: Vec<ThreadSummary>,
        pages_collected: usize,
        message: String,
        advance: Duration,
    },
}

struct ScriptedInventoryOperations {
    targets: Vec<MemberThreadInventoryTarget>,
    elapsed: Rc<Cell<Duration>>,
    connect_advance: Duration,
    list_attempts: VecDeque<ScriptedListAttempt>,
    metadata_results: VecDeque<
        Result<ThreadSummary, crate::member_thread_inventory::ThreadForkParentMetadataReadError>,
    >,
    connect_calls: Vec<(usize, Duration)>,
    list_calls: Vec<(usize, ThreadListBudget)>,
    metadata_calls: Vec<(usize, String, Duration)>,
}

struct FakeManagedBackendState {
    elapsed: Duration,
    connect_advance: Duration,
    list_advance: Duration,
    read_advance: Duration,
    connect_error: bool,
    connect_calls: Vec<Duration>,
    list_calls: Vec<(ThreadListOptions, ThreadListBudget)>,
    read_calls: Vec<(String, Duration)>,
    list_results: VecDeque<Result<ThreadListCollection, ThreadListCollectionError>>,
    read_results: VecDeque<Result<ThreadSummary, ManagedBackendError>>,
}

#[derive(Clone)]
struct FakeManagedBackendConnector {
    state: Arc<Mutex<FakeManagedBackendState>>,
}

struct FakeManagedBackendSession {
    state: Arc<Mutex<FakeManagedBackendState>>,
}

#[derive(Clone)]
struct FakeManagedBackendElapsed {
    state: Arc<Mutex<FakeManagedBackendState>>,
}

impl MemberThreadInventoryElapsed for FakeManagedBackendElapsed {
    fn elapsed(&self) -> Duration {
        self.state.lock().unwrap().elapsed
    }
}

impl ManagedBackendInventoryConnector for FakeManagedBackendConnector {
    type Session = FakeManagedBackendSession;

    fn connect_inventory_client(
        &self,
        timeout: Duration,
    ) -> Result<Self::Session, ManagedBackendError> {
        let mut state = self.state.lock().unwrap();
        state.connect_calls.push(timeout);
        let advance = state.connect_advance;
        state.elapsed += advance;
        if state.connect_error {
            return Err(ManagedBackendError::RequestTimeout {
                method: "connect".to_string(),
                timeout,
            });
        }
        Ok(FakeManagedBackendSession {
            state: self.state.clone(),
        })
    }
}

impl ManagedBackendInventorySession for FakeManagedBackendSession {
    fn list_inventory_threads_bounded(
        &mut self,
        options: ThreadListOptions,
        budget: ThreadListBudget,
    ) -> Result<ThreadListCollection, ThreadListCollectionError> {
        let mut state = self.state.lock().unwrap();
        state.list_calls.push((options, budget));
        let advance = state.list_advance;
        state.elapsed += advance;
        state
            .list_results
            .pop_front()
            .expect("fake managed backend list result")
    }

    fn read_inventory_thread_metadata(
        &mut self,
        thread_id: &str,
        timeout: Duration,
    ) -> Result<ThreadSummary, ManagedBackendError> {
        let mut state = self.state.lock().unwrap();
        state.read_calls.push((thread_id.to_string(), timeout));
        let advance = state.read_advance;
        state.elapsed += advance;
        state
            .read_results
            .pop_front()
            .expect("fake managed backend metadata result")
    }
}

impl MemberThreadInventoryElapsed for ScriptedElapsed {
    fn elapsed(&self) -> Duration {
        self.now.get()
    }
}

impl MemberThreadInventoryOperations for ScriptedInventoryOperations {
    fn targets(&self) -> Vec<MemberThreadInventoryTarget> {
        self.targets.clone()
    }

    fn connect(
        &mut self,
        target: &MemberThreadInventoryTarget,
        timeout: Duration,
    ) -> Result<(), String> {
        self.connect_calls.push((target.key, timeout));
        self.elapsed.set(self.elapsed.get() + self.connect_advance);
        Ok(())
    }

    fn list_threads(
        &mut self,
        target: &MemberThreadInventoryTarget,
        _options: ThreadListOptions,
        list_budget: ThreadListBudget,
    ) -> MemberThreadInventoryListAttempt {
        self.list_calls.push((target.key, list_budget));
        match self
            .list_attempts
            .pop_front()
            .expect("scripted list attempt")
        {
            ScriptedListAttempt::Collected {
                data,
                pages_collected,
                status,
                advance,
            } => {
                self.elapsed.set(self.elapsed.get() + advance);
                MemberThreadInventoryListAttempt::Collected(ThreadListCollection {
                    data,
                    next_cursor: None,
                    pages_collected,
                    status,
                })
            }
            ScriptedListAttempt::Failed {
                data,
                pages_collected,
                message,
                advance,
            } => {
                self.elapsed.set(self.elapsed.get() + advance);
                MemberThreadInventoryListAttempt::Failed {
                    data,
                    pages_collected,
                    message,
                }
            }
        }
    }

    fn read_thread_metadata(
        &mut self,
        target: &MemberThreadInventoryTarget,
        thread_id: &str,
        timeout: Duration,
    ) -> Result<ThreadSummary, crate::member_thread_inventory::ThreadForkParentMetadataReadError>
    {
        self.metadata_calls
            .push((target.key, thread_id.to_string(), timeout));
        self.metadata_results
            .pop_front()
            .expect("scripted metadata result")
    }
}

fn scripted_operations(
    targets: Vec<WorkspaceId>,
    list_attempts: Vec<ScriptedListAttempt>,
    metadata_results: Vec<
        Result<ThreadSummary, crate::member_thread_inventory::ThreadForkParentMetadataReadError>,
    >,
) -> (ScriptedElapsed, ScriptedInventoryOperations) {
    let now = Rc::new(Cell::new(Duration::ZERO));
    let elapsed = ScriptedElapsed { now: now.clone() };
    let operations = ScriptedInventoryOperations {
        targets: targets
            .into_iter()
            .enumerate()
            .map(|(key, execution_target)| MemberThreadInventoryTarget {
                key,
                execution_target,
            })
            .collect(),
        elapsed: now,
        connect_advance: Duration::ZERO,
        list_attempts: list_attempts.into(),
        metadata_results: metadata_results.into(),
        connect_calls: Vec::new(),
        list_calls: Vec::new(),
        metadata_calls: Vec::new(),
    };
    (elapsed, operations)
}

fn limits(
    max_list_pages: usize,
    max_list_results: usize,
    max_metadata_reads: usize,
) -> MemberThreadInventoryJobLimits {
    MemberThreadInventoryJobLimits {
        elapsed_limit: Duration::from_secs(10),
        max_list_pages,
        max_list_results,
        max_metadata_reads,
    }
}

fn workspace_state(targets: &[WorkspaceId]) -> WorkspaceConversationState {
    let mut state = WorkspaceConversationState::default();
    state
        .designate_primary_execution_target(&targets[0])
        .unwrap();
    for target in &targets[1..] {
        state.attach_execution_target(target).unwrap();
    }
    state
}

fn summary(id: &str, cwd: &Path, forked_from_id: Option<&str>) -> ThreadSummary {
    ThreadSummary {
        id: id.to_string(),
        forked_from_id: forked_from_id.map(str::to_string),
        cwd: cwd.to_path_buf(),
        preview: format!("Preview {id}"),
        name: None,
        agent_nickname: None,
        path: None,
        created_at: 1,
        updated_at: 2,
        model_provider: "openai".to_string(),
        ephemeral: false,
    }
}

fn refreshed_snapshot(result: MemberThreadInventoryResult) -> MemberThreadInventorySnapshot {
    match result {
        MemberThreadInventoryResult::Refreshed { snapshot, .. } => snapshot,
        MemberThreadInventoryResult::Failed { message } => {
            panic!("expected refreshed inventory, got failure: {message}")
        }
    }
}

fn host_and_wsl_targets() -> Vec<WorkspaceId> {
    vec![
        WorkspaceId::host_windows(r"C:\inventory\host"),
        WorkspaceId::wsl_linux("Ubuntu", PathBuf::from("/inventory/wsl")),
    ]
}

#[test]
fn shared_page_budget_stops_dispatch_across_targets() {
    let targets = host_and_wsl_targets();
    let state = workspace_state(&targets);
    let first_row = summary(
        "host-thread",
        targets[0].canonical_path(),
        Some("backend-parent"),
    );
    let (elapsed, mut operations) = scripted_operations(
        targets,
        vec![ScriptedListAttempt::Collected {
            data: vec![first_row],
            pages_collected: 1,
            status: ThreadListCollectionStatus::Complete,
            advance: Duration::ZERO,
        }],
        Vec::new(),
    );

    let snapshot = refreshed_snapshot(run_member_thread_inventory(
        &mut operations,
        &elapsed,
        &BerylWorkspaceId::new("page-budget").unwrap(),
        state,
        limits(1, 10, 10),
    ));

    assert_eq!(operations.list_calls.len(), 1);
    assert_eq!(operations.connect_calls.len(), 1);
    assert!(
        snapshot
            .coverage()
            .partial()
            .unwrap()
            .row_coverage_truncated()
    );
}

#[test]
fn shared_result_budget_stops_dispatch_across_targets() {
    let targets = host_and_wsl_targets();
    let state = workspace_state(&targets);
    let first_row = summary(
        "host-thread",
        targets[0].canonical_path(),
        Some("backend-parent"),
    );
    let (elapsed, mut operations) = scripted_operations(
        targets,
        vec![ScriptedListAttempt::Collected {
            data: vec![first_row],
            pages_collected: 1,
            status: ThreadListCollectionStatus::Complete,
            advance: Duration::ZERO,
        }],
        Vec::new(),
    );

    let snapshot = refreshed_snapshot(run_member_thread_inventory(
        &mut operations,
        &elapsed,
        &BerylWorkspaceId::new("result-budget").unwrap(),
        state,
        limits(10, 1, 10),
    ));

    assert_eq!(operations.list_calls.len(), 1);
    assert_eq!(operations.connect_calls.len(), 1);
    assert!(
        snapshot
            .coverage()
            .partial()
            .unwrap()
            .row_coverage_truncated()
    );
}

#[test]
fn metadata_read_cap_marks_lineage_partial_without_extra_reads() {
    let target = WorkspaceId::host_windows(r"C:\inventory\host");
    let state = workspace_state(std::slice::from_ref(&target));
    let first = summary("first", target.canonical_path(), None);
    let second = summary("second", target.canonical_path(), None);
    let (elapsed, mut operations) = scripted_operations(
        vec![target],
        vec![ScriptedListAttempt::Collected {
            data: vec![first.clone(), second],
            pages_collected: 1,
            status: ThreadListCollectionStatus::Complete,
            advance: Duration::ZERO,
        }],
        vec![Ok(first)],
    );

    let snapshot = refreshed_snapshot(run_member_thread_inventory(
        &mut operations,
        &elapsed,
        &BerylWorkspaceId::new("metadata-budget").unwrap(),
        state,
        limits(10, 10, 1),
    ));

    assert_eq!(operations.metadata_calls.len(), 1);
    let partial = snapshot.coverage().partial().unwrap();
    assert!(partial.lineage_incomplete());
    assert!(!partial.row_coverage_truncated());
}

#[test]
fn each_request_receives_the_current_remaining_elapsed_budget() {
    let target = WorkspaceId::host_windows(r"C:\inventory\host");
    let state = workspace_state(std::slice::from_ref(&target));
    let row = summary("timed", target.canonical_path(), None);
    let (elapsed, mut operations) = scripted_operations(
        vec![target],
        vec![ScriptedListAttempt::Collected {
            data: vec![row.clone()],
            pages_collected: 1,
            status: ThreadListCollectionStatus::Complete,
            advance: Duration::from_secs(3),
        }],
        vec![Ok(row)],
    );
    operations.connect_advance = Duration::from_secs(2);

    let _ = refreshed_snapshot(run_member_thread_inventory(
        &mut operations,
        &elapsed,
        &BerylWorkspaceId::new("request-timeouts").unwrap(),
        state,
        limits(10, 10, 10),
    ));

    assert_eq!(operations.connect_calls[0].1, Duration::from_secs(10));
    assert_eq!(
        operations.list_calls[0].1.aggregate_timeout(),
        Duration::from_secs(8)
    );
    assert_eq!(operations.metadata_calls[0].2, Duration::from_secs(5));
}

#[test]
fn managed_backend_adapter_publishes_mapped_inventory_with_remaining_budgets() {
    let target = WorkspaceId::host_windows(r"C:\inventory\host");
    let state = workspace_state(std::slice::from_ref(&target));
    let listed = summary("managed", target.canonical_path(), None);
    let enriched = summary("managed", target.canonical_path(), Some("managed-parent"));
    let backend_state = Arc::new(Mutex::new(FakeManagedBackendState {
        elapsed: Duration::ZERO,
        connect_advance: Duration::from_secs(2),
        list_advance: Duration::from_secs(3),
        read_advance: Duration::ZERO,
        connect_error: false,
        connect_calls: Vec::new(),
        list_calls: Vec::new(),
        read_calls: Vec::new(),
        list_results: vec![Ok(ThreadListCollection {
            data: vec![listed],
            next_cursor: None,
            pages_collected: 1,
            status: ThreadListCollectionStatus::Complete,
        })]
        .into(),
        read_results: vec![Ok(enriched)].into(),
    }));
    let connector = FakeManagedBackendConnector {
        state: backend_state.clone(),
    };
    let elapsed = FakeManagedBackendElapsed {
        state: backend_state.clone(),
    };
    let workspace_id = BerylWorkspaceId::new("managed-publication").unwrap();
    let inventory = crate::member_thread_inventory::MemberThreadInventoryState::new(
        workspace_id.clone(),
        &state,
    );
    let token = inventory.refresh_token();

    let receiver = spawn_member_thread_inventory_worker_with(
        vec![(target.clone(), connector)],
        workspace_id.clone(),
        token,
        state,
        limits(10, 10, 10),
        move || elapsed,
    );
    let update = receiver.recv_timeout(Duration::from_secs(2)).unwrap();
    let MemberThreadInventoryUpdate::Finished {
        workspace_id: published_workspace_id,
        token: published_token,
        result,
    } = update;

    assert_eq!(published_workspace_id, workspace_id);
    assert_eq!(published_token, token);
    let snapshot = refreshed_snapshot(result);
    assert_eq!(snapshot.groups()[0].threads().len(), 1);
    assert_eq!(
        snapshot.groups()[0].threads()[0]
            .forked_from_id()
            .unwrap()
            .as_str(),
        "managed-parent"
    );

    let backend_state = backend_state.lock().unwrap();
    assert_eq!(backend_state.connect_calls, vec![Duration::from_secs(10)]);
    assert_eq!(backend_state.list_calls.len(), 1);
    assert_eq!(backend_state.list_calls[0].0.limit, Some(100));
    assert_eq!(
        backend_state.list_calls[0].0.cwd,
        vec![target.canonical_path().to_path_buf()]
    );
    assert_eq!(
        backend_state.list_calls[0].0.sort_key,
        Some(beryl_backend::ThreadSortKey::UpdatedAt)
    );
    assert_eq!(
        backend_state.list_calls[0].0.sort_direction,
        Some(beryl_backend::SortDirection::Desc)
    );
    assert_eq!(
        backend_state.list_calls[0].1.aggregate_timeout(),
        Duration::from_secs(8)
    );
    assert_eq!(
        backend_state.read_calls,
        vec![("managed".to_string(), Duration::from_secs(5))]
    );
}

#[test]
fn managed_backend_adapter_publishes_first_page_failure() {
    let target = WorkspaceId::host_windows(r"C:\inventory\host");
    let state = workspace_state(std::slice::from_ref(&target));
    let backend_state = Arc::new(Mutex::new(FakeManagedBackendState {
        elapsed: Duration::ZERO,
        connect_advance: Duration::from_secs(1),
        list_advance: Duration::ZERO,
        read_advance: Duration::ZERO,
        connect_error: false,
        connect_calls: Vec::new(),
        list_calls: Vec::new(),
        read_calls: Vec::new(),
        list_results: vec![Err(ThreadListCollectionError {
            data: Vec::new(),
            next_cursor: None,
            pages_collected: 0,
            source: ManagedBackendError::RequestTimeout {
                method: "thread/list".to_string(),
                timeout: Duration::from_secs(9),
            },
        })]
        .into(),
        read_results: VecDeque::new(),
    }));
    let connector = FakeManagedBackendConnector {
        state: backend_state.clone(),
    };
    let elapsed = FakeManagedBackendElapsed {
        state: backend_state.clone(),
    };
    let workspace_id = BerylWorkspaceId::new("managed-failure").unwrap();
    let inventory = crate::member_thread_inventory::MemberThreadInventoryState::new(
        workspace_id.clone(),
        &state,
    );
    let token = inventory.refresh_token();

    let receiver = spawn_member_thread_inventory_worker_with(
        vec![(target, connector)],
        workspace_id.clone(),
        token,
        state,
        limits(10, 10, 10),
        move || elapsed,
    );
    let update = receiver.recv_timeout(Duration::from_secs(2)).unwrap();
    let MemberThreadInventoryUpdate::Finished {
        workspace_id: published_workspace_id,
        token: published_token,
        result,
    } = update;

    assert_eq!(published_workspace_id, workspace_id);
    assert_eq!(published_token, token);
    let MemberThreadInventoryResult::Failed { message } = result else {
        panic!("expected managed adapter failure publication")
    };
    assert!(message.contains("Beryl could not refresh the workspace thread inventory"));
    assert!(message.contains("thread/list"));
    let backend_state = backend_state.lock().unwrap();
    assert_eq!(backend_state.connect_calls, vec![Duration::from_secs(10)]);
    assert_eq!(
        backend_state.list_calls[0].1.aggregate_timeout(),
        Duration::from_secs(9)
    );
    assert!(backend_state.read_calls.is_empty());
}

#[test]
fn failed_request_after_trusted_page_is_partial_but_first_page_failure_is_failed() {
    let target = WorkspaceId::host_windows(r"C:\inventory\host");
    let state = workspace_state(std::slice::from_ref(&target));
    let trusted = summary("trusted", target.canonical_path(), Some("backend-parent"));
    let (elapsed, mut operations) = scripted_operations(
        vec![target.clone()],
        vec![ScriptedListAttempt::Failed {
            data: vec![trusted],
            pages_collected: 1,
            message: "scripted later-page failure".to_string(),
            advance: Duration::ZERO,
        }],
        Vec::new(),
    );
    let partial = refreshed_snapshot(run_member_thread_inventory(
        &mut operations,
        &elapsed,
        &BerylWorkspaceId::new("trusted-page").unwrap(),
        state.clone(),
        limits(10, 10, 10),
    ));
    let coverage = partial.coverage().partial().unwrap();
    assert!(coverage.row_coverage_truncated());
    assert!(
        coverage
            .reasons()
            .iter()
            .any(|reason| reason.contains("scripted later-page failure"))
    );
    assert_eq!(partial.groups()[0].threads().len(), 1);
    assert_eq!(
        partial.groups()[0].threads()[0].thread_id().as_str(),
        "trusted"
    );

    let (elapsed, mut operations) = scripted_operations(
        vec![target],
        vec![ScriptedListAttempt::Failed {
            data: Vec::new(),
            pages_collected: 0,
            message: "scripted first-page failure".to_string(),
            advance: Duration::ZERO,
        }],
        Vec::new(),
    );
    let result = run_member_thread_inventory(
        &mut operations,
        &elapsed,
        &BerylWorkspaceId::new("first-page").unwrap(),
        state,
        limits(10, 10, 10),
    );
    assert!(matches!(result, MemberThreadInventoryResult::Failed { .. }));
}

#[test]
fn elapsed_budget_prevents_request_dispatch() {
    let target = WorkspaceId::host_windows(r"C:\inventory\host");
    let state = workspace_state(std::slice::from_ref(&target));
    let (elapsed, mut operations) = scripted_operations(vec![target], Vec::new(), Vec::new());
    elapsed.now.set(Duration::from_secs(10));

    let result = run_member_thread_inventory(
        &mut operations,
        &elapsed,
        &BerylWorkspaceId::new("elapsed-budget").unwrap(),
        state,
        limits(10, 10, 10),
    );

    assert!(matches!(result, MemberThreadInventoryResult::Failed { .. }));
    assert!(operations.connect_calls.is_empty());
    assert!(operations.list_calls.is_empty());
    assert!(operations.metadata_calls.is_empty());
}

#[test]
fn publication_guard_rejects_stale_workspace_or_token() {
    let active_workspace = BerylWorkspaceId::new("active").unwrap();
    let stale_workspace = BerylWorkspaceId::new("stale").unwrap();
    let state = WorkspaceConversationState::default();
    let mut inventory = crate::member_thread_inventory::MemberThreadInventoryState::new(
        active_workspace.clone(),
        &state,
    );
    let stale_token = inventory.refresh_token();
    inventory.prepare_for_backend_reopen();
    let active_token = inventory.refresh_token();

    assert!(member_thread_inventory_result_is_current(
        Some(&active_workspace),
        Some(active_token),
        &active_workspace,
        active_token,
    ));
    assert!(!member_thread_inventory_result_is_current(
        Some(&active_workspace),
        Some(active_token),
        &stale_workspace,
        active_token,
    ));
    assert!(!member_thread_inventory_result_is_current(
        Some(&active_workspace),
        Some(active_token),
        &active_workspace,
        stale_token,
    ));
}
