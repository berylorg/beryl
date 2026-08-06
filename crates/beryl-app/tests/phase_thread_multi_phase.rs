#![allow(dead_code)]

use std::{
    collections::BTreeMap,
    sync::{Arc, atomic::AtomicBool, mpsc},
    time::Duration,
};

use beryl_app::{BerylWorkspacePersistence, LifecycleYieldOutcome};
use beryl_backend::{
    ThreadForkResponse, ThreadInfo, ThreadReadResponse, ThreadRollbackResponse, TurnStatus,
};
use beryl_model::{
    conversation::{
        ConversationThreadId, ConversationTurnId, RegisteredConversationThread,
        WorkspaceConversationState,
    },
    workspace::{BerylWorkspaceId, WorkspaceId},
};
use serde_json::json;

mod execution_detail {
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub(crate) struct UserInputFragment {
        pub(crate) text: String,
    }

    impl UserInputFragment {
        pub(crate) fn text(text: impl Into<String>) -> Self {
            Self { text: text.into() }
        }
    }
}
#[path = "../src/shell/lifecycle_continuation.rs"]
mod lifecycle_continuation;
#[path = "../src/shell/lifecycle_yield.rs"]
mod lifecycle_yield;
#[path = "../src/shell/notifications.rs"]
mod notifications;
#[path = "support/tempdir.rs"]
mod tempdir_support;

mod shell {
    pub(super) mod execution_detail {
        #[derive(Clone, Debug, PartialEq, Eq)]
        pub(crate) struct UserInputFragment {
            pub(crate) text: String,
        }

        impl UserInputFragment {
            pub(crate) fn text(text: impl Into<String>) -> Self {
                Self { text: text.into() }
            }
        }
    }

    #[path = "../../src/shell/phase_thread_preparation_core.rs"]
    pub(super) mod phase_thread_preparation_core;

    pub(super) mod phase_thread_preparation_worker {
        use super::phase_thread_preparation_core::PhaseThreadPreparationOutcome;

        pub(crate) enum PhaseThreadPreparationUpdate {
            Finished(PhaseThreadPreparationOutcome),
        }
    }

    #[path = "../../src/shell/phase_thread_transition.rs"]
    pub(super) mod phase_thread_transition;
    #[path = "../../src/shell/phase_thread_transition_applicator.rs"]
    pub(super) mod phase_thread_transition_applicator;
    #[path = "../../src/shell/phase_thread_transition_deferred.rs"]
    pub(super) mod phase_thread_transition_deferred;
    #[path = "../../src/shell/phase_thread_transition_guard.rs"]
    pub(super) mod phase_thread_transition_guard;
}

use lifecycle_continuation::{
    PHASE_CONTINUE_RESUME_TEXT, phase_continue_new_thread_handoff,
    take_phase_continue_new_thread_handoff_for_finished_worker,
};
use lifecycle_yield::LifecycleYieldState;
use shell::{
    execution_detail::UserInputFragment,
    phase_thread_preparation_core::{
        PhaseThreadCleanupError, PhaseThreadForkError, PhaseThreadPreparationBackend,
        PhaseThreadPreparationRequest, PhaseThreadPreparationRequestParts,
        PhaseThreadPreparationResult, run_phase_thread_preparation,
    },
    phase_thread_transition::{
        PhaseThreadCompletionKind, PhaseThreadPreparationTask, PhaseThreadPreparationTaskState,
        reduce_phase_thread_completion,
    },
    phase_thread_transition_applicator::{
        PhaseThreadCompletionHost, PreparedPhaseThreadActivation, apply_phase_thread_completion,
    },
    phase_thread_transition_deferred::PreparedPhaseThreadRegistration,
};

const WORKSPACE_ID: &str = "phase_thread_multi_phase";
const ROOT_ID: &str = "root";
const TIMEOUT: Duration = Duration::from_secs(1);

struct StatefulBackend {
    root_history: ThreadInfo,
    children: BTreeMap<String, ThreadInfo>,
    fork_roots: Vec<String>,
    rollback_calls: Vec<(String, u32)>,
    read_turn_counts: Vec<(String, usize)>,
}

impl StatefulBackend {
    fn new() -> Self {
        Self {
            root_history: thread(ROOT_ID, json!([user_turn("root-turn")])),
            children: BTreeMap::new(),
            fork_roots: Vec::new(),
            rollback_calls: Vec::new(),
            read_turn_counts: Vec::new(),
        }
    }
}

impl PhaseThreadPreparationBackend for StatefulBackend {
    type Error = String;

