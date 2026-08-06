#![allow(dead_code)]

use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
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
}

use shell::phase_thread_preparation_core::*;

const TIMEOUT: Duration = Duration::from_secs(1);

#[derive(Default)]
struct FakeBackend {
    fork: Option<Result<ThreadForkResponse, PhaseThreadForkError<String>>>,
    rollback: VecDeque<Result<ThreadRollbackResponse, String>>,
    read: VecDeque<Result<ThreadReadResponse, String>>,
    delete: VecDeque<Result<(), PhaseThreadCleanupError<String>>>,
    calls: Vec<String>,
    cancel_after_fork: Option<Arc<AtomicBool>>,
    cancel_after_rollback: Option<Arc<AtomicBool>>,
    cancel_after_read: Option<Arc<AtomicBool>>,
}

struct TestCancellation(Arc<AtomicBool>);
impl PhaseThreadPreparationCancellation for TestCancellation {
    fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

impl PhaseThreadPreparationBackend for FakeBackend {
    type Error = String;
    fn fork_root(
        &mut self,
        id: &str,
        _: Duration,
    ) -> Result<ThreadForkResponse, PhaseThreadForkError<String>> {
        self.calls.push(format!("fork:{id}"));
        if let Some(cancel) = &self.cancel_after_fork {
            cancel.store(true, Ordering::Release);
        }
        self.fork.take().expect("fork configured")
    }
    fn rollback_child(
        &mut self,
        id: &str,
        turns: u32,
        _: Duration,
    ) -> Result<ThreadRollbackResponse, String> {
        self.calls.push(format!("rollback:{id}:{turns}"));
        if let Some(cancel) = &self.cancel_after_rollback {
            cancel.store(true, Ordering::Release);
        }
        self.rollback.pop_front().expect("rollback configured")
    }
    fn read_child(&mut self, id: &str, _: Duration) -> Result<ThreadReadResponse, String> {
        self.calls.push(format!("read:{id}"));
        if let Some(cancel) = &self.cancel_after_read {
            cancel.store(true, Ordering::Release);
        }
        self.read.pop_front().expect("read configured")
    }
    fn delete_child(
        &mut self,
        id: &str,
        _: Duration,
    ) -> Result<(), PhaseThreadCleanupError<String>> {
        self.calls.push(format!("delete:{id}"));
        self.delete.pop_front().unwrap_or(Ok(()))
    }
}

fn request() -> PhaseThreadPreparationRequest {
    let target = WorkspaceId::host_windows(r"C:\work\alpha");
    let binding = ConversationThreadMemberBinding::implicit_home(target.clone());
    let root_id = ConversationThreadId::new("root");
    let root = RegisteredConversationThread::new(root_id.clone(), target, "", None, 1, 1)
        .with_member_binding(binding);
    let mut state = WorkspaceConversationState::default();
    state.remember_thread(root);
    state.record_thread_as_orchestration_root(&root_id).unwrap();
    let root = state.thread_registration(&root_id).unwrap().clone();
    PhaseThreadPreparationRequest::new(
        PhaseThreadPreparationRequestParts {
            request_generation: 7,
            workspace_id: BerylWorkspaceId::new("alpha").unwrap(),
            source_thread_id: root_id.clone(),
            source_turn_id: ConversationTurnId::new("turn-source"),
            orchestration_root_thread_id: root_id.clone(),
            source_selection_thread_id: root_id,
        },
        &root,
        &root,
    )
    .unwrap()
}

fn info(id: &str, turns: serde_json::Value) -> ThreadInfo {
    serde_json::from_value(json!({"id": id, "forkedFromId":"root", "cwd":r"C:\work\alpha", "preview":"", "createdAt":1, "updatedAt":1, "modelProvider":"provider", "ephemeral":false, "status":{"type":"idle"}, "turns": turns})).unwrap()
}

fn fork(thread: ThreadInfo) -> ThreadForkResponse {
    ThreadForkResponse {
        thread,
        model: Some("model".into()),
        model_provider: Some("provider".into()),
        reasoning_effort: None,
    }
}
fn successful_fork(
    thread: ThreadInfo,
) -> Option<Result<ThreadForkResponse, PhaseThreadForkError<String>>> {
    Some(Ok(fork(thread)))
}
fn read(thread: ThreadInfo) -> ThreadReadResponse {
    ThreadReadResponse {
        thread,
        model: Some("model".into()),
        model_provider: Some("provider".into()),
        reasoning_effort: None,
    }
}
fn rollback(thread: ThreadInfo) -> ThreadRollbackResponse {
    ThreadRollbackResponse { thread }
}
fn user_turn(id: &str, messages: usize) -> serde_json::Value {
    json!({"id":id,"status":"completed","items":(0..messages).map(|n| json!({"type":"userMessage","id":format!("u{n}"),"content":[{"type":"text","text":"x"}]})).collect::<Vec<_>>()})
}
fn non_user_turn(id: &str) -> serde_json::Value {
    json!({"id":id,"status":"completed","items":[{"type":"agentMessage","id":"a","text":"x"}]})
}

fn run(backend: &mut FakeBackend) -> PhaseThreadPreparationOutcome {
    run_phase_thread_preparation(backend, request(), &(), TIMEOUT)
}

#[test]
fn request_validation_freezes_identity_and_success_echoes_it() {
    let request = request();
    assert_eq!(request.request_generation(), 7);
    assert_eq!(request.workspace_id().as_str(), "alpha");
    assert_eq!(
        request.source_thread_id().as_str(),
        request.source_selection_thread_id().as_str()
    );
    assert_eq!(request.canonical_cwd(), PathBuf::from(r"C:\work\alpha"));
    let forked = fork(info("child", json!([])));
    let read_back = ThreadReadResponse {
        thread: info("child", json!([])),
        model: None,
        model_provider: None,
        reasoning_effort: None,
    };
    let mut backend = FakeBackend {
        fork: Some(Ok(forked)),
        read: VecDeque::from([Ok(read_back)]),
        ..Default::default()
    };
    let outcome = run(&mut backend);
    assert_eq!(outcome.request, request);
    let PhaseThreadPreparationResult::Prepared {
        child,
        session_metadata,
    } = outcome.result
    else {
        panic!("expected prepared result");
    };
    assert_eq!(child.summary().id, "child");
    assert_eq!(session_metadata.model.as_deref(), Some("model"));
    assert_eq!(session_metadata.model_provider.as_deref(), Some("provider"));
    assert_eq!(session_metadata.reasoning_effort, None);

    let target = WorkspaceId::host_windows(r"C:\work\alpha");
    let source = RegisteredConversationThread::new(
        ConversationThreadId::new("root"),
        target.clone(),
        "",
        None,
        1,
        1,
    );
    let invalid = PhaseThreadPreparationRequest::new(
        PhaseThreadPreparationRequestParts {
            request_generation: 1,
            workspace_id: BerylWorkspaceId::new("alpha").unwrap(),
            source_thread_id: ConversationThreadId::new("root"),
            source_turn_id: ConversationTurnId::new("turn"),
            orchestration_root_thread_id: ConversationThreadId::new("root"),
            source_selection_thread_id: ConversationThreadId::new("other"),
        },
        &source,
        &source,
    );
    assert!(matches!(
        invalid,
        Err(PhaseThreadPreparationRequestError::SourceSelectionMismatch)
    ));
}

#[test]
fn request_validation_rejects_zero_blank_and_structural_registration_mismatches() {
    let target = WorkspaceId::host_windows(r"C:\work\alpha");
    let raw = RegisteredConversationThread::new(
        ConversationThreadId::new("root"),
        target.clone(),
        "",
        None,
        1,
        1,
    );
    let base = || PhaseThreadPreparationRequestParts {
        request_generation: 1,
        workspace_id: BerylWorkspaceId::new("alpha").unwrap(),
        source_thread_id: ConversationThreadId::new("root"),
        source_turn_id: ConversationTurnId::new("turn"),
        orchestration_root_thread_id: ConversationThreadId::new("root"),
        source_selection_thread_id: ConversationThreadId::new("root"),
    };
    let mut zero = base();
    zero.request_generation = 0;
    assert!(matches!(
        PhaseThreadPreparationRequest::new(zero, &raw, &raw),
        Err(PhaseThreadPreparationRequestError::ZeroGeneration)
    ));
    let mut blank = base();
    blank.source_turn_id = ConversationTurnId::new(" ");
    assert!(matches!(
        PhaseThreadPreparationRequest::new(blank, &raw, &raw),
        Err(PhaseThreadPreparationRequestError::BlankIdentity { .. })
    ));
    assert!(matches!(
        PhaseThreadPreparationRequest::new(base(), &raw, &raw),
        Err(PhaseThreadPreparationRequestError::SourceRootMismatch)
    ));

    let mut unbound_state = WorkspaceConversationState::default();
    unbound_state.remember_thread(raw.clone());
    unbound_state
        .record_thread_as_orchestration_root(&ConversationThreadId::new("root"))
        .unwrap();
    let unbound_root = unbound_state
        .thread_registration(&ConversationThreadId::new("root"))
        .unwrap()
        .clone();
    assert!(matches!(
        PhaseThreadPreparationRequest::new(base(), &unbound_root, &unbound_root),
        Err(PhaseThreadPreparationRequestError::MissingMemberBinding)
    ));

    let binding = ConversationThreadMemberBinding::implicit_home(target);
    let root_id = ConversationThreadId::new("root");
    let root = RegisteredConversationThread::new(
        root_id.clone(),
        binding.execution_target().clone(),
        "",
        None,
        1,
        1,
    )
    .with_member_binding(binding);
    let mut state = WorkspaceConversationState::default();
    state.remember_thread(root);
    state.record_thread_as_orchestration_root(&root_id).unwrap();
    let root = state.thread_registration(&root_id).unwrap().clone();
    let mut wrong_source = base();
    wrong_source.source_thread_id = ConversationThreadId::new("other");
    wrong_source.source_selection_thread_id = ConversationThreadId::new("other");
    assert!(matches!(
        PhaseThreadPreparationRequest::new(wrong_source, &root, &root),
        Err(PhaseThreadPreparationRequestError::SourceRegistrationMismatch)
    ));
    let wrong_target = RegisteredConversationThread::new(
        ConversationThreadId::new("other-root"),
        WorkspaceId::host_windows(r"C:\other"),
        "",
        None,
        1,
        1,
    )
    .with_member_binding(ConversationThreadMemberBinding::implicit_home(
        WorkspaceId::host_windows(r"C:\other"),
    ));
    assert!(matches!(
        PhaseThreadPreparationRequest::new(base(), &root, &wrong_target),
        Err(PhaseThreadPreparationRequestError::RootRegistrationMismatch)
    ));

    let source_id = ConversationThreadId::new("source");
    let root_id = ConversationThreadId::new("root");
    let alpha = WorkspaceId::host_windows(r"C:\work\alpha");
    let other = WorkspaceId::host_windows(r"C:\other");
    let source =
        RegisteredConversationThread::new(source_id.clone(), alpha.clone(), "", None, 1, 1)
            .with_member_binding(ConversationThreadMemberBinding::implicit_home(alpha));
    let root = RegisteredConversationThread::new(root_id.clone(), other.clone(), "", None, 1, 1)
        .with_member_binding(ConversationThreadMemberBinding::implicit_home(other));
    let mut state = WorkspaceConversationState::default();
    state.remember_thread(source);
    state.remember_thread(root);
    state.record_thread_as_orchestration_root(&root_id).unwrap();
    state
        .record_thread_orchestration_root(&source_id, &root_id)
        .unwrap();
    let source = state.thread_registration(&source_id).unwrap().clone();
    let root = state.thread_registration(&root_id).unwrap().clone();
    let target_mismatch = PhaseThreadPreparationRequestParts {
        request_generation: 1,
        workspace_id: BerylWorkspaceId::new("alpha").unwrap(),
        source_thread_id: source_id.clone(),
        source_turn_id: ConversationTurnId::new("turn"),
        orchestration_root_thread_id: root_id,
        source_selection_thread_id: source_id,
    };
    assert!(matches!(
        PhaseThreadPreparationRequest::new(target_mismatch, &source, &root),
        Err(PhaseThreadPreparationRequestError::RootTargetMismatch)
    ));
}

#[test]
fn request_validation_requires_non_rebinding_threads_and_current_available_binding() {
    let target = WorkspaceId::host_windows(r"C:\work\alpha");
    let mut state = WorkspaceConversationState::default();
    state.designate_primary_execution_target(&target).unwrap();
    let binding = state.binding_for_execution_target(&target).unwrap();
    let root_id = ConversationThreadId::new("root");
    let source_id = ConversationThreadId::new("source");
    state.remember_thread(
        RegisteredConversationThread::new(root_id.clone(), target.clone(), "", None, 1, 1)
            .with_member_binding(binding.clone()),
    );
    state.remember_thread(
        RegisteredConversationThread::new(source_id.clone(), target, "", None, 1, 1)
            .with_member_binding(binding.clone()),
    );
    state.record_thread_as_orchestration_root(&root_id).unwrap();
    state
        .record_thread_orchestration_root(&source_id, &root_id)
        .unwrap();
    let parts = || PhaseThreadPreparationRequestParts {
        request_generation: 1,
        workspace_id: BerylWorkspaceId::new("alpha").unwrap(),
        source_thread_id: source_id.clone(),
        source_turn_id: ConversationTurnId::new("turn"),
        orchestration_root_thread_id: root_id.clone(),
        source_selection_thread_id: source_id.clone(),
    };
    let source = state.thread_registration(&source_id).unwrap();
    let root = state.thread_registration(&root_id).unwrap();
    assert!(
        PhaseThreadPreparationRequest::new_with_available_binding(
            parts(),
            source,
            root,
            Some(&binding),
        )
        .is_ok()
    );
    assert!(matches!(
        PhaseThreadPreparationRequest::new_with_available_binding(parts(), source, root, None),
        Err(PhaseThreadPreparationRequestError::MemberBindingUnavailable)
    ));

    state
        .mark_thread_rebind_required(&source_id, "source requires rebind")
        .unwrap();
    let source = state.thread_registration(&source_id).unwrap();
    let root = state.thread_registration(&root_id).unwrap();
    assert!(matches!(
        PhaseThreadPreparationRequest::new_with_available_binding(
            parts(),
            source,
            root,
            Some(&binding),
        ),
        Err(PhaseThreadPreparationRequestError::SourceRebindRequired)
    ));
    state.clear_thread_rebind_required(&source_id).unwrap();
    state
        .mark_thread_rebind_required(&root_id, "root requires rebind")
        .unwrap();
    let source = state.thread_registration(&source_id).unwrap();
    let root = state.thread_registration(&root_id).unwrap();
    assert!(matches!(
        PhaseThreadPreparationRequest::new_with_available_binding(
            parts(),
            source,
            root,
            Some(&binding),
        ),
        Err(PhaseThreadPreparationRequestError::RootRebindRequired)
    ));
}

#[test]
fn counts_user_turns_once_and_skips_zero_for_unusual_history() {
    let turns = json!([
        user_turn("one", 2),
        non_user_turn("two"),
        user_turn("three", 1)
    ]);
    let mut backend = FakeBackend {
        fork: successful_fork(info("child", turns)),
        rollback: VecDeque::from([Ok(rollback(info("child", json!([]))))]),
        read: VecDeque::from([Ok(read(info("child", json!([]))))]),
        ..Default::default()
    };
    assert!(matches!(
        run(&mut backend).result,
        PhaseThreadPreparationResult::Prepared { .. }
    ));
    assert!(backend.calls.contains(&"rollback:child:2".to_string()));

    let mut backend = FakeBackend {
        fork: successful_fork(info("child", json!([non_user_turn("compacted")]))),
        read: VecDeque::from([Ok(read(info("child", json!([]))))]),
        ..Default::default()
    };
    assert!(matches!(
        run(&mut backend).result,
        PhaseThreadPreparationResult::Prepared { .. }
    ));
    assert!(
        !backend
            .calls
            .iter()
            .any(|call| call.starts_with("rollback:"))
    );
    assert_eq!(
        checked_rollback_turn_count(u32::MAX as usize + 1),
        Err("inherited user-turn count exceeds backend rollback limit".to_string())
    );
}

#[test]
fn rejects_invalid_child_identity_and_child_contracts_with_cleanup() {
    for child in ["", "root"] {
        let mut backend = FakeBackend {
            fork: successful_fork(info(child, json!([]))),
            ..Default::default()
        };
        let outcome = run(&mut backend);
        assert_eq!(outcome.request, request());
        assert!(matches!(
            outcome.result,
            PhaseThreadPreparationResult::IndeterminateFork { .. }
        ));
        assert!(!backend.calls.iter().any(|call| call.starts_with("delete:")));
    }

    let ephemeral = serde_json::from_value(json!({"id":"child","forkedFromId":"root","cwd":r"C:\work\alpha","preview":"","createdAt":1,"updatedAt":1,"modelProvider":"provider","ephemeral":true,"status":{"type":"idle"},"turns":[]})).unwrap();
    let mut backend = FakeBackend {
        fork: successful_fork(ephemeral),
        ..Default::default()
    };
    assert_known_child_failure(
        run(&mut backend),
        "child",
        PhaseThreadPreparationStage::ForkResponseValidation,
    );
    assert!(backend.calls.contains(&"delete:child".to_string()));

    for response in [
        json!({"id":"child","forkedFromId":"root","cwd":r"C:\work\alpha","preview":"","createdAt":1,"updatedAt":1,"modelProvider":"provider","ephemeral":false,"status":{"type":"active"},"turns":[]}),
        json!({"id":"child","forkedFromId":"other","cwd":r"C:\work\alpha","preview":"","createdAt":1,"updatedAt":1,"modelProvider":"provider","ephemeral":false,"status":{"type":"idle"},"turns":[]}),
        json!({"id":"child","forkedFromId":"root","cwd":r"C:\wrong","preview":"","createdAt":1,"updatedAt":1,"modelProvider":"provider","ephemeral":false,"status":{"type":"idle"},"turns":[]}),
    ] {
        let mut backend = FakeBackend {
            fork: successful_fork(serde_json::from_value(response).unwrap()),
            ..Default::default()
        };
        assert_known_child_failure(
            run(&mut backend),
            "child",
            PhaseThreadPreparationStage::ForkResponseValidation,
        );
    }
}

#[test]
fn rollback_read_and_runtime_failures_preserve_exact_orphan_identity() {
    let users = json!([user_turn("one", 1)]);
    let mut rollback_error = FakeBackend {
        fork: successful_fork(info("child", users.clone())),
        rollback: VecDeque::from([Err("rollback failed".into())]),
        ..Default::default()
    };
    assert_known_child_failure(
        run(&mut rollback_error),
        "child",
        PhaseThreadPreparationStage::Rollback,
    );

    let mut rollback_wrong = FakeBackend {
        fork: successful_fork(info("child", users)),
        rollback: VecDeque::from([Ok(rollback(info("other", json!([]))))]),
        ..Default::default()
    };
    assert_known_child_failure(
        run(&mut rollback_wrong),
        "child",
        PhaseThreadPreparationStage::RollbackResponseValidation,
    );

    let mut read_error = FakeBackend {
        fork: successful_fork(info("child", json!([]))),
        read: VecDeque::from([Err("read failed".into())]),
        ..Default::default()
    };
    assert_known_child_failure(
        run(&mut read_error),
        "child",
        PhaseThreadPreparationStage::Read,
    );

    let mut nonempty = FakeBackend {
        fork: successful_fork(info("child", json!([]))),
        read: VecDeque::from([Ok(read(info("child", json!([non_user_turn("left")]))))]),
        ..Default::default()
    };
    assert_known_child_failure(
        run(&mut nonempty),
        "child",
        PhaseThreadPreparationStage::ReadValidation,
    );

    let mut wrong_read_id = FakeBackend {
        fork: successful_fork(info("child", json!([]))),
        read: VecDeque::from([Ok(read(info("other", json!([]))))]),
        ..Default::default()
    };
    assert_known_child_failure(
        run(&mut wrong_read_id),
        "child",
        PhaseThreadPreparationStage::ReadValidation,
    );

    let mut conflict = FakeBackend {
        fork: successful_fork(info("child", json!([]))),
        read: VecDeque::from([Ok(ThreadReadResponse {
            thread: info("child", json!([])),
            model: Some("other".into()),
            model_provider: Some("provider".into()),
            reasoning_effort: None,
        })]),
        ..Default::default()
    };
    assert_known_child_failure(
        run(&mut conflict),
        "child",
        PhaseThreadPreparationStage::ReadValidation,
    );

    let mut absent = FakeBackend {
        fork: Some(
            ThreadForkResponse {
                thread: info("child", json!([])),
                model: None,
                model_provider: None,
                reasoning_effort: None,
            }
            .pipe(Ok),
        ),
        read: VecDeque::from([Ok(ThreadReadResponse {
            thread: info("child", json!([])),
            model: Some("late".into()),
            model_provider: None,
            reasoning_effort: None,
        })]),
        ..Default::default()
    };
    assert!(matches!(
        run(&mut absent).result,
        PhaseThreadPreparationResult::Prepared { .. }
    ));
}

#[test]
fn fork_commitment_cleanup_and_cancellation_are_truthful() {
    let mut definitive = FakeBackend {
        fork: Some(Err(PhaseThreadForkError::NotCommitted("rejected".into()))),
        ..Default::default()
    };
    assert!(matches!(
        run(&mut definitive).result,
        PhaseThreadPreparationResult::DefinitiveForkFailure { .. }
    ));
    let mut indeterminate = FakeBackend {
        fork: Some(Err(PhaseThreadForkError::Indeterminate("timeout".into()))),
        ..Default::default()
    };
    assert!(matches!(
        run(&mut indeterminate).result,
        PhaseThreadPreparationResult::IndeterminateFork { .. }
    ));
    assert!(
        indeterminate
            .calls
            .iter()
            .all(|call| !call.starts_with("delete:"))
    );

    let cancelled = Arc::new(AtomicBool::new(true));
    let mut backend = FakeBackend::default();
    let outcome = run_phase_thread_preparation(
        &mut backend,
        request(),
        &TestCancellation(cancelled.clone()),
        TIMEOUT,
    );
    assert!(matches!(
        outcome.result,
        PhaseThreadPreparationResult::CancelledBeforeFork
    ));
    assert!(backend.calls.is_empty());

    let cancelled = Arc::new(AtomicBool::new(false));
    let mut backend = FakeBackend {
        fork: successful_fork(info("child", json!([]))),
        cancel_after_fork: Some(cancelled.clone()),
        ..Default::default()
    };
    let outcome = run_phase_thread_preparation(
        &mut backend,
        request(),
        &TestCancellation(cancelled.clone()),
        TIMEOUT,
    );
    assert_known_child_failure(outcome, "child", PhaseThreadPreparationStage::Cancelled);
    assert!(backend.calls.contains(&"delete:child".to_string()));

    let mut cleanup_failed = FakeBackend {
        fork: successful_fork(info("child", json!([]))),
        read: VecDeque::from([Err("read failed".into())]),
        delete: VecDeque::from([Err(PhaseThreadCleanupError::ChildRemains(
            "delete rejected".into(),
        ))]),
        ..Default::default()
    };
    let outcome = run(&mut cleanup_failed);
    let PhaseThreadPreparationResult::KnownChildFailure(failure) = outcome.result else {
        panic!("expected known orphan");
    };
    assert_eq!(failure.child_id, "child");
    assert!(matches!(
        failure.cleanup,
        PhaseThreadCleanupOutcome::ChildRemains { .. }
    ));

    let mut cleanup_indeterminate = FakeBackend {
        fork: successful_fork(info("child", json!([]))),
        read: VecDeque::from([Err("read failed".into())]),
        delete: VecDeque::from([Err(PhaseThreadCleanupError::Indeterminate(
            "timeout".into(),
        ))]),
        ..Default::default()
    };
    let outcome = run(&mut cleanup_indeterminate);
    let PhaseThreadPreparationResult::KnownChildFailure(failure) = outcome.result else {
        panic!("expected known orphan");
    };
    assert_eq!(failure.child_id, "child");
    assert!(matches!(
        failure.cleanup,
        PhaseThreadCleanupOutcome::Indeterminate { .. }
    ));
}

#[test]
fn cancellation_after_rollback_and_read_cleans_the_exact_child() {
    let users = json!([user_turn("one", 1)]);
    let cancelled = Arc::new(AtomicBool::new(false));
    let mut after_rollback = FakeBackend {
        fork: successful_fork(info("child", users)),
        rollback: VecDeque::from([Ok(rollback(info("child", json!([]))))]),
        cancel_after_rollback: Some(cancelled.clone()),
        ..Default::default()
    };
    let outcome = run_phase_thread_preparation(
        &mut after_rollback,
        request(),
        &TestCancellation(cancelled),
        TIMEOUT,
    );
    assert_known_child_failure(outcome, "child", PhaseThreadPreparationStage::Cancelled);
    assert!(after_rollback.calls.contains(&"delete:child".to_string()));

    let cancelled = Arc::new(AtomicBool::new(false));
    let mut after_read = FakeBackend {
        fork: successful_fork(info("child", json!([]))),
        read: VecDeque::from([Ok(read(info("child", json!([]))))]),
        cancel_after_read: Some(cancelled.clone()),
        ..Default::default()
    };
    let outcome = run_phase_thread_preparation(
        &mut after_read,
        request(),
        &TestCancellation(cancelled),
        TIMEOUT,
    );
    assert_known_child_failure(outcome, "child", PhaseThreadPreparationStage::Cancelled);
    assert!(after_read.calls.contains(&"delete:child".to_string()));
}

#[test]
fn known_child_cleanup_acceptance_and_every_failure_echo_request_identity() {
    let expected = request();
    let mut backend = FakeBackend {
        fork: successful_fork(info("child", json!([]))),
        read: VecDeque::from([Err("read failed".into())]),
        ..Default::default()
    };
    let outcome = run(&mut backend);
    assert_eq!(outcome.request, expected);
    let PhaseThreadPreparationResult::KnownChildFailure(failure) = outcome.result else {
        panic!("expected known child failure");
    };
    assert!(matches!(
        failure.cleanup,
        PhaseThreadCleanupOutcome::Accepted
    ));

    for fork_failure in [
        PhaseThreadForkError::NotCommitted("rejected".to_string()),
        PhaseThreadForkError::Indeterminate("timeout".to_string()),
    ] {
        let mut backend = FakeBackend {
            fork: Some(Err(fork_failure)),
            ..Default::default()
        };
        let outcome = run(&mut backend);
        assert_eq!(outcome.request, request());
    }
}

fn assert_known_child_failure(
    outcome: PhaseThreadPreparationOutcome,
    id: &str,
    stage: PhaseThreadPreparationStage,
) {
    let PhaseThreadPreparationResult::KnownChildFailure(failure) = outcome.result else {
        panic!("expected known child failure");
    };
    assert_eq!(failure.child_id, id);
    assert_eq!(failure.stage, stage);
}

trait Pipe: Sized {
    fn pipe<T>(self, f: impl FnOnce(Self) -> T) -> T {
        f(self)
    }
}
impl<T> Pipe for T {}
