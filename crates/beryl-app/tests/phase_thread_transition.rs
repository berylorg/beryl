#![allow(dead_code)]

use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

use beryl_backend::{ThreadInfo, ThreadSessionMetadata};
use beryl_model::{
    conversation::{
        ConversationThreadId, ConversationThreadMemberBinding, ConversationTurnId,
        RegisteredConversationThread, WorkspaceConversationState,
    },
    workspace::{BerylWorkspaceId, WorkspaceId},
};
use serde_json::json;

mod shell {
    pub(super) mod execution_detail {
        #[derive(Clone, Debug, PartialEq, Eq)]
        pub(crate) struct UserInputFragment {
            pub(crate) text: String,
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

    #[path = "../../src/shell/phase_thread_transition_guard.rs"]
    pub(super) mod phase_thread_transition_guard;

    #[path = "../../src/shell/phase_thread_transition_applicator.rs"]
    pub(super) mod phase_thread_transition_applicator;

    #[path = "../../src/shell/phase_thread_transition_deferred.rs"]
    pub(super) mod phase_thread_transition_deferred;

    #[path = "../../src/shell/phase_thread_workspace_deletion.rs"]
    pub(super) mod phase_thread_workspace_deletion;
}

use shell::{
    execution_detail::UserInputFragment,
    phase_thread_preparation_core::{
        PhaseThreadPreparationOutcome, PhaseThreadPreparationRequest,
        PhaseThreadPreparationRequestParts, PhaseThreadPreparationResult,
    },
    phase_thread_preparation_worker::PhaseThreadPreparationUpdate,
    phase_thread_transition::{
        PhaseThreadCompletionKind, PhaseThreadCompletionNotice,
        PhaseThreadContinuationStartDecision, PhaseThreadCurrentIdentity,
        PhaseThreadPreparationPoll, PhaseThreadPreparationTask, PhaseThreadPreparationTaskState,
        PhaseThreadResultGuardFailure, PhaseThreadTransitionOwner,
        bounded_phase_thread_notice_detail, guard_phase_thread_preparation_result,
        phase_thread_request_registrations_are_available, reduce_phase_thread_completion,
        reduce_phase_thread_continuation_start, reduce_phase_thread_disconnect,
    },
    phase_thread_transition_applicator::{
        PhaseThreadCompletionHost, PhaseThreadSourceQueueHost,
        apply_deferred_prepared_registration, apply_phase_thread_completion,
        fail_accepted_source_pending_input,
    },
    phase_thread_transition_deferred::{
        DeferredPhaseThreadOutcome, PreparedPhaseThreadRegistration,
    },
    phase_thread_workspace_deletion::{
        PhaseThreadWorkspaceDeletionDrain, PhaseThreadWorkspaceDeletionHost,
        PhaseThreadWorkspaceDeletionPoll, complete_phase_thread_workspace_deletion,
        poll_phase_thread_workspace_deletion,
    },
};

fn request(generation: u64) -> PhaseThreadPreparationRequest {
    request_for_workspace(generation, "alpha")
}

fn request_for_workspace(generation: u64, workspace_id: &str) -> PhaseThreadPreparationRequest {
    let target = WorkspaceId::host_windows(r"C:\work\alpha");
    let binding = ConversationThreadMemberBinding::implicit_home(target.clone());
    let root_id = ConversationThreadId::new("root");
    let root = RegisteredConversationThread::new(root_id.clone(), target, "", None, 1, 1)
        .with_member_binding(binding.clone());
    let mut state = WorkspaceConversationState::default();
    state.remember_thread(root);
    state.record_thread_as_orchestration_root(&root_id).unwrap();
    let root = state.thread_registration(&root_id).unwrap().clone();
    PhaseThreadPreparationRequest::new(
        PhaseThreadPreparationRequestParts {
            request_generation: generation,
            workspace_id: BerylWorkspaceId::new(workspace_id).unwrap(),
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

fn current(request: &PhaseThreadPreparationRequest) -> PhaseThreadCurrentIdentity {
    PhaseThreadCurrentIdentity {
        request_generation: request.request_generation(),
        workspace_id: request.workspace_id().clone(),
        source_thread_id: request.source_thread_id().clone(),
        source_turn_id: request.source_turn_id().clone(),
        orchestration_root_thread_id: request.orchestration_root_thread_id().clone(),
        source_selection_thread_id: request.source_selection_thread_id().clone(),
        execution_target: request.execution_target().clone(),
        canonical_cwd: request.canonical_cwd().to_path_buf(),
        member_binding: request.member_binding().clone(),
        available_member_binding: Some(request.member_binding().clone()),
        source_rebind_required: false,
        root_rebind_required: false,
    }
}

fn prepared_child(id: &str) -> ThreadInfo {
    serde_json::from_value(json!({
        "id": id,
        "forkedFromId": "root",
        "cwd": r"C:\work\alpha",
        "preview": "",
        "createdAt": 1,
        "updatedAt": 1,
        "modelProvider": "provider",
        "ephemeral": false,
        "status": {"type": "idle"},
        "turns": []
    }))
    .unwrap()
}

#[derive(Default)]
struct CompletionHost {
    current: bool,
    validity_error: Option<String>,
    registration_error: Option<String>,
    registrations: Vec<(String, bool)>,
    refreshes: usize,
    reports: Vec<(String, String, bool)>,
    deferred_registrations: Vec<PreparedPhaseThreadRegistration>,
}

#[derive(Default)]
struct SourceQueueHost {
    queue_present: bool,
    failures: Vec<(String, String)>,
}

struct WorkspaceDeletionHost {
    owner: PhaseThreadTransitionOwner,
    current: bool,
    validity_error: Option<String>,
    registrations: Vec<(String, bool)>,
    refreshes: usize,
    published_refresh_inventory: bool,
    published_known_children: Vec<ConversationThreadId>,
    published_notice: Option<(String, String)>,
    publication_before_barrier: bool,
    deferred_outcomes_at_barrier: usize,
    admission_available_at_barrier: bool,
    barrier_captured: bool,
    worker_started: bool,
    worker_start_error: Option<String>,
}

impl Default for WorkspaceDeletionHost {
    fn default() -> Self {
        Self {
            owner: PhaseThreadTransitionOwner::default(),
            current: true,
            validity_error: None,
            registrations: Vec::new(),
            refreshes: 0,
            published_refresh_inventory: false,
            published_known_children: Vec::new(),
            published_notice: None,
            publication_before_barrier: false,
            deferred_outcomes_at_barrier: 0,
            admission_available_at_barrier: false,
            barrier_captured: false,
            worker_started: false,
            worker_start_error: None,
        }
    }
}

impl PhaseThreadCompletionHost for WorkspaceDeletionHost {
    fn original_workspace_is_current(&self, _: &PhaseThreadPreparationRequest) -> bool {
        self.current
    }

    fn mark_inventory_refresh(&mut self) {
        self.refreshes += 1;
    }

    fn prepared_registration_validity(
        &self,
        _: &PhaseThreadPreparationRequest,
    ) -> Result<(), String> {
        self.validity_error.clone().map_or(Ok(()), Err)
    }

    fn register_prepared_child(
        &mut self,
        _: &PhaseThreadPreparationRequest,
        registration: &PreparedPhaseThreadRegistration,
        activate: bool,
    ) -> Result<(), String> {
        self.registrations.push((
            registration.child_thread_id().as_str().to_string(),
            activate,
        ));
        Ok(())
    }

    fn report_or_defer(
        &mut self,
        request: &PhaseThreadPreparationRequest,
        title: &'static str,
        detail: String,
        refresh_inventory: bool,
        prepared_registration: Option<PreparedPhaseThreadRegistration>,
    ) {
        self.owner.defer_outcome(DeferredPhaseThreadOutcome::new(
            request.clone(),
            title.to_string(),
            detail,
            refresh_inventory,
            prepared_registration,
        ));
    }
}

impl PhaseThreadWorkspaceDeletionHost for WorkspaceDeletionHost {
    fn phase_thread_task_pending_for_workspace(&self, workspace_id: &BerylWorkspaceId) -> bool {
        self.owner.has_task_for_workspace(workspace_id)
    }

    fn take_deferred_phase_thread_outcomes_for_workspace(
        &mut self,
        workspace_id: &BerylWorkspaceId,
    ) -> Vec<DeferredPhaseThreadOutcome> {
        self.owner.take_deferred_outcomes(workspace_id)
    }

    fn publish_released_phase_thread_outcomes(
        &mut self,
        _: &BerylWorkspaceId,
        released: &shell::phase_thread_workspace_deletion::ReleasedPhaseThreadWorkspaceDeletionOutcomes,
    ) {
        self.published_known_children
            .extend(released.known_remaining_child_ids().iter().cloned());
        self.published_notice = released
            .final_notice()
            .map(|(title, detail)| (title.to_string(), detail.to_string()));
        self.published_refresh_inventory = released.refresh_inventory();
        self.publication_before_barrier = true;
    }

    fn capture_persistence_barrier_and_start_delete_worker(
        &mut self,
        _: &BerylWorkspaceId,
    ) -> Result<(), String> {
        self.deferred_outcomes_at_barrier = self.owner.deferred_notice_count();
        self.admission_available_at_barrier = self.owner.can_install_active();
        self.barrier_captured = true;
        if let Some(error) = self.worker_start_error.clone() {
            return Err(error);
        }
        self.worker_started = true;
        Ok(())
    }
}

impl PhaseThreadSourceQueueHost for SourceQueueHost {
    fn fail_source_pending_queue(&mut self, source_thread_id: &str, message: &str) -> bool {
        if !self.queue_present {
            return false;
        }
        self.queue_present = false;
        self.failures
            .push((source_thread_id.to_string(), message.to_string()));
        true
    }
}

impl PhaseThreadCompletionHost for CompletionHost {
    fn original_workspace_is_current(&self, _: &PhaseThreadPreparationRequest) -> bool {
        self.current
    }

    fn mark_inventory_refresh(&mut self) {
        self.refreshes += 1;
    }

    fn prepared_registration_validity(
        &self,
        _: &PhaseThreadPreparationRequest,
    ) -> Result<(), String> {
        self.validity_error.clone().map_or(Ok(()), Err)
    }

    fn register_prepared_child(
        &mut self,
        _: &PhaseThreadPreparationRequest,
        registration: &PreparedPhaseThreadRegistration,
        activate: bool,
    ) -> Result<(), String> {
        self.registrations.push((
            registration.child_thread_id().as_str().to_string(),
            activate,
        ));
        self.registration_error.clone().map_or(Ok(()), Err)
    }

    fn report_or_defer(
        &mut self,
        _: &PhaseThreadPreparationRequest,
        title: &'static str,
        detail: String,
        refresh_inventory: bool,
        prepared_registration: Option<PreparedPhaseThreadRegistration>,
    ) {
        self.reports
            .push((title.to_string(), detail, refresh_inventory));
        self.deferred_registrations.extend(prepared_registration);
    }
}

struct ProvenanceHost {
    current: bool,
    state: WorkspaceConversationState,
    persists: usize,
    reports: Vec<PreparedPhaseThreadRegistration>,
}

impl PhaseThreadCompletionHost for ProvenanceHost {
    fn original_workspace_is_current(&self, _: &PhaseThreadPreparationRequest) -> bool {
        self.current
    }

    fn mark_inventory_refresh(&mut self) {}

    fn prepared_registration_validity(
        &self,
        request: &PhaseThreadPreparationRequest,
    ) -> Result<(), String> {
        if !self.current {
            return Err("original workspace is not current".to_string());
        }
        let source = self
            .state
            .thread_registration(request.source_thread_id())
            .ok_or_else(|| "source registration missing".to_string())?;
        let root = self
            .state
            .thread_registration(request.orchestration_root_thread_id())
            .ok_or_else(|| "root registration missing".to_string())?;
        if source.rebind_required().is_some() || root.rebind_required().is_some() {
            return Err("source or root requires rebind".to_string());
        }
        if self
            .state
            .binding_for_execution_target(request.execution_target())
            .as_ref()
            != Some(request.member_binding())
        {
            return Err("frozen binding unavailable".to_string());
        }
        Ok(())
    }

    fn register_prepared_child(
        &mut self,
        request: &PhaseThreadPreparationRequest,
        registration: &PreparedPhaseThreadRegistration,
        activate: bool,
    ) -> Result<(), String> {
        let thread_id = registration.child_thread_id().clone();
        let thread = RegisteredConversationThread::new(
            thread_id.clone(),
            request.execution_target().clone(),
            "",
            None,
            registration.created_at_millis(),
            registration.updated_at_millis(),
        )
        .with_member_binding(request.member_binding().clone())
        .with_beryl_created();
        self.state.remember_thread(thread);
        self.state
            .record_thread_orchestration_root(&thread_id, request.orchestration_root_thread_id())
            .map_err(|error| error.to_string())?;
        if activate {
            self.state
                .activate_thread(&thread_id)
                .ok_or_else(|| "activation failed".to_string())?;
        }
        self.persists += 1;
        Ok(())
    }

    fn report_or_defer(
        &mut self,
        _: &PhaseThreadPreparationRequest,
        _: &'static str,
        _: String,
        _: bool,
        prepared_registration: Option<PreparedPhaseThreadRegistration>,
    ) {
        self.reports.extend(prepared_registration);
    }
}

fn explicit_root_request(
    generation: u64,
) -> (
    PhaseThreadPreparationRequest,
    WorkspaceConversationState,
    ConversationThreadMemberBinding,
) {
    let target = WorkspaceId::host_windows(r"C:\work\alpha");
    let mut state = WorkspaceConversationState::default();
    state.designate_primary_execution_target(&target).unwrap();
    let binding = state.binding_for_execution_target(&target).unwrap();
    let root_id = ConversationThreadId::new("root");
    state.remember_thread(
        RegisteredConversationThread::new(root_id.clone(), target, "", None, 1, 1)
            .with_member_binding(binding.clone())
            .with_beryl_created(),
    );
    state.record_thread_as_orchestration_root(&root_id).unwrap();
    let root = state.thread_registration(&root_id).unwrap();
    let request = PhaseThreadPreparationRequest::new_with_available_binding(
        PhaseThreadPreparationRequestParts {
            request_generation: generation,
            workspace_id: BerylWorkspaceId::new("alpha").unwrap(),
            source_thread_id: root_id.clone(),
            source_turn_id: ConversationTurnId::new("turn-source"),
            orchestration_root_thread_id: root_id.clone(),
            source_selection_thread_id: root_id,
        },
        root,
        root,
        Some(&binding),
    )
    .unwrap();
    (request, state, binding)
}

fn preparation_task(
    request: PhaseThreadPreparationRequest,
) -> (
    PhaseThreadPreparationTask,
    mpsc::Sender<PhaseThreadPreparationUpdate>,
) {
    let (sender, receiver) = mpsc::channel();
    (
        PhaseThreadPreparationTask::new(
            request,
            UserInputFragment {
                text: "Continue from the root doc/plan.md.".to_string(),
            },
            Arc::new(AtomicBool::new(false)),
            receiver,
        ),
        sender,
    )
}

fn guard_failure(
    expected: &PhaseThreadPreparationRequest,
    returned: &PhaseThreadPreparationRequest,
    current: &PhaseThreadCurrentIdentity,
) -> PhaseThreadResultGuardFailure {
    guard_phase_thread_preparation_result(expected, returned, current).unwrap_err()
}

#[test]
fn exact_result_guard_accepts_only_every_frozen_identity_dimension() {
    let request = request(7);
    let baseline = current(&request);
    assert_eq!(
        guard_phase_thread_preparation_result(&request, &request, &baseline),
        Ok(())
    );

    let mut changed = baseline.clone();
    changed.request_generation = 8;
    assert_eq!(
        guard_failure(&request, &request, &changed),
        PhaseThreadResultGuardFailure::RequestGeneration
    );
    let mut changed = baseline.clone();
    changed.workspace_id = BerylWorkspaceId::new("other").unwrap();
    assert_eq!(
        guard_failure(&request, &request, &changed),
        PhaseThreadResultGuardFailure::Workspace
    );
    let mut changed = baseline.clone();
    changed.source_thread_id = ConversationThreadId::new("other-source");
    assert_eq!(
        guard_failure(&request, &request, &changed),
        PhaseThreadResultGuardFailure::SourceThread
    );
    let mut changed = baseline.clone();
    changed.source_turn_id = ConversationTurnId::new("other-turn");
    assert_eq!(
        guard_failure(&request, &request, &changed),
        PhaseThreadResultGuardFailure::SourceTurn
    );
    let mut changed = baseline.clone();
    changed.orchestration_root_thread_id = ConversationThreadId::new("other-root");
    assert_eq!(
        guard_failure(&request, &request, &changed),
        PhaseThreadResultGuardFailure::OrchestrationRoot
    );
    let mut changed = baseline.clone();
    changed.source_selection_thread_id = ConversationThreadId::new("other-selection");
    assert_eq!(
        guard_failure(&request, &request, &changed),
        PhaseThreadResultGuardFailure::SourceSelection
    );
}

#[test]
fn exact_result_guard_rejects_target_cwd_binding_and_returned_request_changes() {
    let expected = request(7);
    let baseline = current(&expected);
    let mut changed = baseline.clone();
    changed.execution_target = WorkspaceId::host_windows(r"C:\work\other");
    assert_eq!(
        guard_failure(&expected, &expected, &changed),
        PhaseThreadResultGuardFailure::ExecutionTarget
    );
    let mut changed = baseline.clone();
    changed.canonical_cwd = PathBuf::from(r"C:\work\other");
    assert_eq!(
        guard_failure(&expected, &expected, &changed),
        PhaseThreadResultGuardFailure::CanonicalWorkingDirectory
    );
    let mut changed = baseline.clone();
    changed.member_binding =
        ConversationThreadMemberBinding::implicit_home(WorkspaceId::host_windows(r"C:\work\other"));
    assert_eq!(
        guard_failure(&expected, &expected, &changed),
        PhaseThreadResultGuardFailure::MemberBinding
    );
    let mut changed = baseline.clone();
    changed.source_rebind_required = true;
    assert_eq!(
        guard_failure(&expected, &expected, &changed),
        PhaseThreadResultGuardFailure::SourceRebindRequired
    );
    let mut changed = baseline.clone();
    changed.root_rebind_required = true;
    assert_eq!(
        guard_failure(&expected, &expected, &changed),
        PhaseThreadResultGuardFailure::RootRebindRequired
    );
    let mut changed = baseline.clone();
    changed.available_member_binding = None;
    assert_eq!(
        guard_failure(&expected, &expected, &changed),
        PhaseThreadResultGuardFailure::MemberBindingUnavailable
    );
    assert_eq!(
        guard_failure(&expected, &request(8), &baseline),
        PhaseThreadResultGuardFailure::UnexpectedRequest
    );
}

#[test]
fn production_workspace_state_guard_invalidates_only_binding_affecting_member_mutations() {
    let (explicit_request, mut state, _) = explicit_root_request(7);
    assert!(phase_thread_request_registrations_are_available(
        &explicit_request,
        &state,
        None,
    ));
    let unrelated = WorkspaceId::host_windows(r"C:\work\unrelated");
    state.attach_execution_target(&unrelated).unwrap();
    let unrelated_member = state
        .explicit_members()
        .iter()
        .find(|member| member.execution_target() == unrelated)
        .unwrap()
        .id()
        .clone();
    state
        .set_primary_explicit_member(&unrelated_member)
        .unwrap();
    assert!(phase_thread_request_registrations_are_available(
        &explicit_request,
        &state,
        None,
    ));
    state.detach_explicit_member(&unrelated_member).unwrap();
    assert!(phase_thread_request_registrations_are_available(
        &explicit_request,
        &state,
        None,
    ));
    let bound_member = state.explicit_members()[0].id().clone();
    state.detach_explicit_member(&bound_member).unwrap();
    assert!(!phase_thread_request_registrations_are_available(
        &explicit_request,
        &state,
        None,
    ));

    let implicit_request = request(9);
    let target = implicit_request.execution_target().clone();
    let root_id = implicit_request.orchestration_root_thread_id().clone();
    let mut implicit_state = WorkspaceConversationState::default();
    implicit_state
        .select_runtime(target.runtime_mode().clone())
        .unwrap();
    implicit_state.remember_thread(
        RegisteredConversationThread::new(root_id.clone(), target.clone(), "", None, 1, 1)
            .with_member_binding(implicit_request.member_binding().clone()),
    );
    implicit_state
        .record_thread_as_orchestration_root(&root_id)
        .unwrap();
    assert!(phase_thread_request_registrations_are_available(
        &implicit_request,
        &implicit_state,
        Some(&target),
    ));
    implicit_state.attach_execution_target(&target).unwrap();
    assert!(!phase_thread_request_registrations_are_available(
        &implicit_request,
        &implicit_state,
        Some(&target),
    ));
    assert!(
        implicit_state
            .thread_registration(&root_id)
            .unwrap()
            .rebind_required()
            .is_some()
    );
}

#[test]
fn dedicated_task_owns_cancellation_receiver_request_and_resume_fragment() {
    let request = request(7);
    let cancellation = Arc::new(AtomicBool::new(false));
    let (sender, receiver) = mpsc::channel();
    let mut task = PhaseThreadPreparationTask::new(
        request.clone(),
        UserInputFragment {
            text: "Continue from the root doc/plan.md.".to_string(),
        },
        cancellation.clone(),
        receiver,
    );
    assert_eq!(task.state(), PhaseThreadPreparationTaskState::Active);
    assert!(task.state().blocks_controls());
    assert_eq!(task.request(), &request);
    assert_eq!(
        task.resume_fragment().text,
        "Continue from the root doc/plan.md."
    );
    task.cancel(Instant::now() + Duration::from_secs(1));
    assert!(cancellation.load(Ordering::Acquire));
    assert_eq!(task.state(), PhaseThreadPreparationTaskState::Cancelling);
    assert!(!task.state().blocks_controls());

    let outcome = PhaseThreadPreparationOutcome {
        request,
        result: PhaseThreadPreparationResult::DefinitiveForkFailure {
            detail: "rejected".to_string(),
        },
    };
    sender
        .send(PhaseThreadPreparationUpdate::Finished(outcome.clone()))
        .unwrap();
    assert_eq!(task.try_recv().unwrap(), outcome);
}

#[test]
fn production_owner_polls_late_completion_disconnect_and_retention_expiry() {
    let mut owner = PhaseThreadTransitionOwner::default();
    let first_request = request(7);
    let (task, sender) = preparation_task(first_request.clone());
    owner.install_active(task).unwrap();
    assert!(owner.blocks_controls());
    let deadline = Instant::now() + Duration::from_secs(1);
    assert!(owner.cancel_active(deadline));
    assert!(!owner.blocks_controls());
    assert!(owner.has_poll_work());
    assert!(!owner.cancel_active(deadline));
    assert!(owner.has_poll_work());

    sender
        .send(PhaseThreadPreparationUpdate::Finished(
            PhaseThreadPreparationOutcome {
                request: first_request.clone(),
                result: PhaseThreadPreparationResult::CancelledBeforeFork,
            },
        ))
        .unwrap();
    let PhaseThreadPreparationPoll::Outcome { task, outcome } =
        owner.poll_next(Instant::now()).unwrap()
    else {
        panic!("expected retained cancellation outcome");
    };
    assert_eq!(task.state(), PhaseThreadPreparationTaskState::Cancelling);
    assert_eq!(outcome.request, first_request);

    let (task, sender) = preparation_task(request(8));
    owner.install_active(task).unwrap();
    drop(sender);
    assert!(matches!(
        owner.poll_next(Instant::now()),
        Some(PhaseThreadPreparationPoll::Disconnected { .. })
    ));

    let (task, _sender) = preparation_task(request(9));
    owner.install_active(task).unwrap();
    owner.cancel_active(Instant::now());
    assert!(matches!(
        owner.poll_next(Instant::now()),
        Some(PhaseThreadPreparationPoll::RetentionExpired { .. })
    ));
    assert!(!owner.has_poll_work());
}

#[test]
fn accepted_create_and_active_delete_invalidate_same_frame_results_but_unrelated_delete_does_not() {
    for outcome_ready_before_invalidation in [false, true] {
        for _replacement_worker_succeeds in [false, true] {
            for _replacement_kind in ["create", "active-delete"] {
                let mut owner = PhaseThreadTransitionOwner::default();
                let request = request(7);
                let (task, sender) = preparation_task(request.clone());
                owner.install_active(task).unwrap();
                let outcome = PhaseThreadPreparationOutcome {
                    request,
                    result: PhaseThreadPreparationResult::Prepared {
                        child: prepared_child("late-child"),
                        session_metadata: ThreadSessionMetadata::default(),
                    },
                };
                if outcome_ready_before_invalidation {
                    sender
                        .send(PhaseThreadPreparationUpdate::Finished(outcome.clone()))
                        .unwrap();
                }
                let generation = owner.generation();
                assert!(owner.invalidate_for_accepted_workspace_replacement(
                    true,
                    Instant::now() + Duration::from_secs(1),
                ));
                assert!(owner.generation() > generation);
                if !outcome_ready_before_invalidation {
                    sender
                        .send(PhaseThreadPreparationUpdate::Finished(outcome))
                        .unwrap();
                }
                let PhaseThreadPreparationPoll::Outcome { task, .. } =
                    owner.poll_next(Instant::now()).unwrap()
                else {
                    panic!("accepted replacement must retain the late outcome");
                };
                assert_eq!(task.state(), PhaseThreadPreparationTaskState::Cancelling);
            }
        }
    }

    let mut owner = PhaseThreadTransitionOwner::default();
    let (task, _sender) = preparation_task(request(7));
    owner.install_active(task).unwrap();
    let generation = owner.generation();
    assert!(!owner.invalidate_for_accepted_workspace_replacement(
        false,
        Instant::now() + Duration::from_secs(1),
    ));
    assert_eq!(owner.generation(), generation);
    assert!(owner.blocks_controls());
}

#[test]
fn active_workspace_deletion_waits_for_ready_late_result_before_worker_start() {
    for result_ready_before_delete in [false, true] {
        let request = request(7);
        let workspace_id = request.workspace_id().clone();
        let (task, sender) = preparation_task(request.clone());
        let mut host = WorkspaceDeletionHost {
            validity_error: Some("frozen member binding unavailable".to_string()),
            ..WorkspaceDeletionHost::default()
        };
        host.owner.install_active(task).unwrap();
        let outcome = PhaseThreadPreparationOutcome {
            request: request.clone(),
            result: PhaseThreadPreparationResult::Prepared {
                child: prepared_child("late-delete-child"),
                session_metadata: ThreadSessionMetadata::default(),
            },
        };
        if result_ready_before_delete {
            sender
                .send(PhaseThreadPreparationUpdate::Finished(outcome.clone()))
                .unwrap();
        }

        let generation = host.owner.generation();
        let mut deletion = PhaseThreadWorkspaceDeletionDrain::accept(
            workspace_id.clone(),
            &mut host.owner,
            Instant::now(),
        );
        assert!(host.owner.generation() > generation);
        assert!(host.owner.has_task_for_workspace(&workspace_id));
        assert_eq!(
            poll_phase_thread_workspace_deletion(&mut deletion, &mut host),
            PhaseThreadWorkspaceDeletionPoll::WaitingForPhaseThread
        );
        assert!(!host.barrier_captured);
        if !result_ready_before_delete {
            sender
                .send(PhaseThreadPreparationUpdate::Finished(outcome))
                .unwrap();
        }
        let PhaseThreadPreparationPoll::Outcome { task, outcome } =
            host.owner.poll_next(Instant::now()).unwrap()
        else {
            panic!("accepted deletion must retain its late prepared result");
        };
        assert_eq!(task.state(), PhaseThreadPreparationTaskState::Cancelling);
        assert!(!host.owner.has_task_for_workspace(&workspace_id));

        for index in 0..7 {
            host.owner.defer_outcome(DeferredPhaseThreadOutcome::new(
                request.clone(),
                "retained deletion truth".to_string(),
                format!("retained outcome {index}"),
                false,
                None,
            ));
        }
        let current_after_invalidation = PhaseThreadCurrentIdentity {
            request_generation: host.owner.generation(),
            ..current(&outcome.request)
        };
        let stale_reason = guard_phase_thread_preparation_result(
            task.request(),
            &outcome.request,
            &current_after_invalidation,
        )
        .err();
        assert_eq!(
            stale_reason,
            Some(PhaseThreadResultGuardFailure::RequestGeneration)
        );
        let decision = reduce_phase_thread_completion(
            task.state(),
            outcome.request == *task.request(),
            stale_reason.is_none(),
            true,
            PhaseThreadCompletionKind::Prepared,
        );
        let activation =
            apply_phase_thread_completion(&mut host, task, outcome.result, decision, stale_reason);
        assert!(activation.is_none());
        assert!(host.registrations.is_empty());
        assert_eq!(host.owner.deferred_notice_count(), 8);
        assert!(!host.owner.can_install_active());

        assert_eq!(
            poll_phase_thread_workspace_deletion(&mut deletion, &mut host),
            PhaseThreadWorkspaceDeletionPoll::WorkerStarted
        );
        assert!(host.publication_before_barrier);
        assert!(host.published_refresh_inventory);
        assert_eq!(host.refreshes, 1);
        assert!(host.barrier_captured);
        assert!(host.worker_started);
        assert_eq!(host.deferred_outcomes_at_barrier, 0);
        assert!(host.admission_available_at_barrier);
        assert!(host.owner.can_install_active());
        assert_eq!(
            host.published_known_children,
            vec![ConversationThreadId::new("late-delete-child")]
        );
        let (title, detail) = host.published_notice.as_ref().unwrap();
        assert_eq!(title, "Lifecycle phase child remains in backend");
        assert!(detail.contains("late-delete-child"));
        assert_eq!(
            poll_phase_thread_workspace_deletion(&mut deletion, &mut host),
            PhaseThreadWorkspaceDeletionPoll::WaitingForPhaseThread
        );
    }
}

#[test]
fn active_workspace_deletion_uses_bounded_indeterminate_drain_but_other_workspace_is_independent() {
    let active_request = request(7);
    let active_workspace_id = active_request.workspace_id().clone();
    let (task, _sender) = preparation_task(active_request);
    let mut active_host = WorkspaceDeletionHost::default();
    active_host.owner.install_active(task).unwrap();
    let mut active_deletion = PhaseThreadWorkspaceDeletionDrain::accept(
        active_workspace_id.clone(),
        &mut active_host.owner,
        Instant::now(),
    );
    assert_eq!(
        poll_phase_thread_workspace_deletion(&mut active_deletion, &mut active_host),
        PhaseThreadWorkspaceDeletionPoll::WaitingForPhaseThread
    );
    let unrelated_workspace_id = BerylWorkspaceId::new("unrelated").unwrap();
    let mut unrelated_deletion =
        PhaseThreadWorkspaceDeletionDrain::new(unrelated_workspace_id.clone());
    assert!(
        !active_host
            .owner
            .has_task_for_workspace(&unrelated_workspace_id)
    );
    let mut unrelated_host = WorkspaceDeletionHost::default();
    assert_eq!(
        poll_phase_thread_workspace_deletion(&mut unrelated_deletion, &mut unrelated_host),
        PhaseThreadWorkspaceDeletionPoll::WorkerStarted
    );

    assert!(matches!(
        active_host.owner.poll_next(Instant::now()),
        Some(PhaseThreadPreparationPoll::RetentionExpired { .. })
    ));
    assert_eq!(
        poll_phase_thread_workspace_deletion(&mut active_deletion, &mut active_host),
        PhaseThreadWorkspaceDeletionPoll::WorkerStarted
    );
}

#[test]
fn deletion_coordinator_releases_controls_after_worker_failure() {
    let workspace_id = BerylWorkspaceId::new("alpha").unwrap();
    let mut deletion = PhaseThreadWorkspaceDeletionDrain::new(workspace_id);
    let mut host = WorkspaceDeletionHost::default();
    assert_eq!(
        poll_phase_thread_workspace_deletion(&mut deletion, &mut host),
        PhaseThreadWorkspaceDeletionPoll::WorkerStarted
    );
    let mut owned_deletion = Some(deletion);
    assert!(complete_phase_thread_workspace_deletion(
        &mut owned_deletion
    ));
    assert!(owned_deletion.is_none());

    let mut deletion =
        PhaseThreadWorkspaceDeletionDrain::new(BerylWorkspaceId::new("beta").unwrap());
    let mut host = WorkspaceDeletionHost {
        worker_start_error: Some("worker unavailable".to_string()),
        ..WorkspaceDeletionHost::default()
    };
    for index in 0..8 {
        host.owner.defer_outcome(DeferredPhaseThreadOutcome::new(
            request_for_workspace(index + 1, "beta"),
            "retained worker-failure truth".to_string(),
            format!("retained outcome {index}"),
            false,
            None,
        ));
    }
    assert!(!host.owner.can_install_active());
    assert_eq!(
        poll_phase_thread_workspace_deletion(&mut deletion, &mut host),
        PhaseThreadWorkspaceDeletionPoll::WorkerStartFailed("worker unavailable".to_string())
    );
    assert!(host.publication_before_barrier);
    assert!(host.barrier_captured);
    assert!(!host.worker_started);
    assert_eq!(host.deferred_outcomes_at_barrier, 0);
    assert!(host.admission_available_at_barrier);
    assert!(host.owner.can_install_active());
    let mut owned_deletion = Some(deletion);
    assert!(complete_phase_thread_workspace_deletion(
        &mut owned_deletion
    ));
    assert!(owned_deletion.is_none());
}

#[test]
fn deletion_release_reports_exact_known_child_and_inventory_truth() {
    let child_id = ConversationThreadId::new("known-child-after-delete-drain");
    let mut host = WorkspaceDeletionHost {
        validity_error: Some("workspace binding unavailable".to_string()),
        ..WorkspaceDeletionHost::default()
    };
    host.owner.defer_outcome(DeferredPhaseThreadOutcome::new(
        request(7),
        "Lifecycle phase thread not activated".to_string(),
        "retained child".to_string(),
        true,
        Some(PreparedPhaseThreadRegistration::new(child_id.clone(), 1, 1)),
    ));
    let mut deletion =
        PhaseThreadWorkspaceDeletionDrain::new(BerylWorkspaceId::new("alpha").unwrap());
    assert_eq!(
        poll_phase_thread_workspace_deletion(&mut deletion, &mut host),
        PhaseThreadWorkspaceDeletionPoll::WorkerStarted
    );
    assert_eq!(host.published_known_children, vec![child_id]);
    let (title, detail) = host.published_notice.unwrap();
    assert_eq!(title, "Lifecycle phase child remains in backend");
    assert!(detail.contains("known-child-after-delete-drain"));
    assert!(detail.chars().count() <= 1_200);
}

#[test]
fn production_applicator_drives_registration_selection_and_exact_orphan_reporting() {
    let request = request(7);
    let (task, _sender) = preparation_task(request.clone());
    let decision = reduce_phase_thread_completion(
        PhaseThreadPreparationTaskState::Active,
        true,
        true,
        true,
        PhaseThreadCompletionKind::Prepared,
    );
    let mut host = CompletionHost {
        current: true,
        ..CompletionHost::default()
    };
    let activation = apply_phase_thread_completion(
        &mut host,
        task,
        PhaseThreadPreparationResult::Prepared {
            child: prepared_child("child-exact"),
            session_metadata: ThreadSessionMetadata::default(),
        },
        decision,
        None,
    )
    .expect("exact prepared result should select and persist through the host");
    assert_eq!(host.registrations, vec![("child-exact".to_string(), true)]);
    assert_eq!(activation.child.summary().id, "child-exact");
    assert!(host.reports.is_empty());

    let (task, _sender) = preparation_task(request);
    host.registration_error = Some("local registration rejected".repeat(200));
    let activation = apply_phase_thread_completion(
        &mut host,
        task,
        PhaseThreadPreparationResult::Prepared {
            child: prepared_child("child-known-orphan"),
            session_metadata: ThreadSessionMetadata::default(),
        },
        decision,
        None,
    );
    assert!(activation.is_none());
    assert_eq!(host.refreshes, 1);
    let (_, detail, refresh) = host.reports.last().unwrap();
    assert!(detail.contains("child-known-orphan"));
    assert!(detail.chars().count() <= 1_200);
    assert!(*refresh);
}

#[test]
fn typed_deferred_registration_restores_title_and_root_sibling_provenance_without_activation() {
    let (request, state, binding) = explicit_root_request(7);
    let (task, _sender) = preparation_task(request.clone());
    let decision = reduce_phase_thread_completion(
        PhaseThreadPreparationTaskState::Cancelling,
        true,
        false,
        false,
        PhaseThreadCompletionKind::Prepared,
    );
    let mut host = ProvenanceHost {
        current: false,
        state,
        persists: 0,
        reports: Vec::new(),
    };
    assert!(
        apply_phase_thread_completion(
            &mut host,
            task,
            PhaseThreadPreparationResult::Prepared {
                child: prepared_child("phase-child-one"),
                session_metadata: ThreadSessionMetadata::default(),
            },
            decision,
            Some(PhaseThreadResultGuardFailure::Workspace),
        )
        .is_none()
    );
    assert!(
        host.state
            .thread_registration(&ConversationThreadId::new("phase-child-one"))
            .is_none()
    );
    let registration = host.reports.pop().expect("typed registration retained");

    host.current = true;
    apply_deferred_prepared_registration(&mut host, &request, &registration).unwrap();
    let child_id = ConversationThreadId::new("phase-child-one");
    let child = host.state.thread_registration(&child_id).unwrap();
    assert!(child.beryl_created());
    assert_eq!(child.member_binding(), Some(&binding));
    assert_eq!(
        child.orchestration_root_thread_id(),
        Some(&ConversationThreadId::new("root"))
    );
    assert!(
        host.state
            .thread_automatic_title_generation_eligible(&child_id)
    );
    assert_eq!(host.state.active_thread(), None);
    assert_eq!(host.persists, 1);

    let root_id = ConversationThreadId::new("root");
    let root = host.state.thread_registration(&root_id).unwrap();
    let child = host.state.thread_registration(&child_id).unwrap();
    let sibling_request = PhaseThreadPreparationRequest::new_with_available_binding(
        PhaseThreadPreparationRequestParts {
            request_generation: 8,
            workspace_id: BerylWorkspaceId::new("alpha").unwrap(),
            source_thread_id: child_id.clone(),
            source_turn_id: ConversationTurnId::new("turn-child-one"),
            orchestration_root_thread_id: root_id.clone(),
            source_selection_thread_id: child_id,
        },
        child,
        root,
        Some(&binding),
    )
    .expect("the deferred child must remain a valid source for a later root sibling");
    assert_eq!(sibling_request.orchestration_root_thread_id(), &root_id);
}

#[test]
fn typed_deferred_registration_waits_for_exact_binding_availability() {
    let (request, state, _) = explicit_root_request(7);
    let member_id = state.explicit_members()[0].id().clone();
    let registration =
        shell::phase_thread_transition_applicator::prepared_phase_thread_registration(
            &prepared_child("phase-child-waiting"),
        );
    let mut host = ProvenanceHost {
        current: true,
        state,
        persists: 0,
        reports: Vec::new(),
    };
    host.state
        .mark_explicit_member_path_not_found(&member_id)
        .unwrap();
    assert!(apply_deferred_prepared_registration(&mut host, &request, &registration).is_err());
    assert!(
        host.state
            .thread_registration(registration.child_thread_id())
            .is_none()
    );
    assert!(
        host.state
            .thread_registration(request.orchestration_root_thread_id())
            .unwrap()
            .rebind_required()
            .is_some()
    );

    host.state
        .mark_explicit_member_available(&member_id)
        .unwrap();
    apply_deferred_prepared_registration(&mut host, &request, &registration).unwrap();
    assert!(
        host.state
            .thread_registration(registration.child_thread_id())
            .is_some()
    );
    assert_eq!(host.persists, 1);
}

#[test]
fn owner_bounds_cancelling_tasks_and_workspace_scoped_deferred_truth() {
    let mut owner = PhaseThreadTransitionOwner::default();
    let mut senders = Vec::new();
    for generation in 1..=8 {
        let request = request(generation);
        let (task, sender) = preparation_task(request.clone());
        owner.install_active(task).unwrap();
        owner.cancel_active(Instant::now() + Duration::from_secs(60));
        senders.push((sender, request));
    }
    assert!(!owner.can_install_active());
    assert!(owner.has_poll_work());

    let exact_large_child_id = format!("child-large-{}", "x".repeat(20_000));
    for (index, (sender, request)) in senders.into_iter().enumerate() {
        sender
            .send(PhaseThreadPreparationUpdate::Finished(
                PhaseThreadPreparationOutcome {
                    request: request.clone(),
                    result: PhaseThreadPreparationResult::CancelledBeforeFork,
                },
            ))
            .unwrap();
        assert!(matches!(
            owner.poll_next(Instant::now()),
            Some(PhaseThreadPreparationPoll::Outcome { .. })
        ));
        let workspace_id = if index % 2 == 0 {
            BerylWorkspaceId::new("alpha").unwrap()
        } else {
            BerylWorkspaceId::new("beta").unwrap()
        };
        let mut deferred_request = request;
        if workspace_id != deferred_request.workspace_id().clone() {
            deferred_request = request_for_workspace(deferred_request.request_generation(), "beta");
        }
        let prepared_registration = (index == 0).then(|| {
            PreparedPhaseThreadRegistration::new(
                ConversationThreadId::new(exact_large_child_id.clone()),
                1,
                1,
            )
        });
        owner.defer_outcome(DeferredPhaseThreadOutcome::new(
            deferred_request,
            "Lifecycle phase thread outcome unknown".to_string(),
            bounded_phase_thread_notice_detail(format!("child-{index}-{}", "x".repeat(2_000))),
            true,
            prepared_registration,
        ));
    }
    assert_eq!(owner.deferred_notice_count(), 8);
    let alpha = owner.take_deferred_outcomes(&BerylWorkspaceId::new("alpha").unwrap());
    assert_eq!(alpha.len(), 4);
    assert!(alpha.iter().all(|outcome| outcome.refresh_inventory()));
    assert!(
        alpha
            .iter()
            .all(|outcome| outcome.detail().chars().count() <= 1_200)
    );
    assert_eq!(
        alpha[0]
            .prepared_registration()
            .unwrap()
            .child_thread_id()
            .as_str(),
        exact_large_child_id
    );
    assert_eq!(owner.deferred_notice_count(), 4);
    owner.restore_deferred_outcomes(alpha);
    assert_eq!(owner.deferred_notice_count(), 8);
    assert!(
        owner
            .take_deferred_outcomes(&BerylWorkspaceId::new("other").unwrap())
            .is_empty()
    );
    let alpha = owner.take_deferred_outcomes(&BerylWorkspaceId::new("alpha").unwrap());
    assert_eq!(alpha.len(), 4);
    assert_eq!(
        alpha[0]
            .prepared_registration()
            .unwrap()
            .child_thread_id()
            .as_str(),
        exact_large_child_id
    );
    assert!(
        owner.can_install_active(),
        "releasing the deleted workspace's deferred truth must restore admission capacity"
    );
}

#[test]
fn source_queue_policy_fails_only_the_exact_accepted_source_queue() {
    let mut no_queue = SourceQueueHost::default();
    assert!(!fail_accepted_source_pending_input(&mut no_queue, "source"));
    assert!(no_queue.failures.is_empty());

    let mut queued_overflow = SourceQueueHost {
        queue_present: true,
        ..SourceQueueHost::default()
    };
    assert!(fail_accepted_source_pending_input(
        &mut queued_overflow,
        "source"
    ));
    assert!(!queued_overflow.queue_present);
    assert_eq!(queued_overflow.failures.len(), 1);
    assert_eq!(queued_overflow.failures[0].0, "source");
    assert!(queued_overflow.failures[0].1.contains("not delivered"));
}

#[test]
fn shutdown_cancel_all_boundedly_drains_active_and_retained_tasks() {
    let mut owner = PhaseThreadTransitionOwner::default();
    let mut senders = Vec::new();
    let (task, sender) = preparation_task(request(1));
    owner.install_active(task).unwrap();
    owner.cancel_active(Instant::now() + Duration::from_secs(60));
    senders.push(sender);
    let (task, sender) = preparation_task(request(2));
    owner.install_active(task).unwrap();
    senders.push(sender);

    let mut deletion = Some(PhaseThreadWorkspaceDeletionDrain::accept(
        BerylWorkspaceId::new("alpha").unwrap(),
        &mut owner,
        Instant::now() + Duration::from_secs(60),
    ));
    assert!(complete_phase_thread_workspace_deletion(&mut deletion));
    assert!(deletion.is_none());
    let generation_before_shutdown = owner.generation();
    owner.cancel_all(Instant::now());
    assert!(owner.generation() > generation_before_shutdown);
    assert!(!owner.blocks_controls());
    let mut expired = 0;
    while let Some(completion) = owner.poll_next(Instant::now()) {
        assert!(matches!(
            completion,
            PhaseThreadPreparationPoll::RetentionExpired { .. }
        ));
        expired += 1;
    }
    assert_eq!(expired, 2);
    assert!(!owner.has_poll_work());
    drop(senders);
}

#[test]
fn shutdown_late_prepared_outcome_registers_without_selection_or_start() {
    let mut owner = PhaseThreadTransitionOwner::default();
    let request = request(7);
    let (task, sender) = preparation_task(request.clone());
    owner.install_active(task).unwrap();
    owner.cancel_all(Instant::now() + Duration::from_secs(1));
    sender
        .send(PhaseThreadPreparationUpdate::Finished(
            PhaseThreadPreparationOutcome {
                request: request.clone(),
                result: PhaseThreadPreparationResult::Prepared {
                    child: prepared_child("shutdown-child"),
                    session_metadata: ThreadSessionMetadata::default(),
                },
            },
        ))
        .unwrap();
    let PhaseThreadPreparationPoll::Outcome { task, outcome } =
        owner.poll_next(Instant::now()).unwrap()
    else {
        panic!("expected definitive shutdown outcome");
    };
    let decision = reduce_phase_thread_completion(
        task.state(),
        outcome.request == *task.request(),
        false,
        true,
        PhaseThreadCompletionKind::Prepared,
    );
    let mut host = CompletionHost {
        current: true,
        ..CompletionHost::default()
    };
    let activation = apply_phase_thread_completion(
        &mut host,
        task,
        outcome.result,
        decision,
        Some(PhaseThreadResultGuardFailure::RequestGeneration),
    );
    assert!(activation.is_none());
    assert_eq!(
        host.registrations,
        vec![("shutdown-child".to_string(), false)]
    );
    assert_eq!(host.refreshes, 1);
    assert_eq!(host.reports.len(), 1);
}

#[test]
fn exact_active_prepared_result_is_the_only_activation_decision() {
    let exact = reduce_phase_thread_completion(
        PhaseThreadPreparationTaskState::Active,
        true,
        true,
        true,
        PhaseThreadCompletionKind::Prepared,
    );
    assert!(exact.activate_prepared);
    assert!(!exact.register_prepared);
    assert!(!exact.refresh_original_workspace);
    assert_eq!(exact.notice, PhaseThreadCompletionNotice::None);

    for (returned_request_matches, current_identity_matches) in
        [(false, true), (true, false), (false, false)]
    {
        let stale = reduce_phase_thread_completion(
            PhaseThreadPreparationTaskState::Active,
            returned_request_matches,
            current_identity_matches,
            true,
            PhaseThreadCompletionKind::Prepared,
        );
        assert!(!stale.activate_prepared);
        assert_eq!(stale.register_prepared, returned_request_matches);
        assert!(stale.refresh_original_workspace);
        assert_eq!(
            stale.notice,
            PhaseThreadCompletionNotice::PreparedNotActivated
        );
    }
}

#[test]
fn cancellation_race_retains_late_prepared_child_without_activation() {
    let decision = reduce_phase_thread_completion(
        PhaseThreadPreparationTaskState::Cancelling,
        true,
        true,
        true,
        PhaseThreadCompletionKind::Prepared,
    );
    assert!(!decision.activate_prepared);
    assert!(decision.register_prepared);
    assert!(decision.refresh_original_workspace);
    assert_eq!(
        decision.notice,
        PhaseThreadCompletionNotice::PreparedNotActivated
    );

    for kind in [
        PhaseThreadCompletionKind::CancelledBeforeFork,
        PhaseThreadCompletionKind::DefinitiveForkFailure,
        PhaseThreadCompletionKind::KnownChildCleaned,
    ] {
        let retired = reduce_phase_thread_completion(
            PhaseThreadPreparationTaskState::Cancelling,
            true,
            false,
            true,
            kind,
        );
        assert!(!retired.activate_prepared);
        assert!(!retired.register_prepared);
        assert!(!retired.refresh_original_workspace);
        assert_eq!(retired.notice, PhaseThreadCompletionNotice::None);
    }
}

#[test]
fn known_orphan_and_disconnect_retain_refresh_for_the_original_workspace() {
    for original_workspace_is_current in [true, false] {
        let orphan = reduce_phase_thread_completion(
            PhaseThreadPreparationTaskState::Cancelling,
            true,
            false,
            original_workspace_is_current,
            PhaseThreadCompletionKind::KnownChildRemaining,
        );
        assert!(!orphan.activate_prepared);
        assert!(!orphan.register_prepared);
        assert!(orphan.refresh_original_workspace);
        assert_eq!(orphan.notice, PhaseThreadCompletionNotice::KnownOrphan);

        let disconnected = reduce_phase_thread_disconnect(original_workspace_is_current);
        assert!(!disconnected.activate_prepared);
        assert!(disconnected.refresh_original_workspace);
        assert_eq!(
            disconnected.notice,
            PhaseThreadCompletionNotice::Indeterminate
        );
    }
}

#[test]
fn indeterminate_and_continuation_start_failure_preserve_selection_truth() {
    let indeterminate = reduce_phase_thread_completion(
        PhaseThreadPreparationTaskState::Active,
        true,
        true,
        true,
        PhaseThreadCompletionKind::IndeterminateFork,
    );
    assert!(!indeterminate.activate_prepared);
    assert!(indeterminate.refresh_original_workspace);
    assert_eq!(
        indeterminate.notice,
        PhaseThreadCompletionNotice::Indeterminate
    );
    assert_eq!(
        reduce_phase_thread_continuation_start(false),
        PhaseThreadContinuationStartDecision::LeaveSelectedIdle
    );
    assert_eq!(
        reduce_phase_thread_continuation_start(true),
        PhaseThreadContinuationStartDecision::Started
    );
}

#[test]
fn preparation_notice_detail_is_bounded() {
    let detail = "x".repeat(2_000);
    let bounded = bounded_phase_thread_notice_detail(detail);
    assert_eq!(bounded.chars().count(), 1_200);
    assert!(bounded.ends_with('…'));
}