    fn fork_root(
        &mut self,
        root_id: &str,
        _: Duration,
    ) -> Result<ThreadForkResponse, PhaseThreadForkError<Self::Error>> {
        self.fork_roots.push(root_id.to_string());
        let child_id = format!("child{}", self.children.len() + 1);
        let child = thread(&child_id, json!([user_turn("root-turn")]));
        self.children.insert(child_id, child.clone());
        Ok(ThreadForkResponse {
            thread: child,
            model: Some("model".to_string()),
            model_provider: Some("provider".to_string()),
            reasoning_effort: None,
        })
    }

    fn rollback_child(
        &mut self,
        child_id: &str,
        turns: u32,
        _: Duration,
    ) -> Result<ThreadRollbackResponse, Self::Error> {
        self.rollback_calls.push((child_id.to_string(), turns));
        let child = self
            .children
            .get_mut(child_id)
            .ok_or_else(|| format!("unknown child {child_id}"))?;
        child.turns.clear();
        Ok(ThreadRollbackResponse {
            thread: child.clone(),
        })
    }

    fn read_child(
        &mut self,
        child_id: &str,
        _: Duration,
    ) -> Result<ThreadReadResponse, Self::Error> {
        let child = self
            .children
            .get(child_id)
            .cloned()
            .ok_or_else(|| format!("unknown child {child_id}"))?;
        self.read_turn_counts
            .push((child_id.to_string(), child.turns.len()));
        Ok(ThreadReadResponse {
            thread: child,
            model: Some("model".to_string()),
            model_provider: Some("provider".to_string()),
            reasoning_effort: None,
        })
    }

    fn delete_child(
        &mut self,
        child_id: &str,
        _: Duration,
    ) -> Result<(), PhaseThreadCleanupError<Self::Error>> {
        self.children.remove(child_id);
        Ok(())
    }
}

struct PersistenceHost<'a> {
    persistence: &'a BerylWorkspacePersistence,
    workspace_id: BerylWorkspaceId,
    state: WorkspaceConversationState,
    registrations: Vec<(ConversationThreadId, bool)>,
}

impl PhaseThreadCompletionHost for PersistenceHost<'_> {
    fn original_workspace_is_current(&self, _: &PhaseThreadPreparationRequest) -> bool {
        true
    }

    fn mark_inventory_refresh(&mut self) {}

    fn prepared_registration_validity(
        &self,
        request: &PhaseThreadPreparationRequest,
    ) -> Result<(), String> {
        let source = self
            .state
            .thread_registration(request.source_thread_id())
            .ok_or_else(|| "source registration missing".to_string())?;
        let root = self
            .state
            .thread_registration(request.orchestration_root_thread_id())
            .ok_or_else(|| "root registration missing".to_string())?;
        (source.rebind_required().is_none()
            && root.rebind_required().is_none()
            && self
                .state
                .binding_for_execution_target(request.execution_target())
                .as_ref()
                == Some(request.member_binding()))
        .then_some(())
        .ok_or_else(|| "frozen phase-thread registration became invalid".to_string())
    }

    fn register_prepared_child(
        &mut self,
        request: &PhaseThreadPreparationRequest,
        registration: &PreparedPhaseThreadRegistration,
        activate: bool,
    ) -> Result<(), String> {
        let child_id = registration.child_thread_id().clone();
        let child = RegisteredConversationThread::new(
            child_id.clone(),
            request.execution_target().clone(),
            "",
            None,
            registration.created_at_millis(),
            registration.updated_at_millis(),
        )
        .with_member_binding(request.member_binding().clone())
        .with_beryl_created();
        self.state.remember_thread(child);
        self.state
            .record_thread_orchestration_root(&child_id, request.orchestration_root_thread_id())
            .map_err(|error| error.to_string())?;
        if activate {
            self.state
                .activate_thread(&child_id)
                .ok_or_else(|| "prepared child could not be activated".to_string())?;
        }
        self.persistence
            .save_workspace_state(&self.workspace_id, &self.state)
            .map_err(|error| error.to_string())?;
        self.registrations.push((child_id, activate));
        Ok(())
    }

    fn report_or_defer(
        &mut self,
        _: &PhaseThreadPreparationRequest,
        title: &'static str,
        detail: String,
        _: bool,
        _: Option<PreparedPhaseThreadRegistration>,
    ) {
        panic!("unexpected phase-thread report: {title}: {detail}");
    }
}

fn thread(id: &str, turns: serde_json::Value) -> ThreadInfo {
    serde_json::from_value(json!({
        "id": id,
        "forkedFromId": ROOT_ID,
        "cwd": r"C:\work\phase-thread-multi-phase",
        "preview": "",
        "createdAt": 1,
        "updatedAt": 1,
        "modelProvider": "provider",
        "ephemeral": false,
        "status": {"type": "idle"},
        "turns": turns,
    }))
    .expect("test thread data should match the backend contract")
}

