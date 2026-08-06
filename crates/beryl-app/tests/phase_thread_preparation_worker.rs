#![allow(dead_code)]

use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};

use beryl_backend::{ThreadForkResponse, ThreadInfo, ThreadReadResponse, ThreadRollbackResponse};
use beryl_model::{
    conversation::{
        ConversationThreadId, ConversationThreadMemberBinding, ConversationTurnId,
        RegisteredConversationThread, WorkspaceConversationState,
    },
    workspace::{BerylWorkspaceId, WorkspaceId},
};
use serde_json::json;

mod shell {
    #[path = "../../src/shell/phase_thread_preparation_core.rs"]
    pub(super) mod phase_thread_preparation_core;
    #[path = "../../src/shell/phase_thread_preparation_worker.rs"]
    pub(super) mod phase_thread_preparation_worker;
}

use shell::{phase_thread_preparation_core::*, phase_thread_preparation_worker::*};

fn request() -> PhaseThreadPreparationRequest {
    let target = WorkspaceId::host_windows(r"C:\work\alpha");
    let root_id = ConversationThreadId::new("root");
    let root = RegisteredConversationThread::new(root_id.clone(), target.clone(), "", None, 1, 1)
        .with_member_binding(ConversationThreadMemberBinding::implicit_home(target));
    let mut state = WorkspaceConversationState::default();
    state.remember_thread(root);
    state.record_thread_as_orchestration_root(&root_id).unwrap();
    let root = state.thread_registration(&root_id).unwrap().clone();
    PhaseThreadPreparationRequest::new(
        PhaseThreadPreparationRequestParts {
            request_generation: 1,
            workspace_id: BerylWorkspaceId::new("alpha").unwrap(),
            source_thread_id: root_id.clone(),
            source_turn_id: ConversationTurnId::new("turn"),
            orchestration_root_thread_id: root_id.clone(),
            source_selection_thread_id: root_id,
        },
        &root,
        &root,
    )
    .unwrap()
}

fn child() -> ThreadInfo {
    serde_json::from_value(json!({"id":"child","forkedFromId":"root","cwd":r"C:\work\alpha","preview":"","createdAt":1,"updatedAt":1,"modelProvider":"provider","ephemeral":false,"status":{"type":"idle"},"turns":[]})).unwrap()
}

struct ReadyBackend;
impl PhaseThreadPreparationBackend for ReadyBackend {
    type Error = String;
    fn fork_root(
        &mut self,
        _: &str,
        _: Duration,
    ) -> Result<ThreadForkResponse, PhaseThreadForkError<String>> {
        Ok(ThreadForkResponse {
            thread: child(),
            model: None,
            model_provider: None,
            reasoning_effort: None,
        })
    }
    fn rollback_child(
        &mut self,
        _: &str,
        _: u32,
        _: Duration,
    ) -> Result<ThreadRollbackResponse, String> {
        unreachable!("empty fork has no rollback")
    }
    fn read_child(&mut self, _: &str, _: Duration) -> Result<ThreadReadResponse, String> {
        Ok(ThreadReadResponse {
            thread: child(),
            model: None,
            model_provider: None,
            reasoning_effort: None,
        })
    }
    fn delete_child(
        &mut self,
        _: &str,
        _: Duration,
    ) -> Result<(), PhaseThreadCleanupError<String>> {
        Ok(())
    }
}

struct Connector {
    execution_target: WorkspaceId,
    connection_result: Result<(), String>,
    connect_calls: Arc<AtomicUsize>,
}

impl Connector {
    fn ready(connect_calls: Arc<AtomicUsize>) -> Self {
        Self {
            execution_target: WorkspaceId::host_windows(r"C:\work\alpha"),
            connection_result: Ok(()),
            connect_calls,
        }
    }
}
impl PhaseThreadPreparationConnector for Connector {
    type Backend = ReadyBackend;
    type Error = String;
    fn execution_target(&self) -> WorkspaceId {
        self.execution_target.clone()
    }
    fn connect_request_client(&self, _: Duration) -> Result<Self::Backend, Self::Error> {
        self.connect_calls.fetch_add(1, Ordering::SeqCst);
        self.connection_result.clone().map(|()| ReadyBackend)
    }
}

#[test]
fn worker_reports_connection_failure_without_dispatching_a_fork() {
    let connect_calls = Arc::new(AtomicUsize::new(0));
    let connector = Connector {
        execution_target: WorkspaceId::host_windows(r"C:\work\alpha"),
        connection_result: Err("offline".into()),
        connect_calls: connect_calls.clone(),
    };
    let outcome = run_phase_thread_preparation_worker(
        &connector,
        request(),
        Arc::new(AtomicBool::new(false)),
        Duration::from_secs(1),
    );
    assert!(
        matches!(outcome.result, PhaseThreadPreparationResult::DefinitiveForkFailure { ref detail } if detail.contains("independent managed-backend request client"))
    );
    assert_eq!(connect_calls.load(Ordering::SeqCst), 1);
}

#[test]
fn worker_receiver_completes_with_a_verified_result_without_gui_state() {
    let connect_calls = Arc::new(AtomicUsize::new(0));
    let receiver = spawn_phase_thread_preparation_worker_with(
        Connector::ready(connect_calls.clone()),
        request(),
        Arc::new(AtomicBool::new(false)),
        Duration::from_secs(1),
    );
    let PhaseThreadPreparationUpdate::Finished(outcome) = receiver
        .recv_timeout(Duration::from_secs(1))
        .expect("worker completion");
    assert!(
        matches!(outcome.result, PhaseThreadPreparationResult::Prepared { ref child, .. } if child.summary().id == "child")
    );
    assert_eq!(connect_calls.load(Ordering::SeqCst), 1);
    let worker_source = include_str!("../src/shell/phase_thread_preparation_worker.rs");
    let core_source = include_str!("../src/shell/phase_thread_preparation_core.rs");
    for source in [worker_source, core_source] {
        assert!(!source.contains("ShellView"));
        assert!(!source.contains("WorkspaceConversationState"));
        assert!(!source.contains("transcript_branch"));
    }
}

#[test]
fn worker_rejects_wrong_runtime_or_cwd_before_connecting() {
    for execution_target in [
        WorkspaceId::wsl_linux("Ubuntu", "/work/alpha"),
        WorkspaceId::host_windows(r"C:\work\other"),
    ] {
        let connect_calls = Arc::new(AtomicUsize::new(0));
        let connector = Connector {
            execution_target,
            connection_result: Ok(()),
            connect_calls: connect_calls.clone(),
        };
        let expected = request();
        let outcome = run_phase_thread_preparation_worker(
            &connector,
            expected.clone(),
            Arc::new(AtomicBool::new(false)),
            Duration::from_secs(1),
        );
        assert_eq!(outcome.request, expected);
        assert!(
            matches!(outcome.result, PhaseThreadPreparationResult::DefinitiveForkFailure { ref detail } if detail.contains("connector execution target"))
        );
        assert_eq!(connect_calls.load(Ordering::SeqCst), 0);
    }
}