fn user_turn(id: &str) -> serde_json::Value {
    json!({
        "id": id,
        "status": "completed",
        "items": [{
            "type": "userMessage",
            "id": format!("{id}-user"),
            "content": [{"type": "text", "text": "phase input"}],
        }],
    })
}

fn initial_state() -> WorkspaceConversationState {
    let target = WorkspaceId::host_windows(r"C:\work\phase-thread-multi-phase");
    let mut state = WorkspaceConversationState::default();
    state
        .designate_primary_execution_target(&target)
        .expect("the exact host execution target should be selectable");
    let binding = state
        .binding_for_execution_target(&target)
        .expect("the selected target should have an exact member binding");
    state.remember_thread(
        RegisteredConversationThread::new(
            ConversationThreadId::new(ROOT_ID),
            target.clone(),
            "",
            None,
            1,
            1,
        )
        .with_member_binding(binding)
        .with_beryl_created(),
    );
    state
        .record_thread_as_orchestration_root(&ConversationThreadId::new(ROOT_ID))
        .expect("root registration should accept root provenance");
    state
}

fn request_for(
    state: &WorkspaceConversationState,
    workspace_id: BerylWorkspaceId,
    source_id: &str,
    source_turn_id: &str,
    generation: u64,
) -> PhaseThreadPreparationRequest {
    let source_id = ConversationThreadId::new(source_id);
    let root_id = ConversationThreadId::new(ROOT_ID);
    let source = state
        .thread_registration(&source_id)
        .expect("source should be registered")
        .clone();
    let root = state
        .thread_registration(&root_id)
        .expect("root should be registered")
        .clone();
    PhaseThreadPreparationRequest::new_with_available_binding(
        PhaseThreadPreparationRequestParts {
            request_generation: generation,
            workspace_id,
            source_thread_id: source_id.clone(),
            source_turn_id: ConversationTurnId::new(source_turn_id),
            orchestration_root_thread_id: root_id,
            source_selection_thread_id: source_id,
        },
        &source,
        &root,
        source.member_binding(),
    )
    .expect("the selected source and its root should be valid for phase preparation")
}

fn prepare_and_activate(
    host: &mut PersistenceHost<'_>,
    backend: &mut StatefulBackend,
    request: PhaseThreadPreparationRequest,
    resume_fragment: UserInputFragment,
) -> PreparedPhaseThreadActivation {
    let outcome = run_phase_thread_preparation(backend, request.clone(), &(), TIMEOUT);
    assert_eq!(outcome.request, request);
    assert!(matches!(
        outcome.result,
        PhaseThreadPreparationResult::Prepared { .. }
    ));
    let (_sender, receiver) = mpsc::channel();
    let task = PhaseThreadPreparationTask::new(
        request,
        resume_fragment,
        Arc::new(AtomicBool::new(false)),
        receiver,
    );
    let decision = reduce_phase_thread_completion(
        PhaseThreadPreparationTaskState::Active,
        true,
        true,
        true,
        PhaseThreadCompletionKind::Prepared,
    );
    apply_phase_thread_completion(host, task, outcome.result, decision, None)
        .expect("an exact prepared child should register and activate")
}

fn assert_root_provenance(state: &WorkspaceConversationState, thread_id: &str) {
    assert_eq!(
        state
            .thread_registration(&ConversationThreadId::new(thread_id))
            .expect("thread should be registered")
            .orchestration_root_thread_id(),
        Some(&ConversationThreadId::new(ROOT_ID))
    );
}

#[test]
fn phase_continue_new_thread_repeatedly_forks_the_original_root_and_persists_provenance() {
    let root = tempdir_support::temp_dir("beryl-phase-thread-multi-phase-");
    let persistence = BerylWorkspacePersistence::new(&root);
    let workspace_id = BerylWorkspaceId::new(WORKSPACE_ID).unwrap();
    let state = initial_state();
    persistence
        .save_workspace_state(&workspace_id, &state)
        .expect("root state should persist");
    let mut host = PersistenceHost {
        persistence: &persistence,
        workspace_id: workspace_id.clone(),
        state,
        registrations: Vec::new(),
    };
    let mut backend = StatefulBackend::new();

    let mut yields = LifecycleYieldState::default();
    assert!(yields.record(
        ROOT_ID,
        "root-turn",
        LifecycleYieldOutcome::PhaseContinueNewThread
    ));
    let terminal = yields
        .apply_terminal_turn(ROOT_ID, "root-turn")
        .expect("the root yield should become terminal");
    let mut pending = phase_continue_new_thread_handoff(&terminal, TurnStatus::Completed);
    let root_handoff =
        take_phase_continue_new_thread_handoff_for_finished_worker(&mut pending, Some(ROOT_ID))
            .expect("the exact finished root worker should consume its handoff");
    assert_eq!(root_handoff.source_thread_id(), ROOT_ID);
    assert_eq!(root_handoff.source_turn_id(), "root-turn");
    assert_eq!(
        root_handoff.resume_fragment().text,
        PHASE_CONTINUE_RESUME_TEXT
    );

    let child1_request = request_for(&host.state, workspace_id.clone(), ROOT_ID, "root-turn", 1);
    let child1 = prepare_and_activate(
        &mut host,
        &mut backend,
        child1_request,
        UserInputFragment::text(root_handoff.resume_fragment().text),
    );
    assert_eq!(child1.child.summary().id, "child1");
    assert_eq!(child1.resume_fragment.text, PHASE_CONTINUE_RESUME_TEXT);
    assert_eq!(backend.fork_roots, vec![ROOT_ID]);
    assert_eq!(backend.rollback_calls, vec![("child1".to_string(), 1)]);
    assert_eq!(backend.read_turn_counts, vec![("child1".to_string(), 0)]);
    assert_eq!(
        child1.child.summary().forked_from_id.as_deref(),
        Some(ROOT_ID)
    );
    assert!(child1.child.turns.is_empty());
    assert_eq!(
        host.state.active_thread().map(ConversationThreadId::as_str),
        Some("child1")
    );
    assert_root_provenance(&host.state, ROOT_ID);
    assert_root_provenance(&host.state, "child1");
    let reloaded = persistence.load_workspace_state(&workspace_id).unwrap();
    assert_root_provenance(&reloaded, ROOT_ID);
    assert_root_provenance(&reloaded, "child1");
    host.state = reloaded;

    assert!(yields.record(
        "child1",
        "child1-turn",
        LifecycleYieldOutcome::PhaseContinueNewThread
    ));
    let terminal = yields
        .apply_terminal_turn("child1", "child1-turn")
        .expect("the first phase child yield should become terminal");
    let mut pending = phase_continue_new_thread_handoff(&terminal, TurnStatus::Completed);
    let child1_handoff =
        take_phase_continue_new_thread_handoff_for_finished_worker(&mut pending, Some("child1"))
            .expect("the exact finished child worker should consume its handoff");
    assert_eq!(child1_handoff.source_thread_id(), "child1");
    assert_eq!(child1_handoff.source_turn_id(), "child1-turn");
    assert_eq!(
        child1_handoff.resume_fragment().text,
        PHASE_CONTINUE_RESUME_TEXT
    );

    let child2_request = request_for(
        &host.state,
        workspace_id.clone(),
        "child1",
        "child1-turn",
        2,
    );
    let child2 = prepare_and_activate(
        &mut host,
        &mut backend,
        child2_request,
        UserInputFragment::text(child1_handoff.resume_fragment().text),
    );
    assert_eq!(child2.child.summary().id, "child2");
    assert_eq!(child2.resume_fragment.text, PHASE_CONTINUE_RESUME_TEXT);
    assert_eq!(backend.fork_roots, vec![ROOT_ID, ROOT_ID]);
    assert_eq!(
        backend.rollback_calls,
        vec![("child1".to_string(), 1), ("child2".to_string(), 1)]
    );
    assert_eq!(
        backend.read_turn_counts,
        vec![("child1".to_string(), 0), ("child2".to_string(), 0)]
    );
    assert_eq!(
        child2.child.summary().forked_from_id.as_deref(),
        Some(ROOT_ID)
    );
    assert!(child2.child.turns.is_empty());
    assert_eq!(
        host.state.active_thread().map(ConversationThreadId::as_str),
        Some("child2")
    );
    assert_eq!(
        host.registrations,
        vec![
            (ConversationThreadId::new("child1"), true),
            (ConversationThreadId::new("child2"), true),
        ]
    );
    let reloaded = persistence.load_workspace_state(&workspace_id).unwrap();
    for thread_id in [ROOT_ID, "child1", "child2"] {
        assert_root_provenance(&reloaded, thread_id);
    }

    assert!(yields.record("child2", "child2-turn", LifecycleYieldOutcome::PlanComplete));
    let terminal = yields
        .apply_terminal_turn("child2", "child2-turn")
        .expect("the final phase yield should become terminal");
    assert!(phase_continue_new_thread_handoff(&terminal, TurnStatus::Completed).is_none());
    assert_eq!(backend.children.len(), 2);
    assert_eq!(backend.fork_roots.len(), 2);
    assert_eq!(host.state.threads().len(), 3);

    root.close()
        .expect("task-owned temporary storage should clean up");
}
