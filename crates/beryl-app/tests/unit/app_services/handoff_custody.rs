use super::*;
use crate::{
    discussion_settlement::{
        DiscussionParentInputRequest, DiscussionParentNondispatch, DiscussionSettlementOutcome,
        DiscussionSettlementResult, DiscussionSettlementService,
    },
    support,
};
use beryl_home_store::{CommandOutcome, HomeCommand};
use beryl_model::{JobId, ResolutionIntentId, SyndicItemId, SyndicTurnId};
use beryl_state::{
    AdmitBranchHandoffJob, BranchHandoffJobAdmission, BranchHandoffJobLifecycle,
    HandoffFailureKind, ParentHandoffIdentity, ParentQueueOrdinal, ResolutionAttemptOrdinal,
    ResolutionRequestIdentity, ResolutionText,
};
use syndic_storage::{
    BindingPublicationStatus, CancelBindingActivation, DiscussionHandoffMutation,
    SyndicPointReadLimit,
};

#[path = "handoff_custody/support.rs"]
mod seed;

fn limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(400_000).unwrap()
}

fn prepare_parent(owner: &ProcessServiceOwner) -> (JobId, ParentHandoffIdentity) {
    let graph = owner.graph().unwrap();
    let home = graph.home();
    let state = graph.state();
    let syndic = graph.syndic();
    seed::parent_and_child(home, syndic);
    let mut request = support::discussion_handoff::active_request(
        home,
        syndic,
        ResolutionIntentId::from_bytes([210; 16]),
        JobId::from_bytes([211; 16]),
    );
    let admission = BranchHandoffJobAdmission::new(
        request.intent_id,
        ResolutionAttemptOrdinal::FIRST,
        request.thread_id,
        request.parent.thread_id,
        request.context_owner,
        request.context_digest,
        request.resolving_target.pending().active_turn_id(),
        ResolutionRequestIdentity::new(
            request.resolving_target.pending().cas_thread_id().clone(),
            request.resolving_target.cas_turn_id().clone(),
            beryl_model::DynamicToolCallId::new("resolve").unwrap(),
        ),
        ParentQueueOrdinal::new(request.parent.accepted_high_water),
        ResolutionText::new("Resolution result").unwrap(),
    );
    let job = admission.job_id();
    request.job_id = job;
    let handoff = syndic
        .prepare_discussion_handoff(home, DiscussionHandoffMutation::Admit(request))
        .unwrap();
    let mut command = HomeCommand::new(home.home_revision().unwrap());
    command.add(handoff.contribution()).unwrap();
    command
        .add(state.durable_jobs().admit_branch_handoff(
            state.durable_jobs().revision(home).unwrap(),
            AdmitBranchHandoffJob::new(admission),
        ))
        .unwrap();
    assert!(matches!(
        home.execute(command),
        CommandOutcome::Committed {
            later_failure: None,
            local_finalization: None,
            ..
        }
    ));
    let service = DiscussionSettlementService::new(
        owner.settlements.clone(),
        home.service_reference(),
        state.clone(),
        syndic.clone(),
    );
    support::discussion_handoff::complete_resolving_turn(home, syndic);
    assert!(matches!(
        service
            .prepare(job, CommandCancellation::new())
            .unwrap()
            .unwrap()
            .execute(),
        DiscussionSettlementOutcome::Committed {
            result: DiscussionSettlementResult::ReadyForParent,
            later_failure: None,
            ..
        }
    ));
    let parent_request = DiscussionParentInputRequest {
        turn_id: SyndicTurnId::from_bytes([240; 16]),
        item_id: SyndicItemId::from_bytes([241; 16]),
        admitted_at: support::timestamp(500),
    };
    let parent = ParentHandoffIdentity::new(
        beryl_model::SyndicAcceptedInputId::from_bytes(*job.as_bytes()),
        parent_request.turn_id,
    );
    assert!(matches!(
        service
            .prepare_parent_input(job, parent_request, CommandCancellation::new())
            .unwrap()
            .unwrap()
            .execute(),
        DiscussionSettlementOutcome::Committed {
            result: DiscussionSettlementResult::StartingParent(actual),
            later_failure: None,
            ..
        } if actual == parent
    ));
    (job, parent)
}

fn retain_activated_parent(
    owner: &ProcessServiceOwner,
) -> (JobId, ParentHandoffIdentity, CancelBindingActivation) {
    let (job, parent) = prepare_parent(owner);
    let graph = owner.graph().unwrap();
    let home = graph.home();
    let syndic = graph.syndic();
    let service = DiscussionSettlementService::new(
        owner.settlements.clone(),
        home.service_reference(),
        graph.state().clone(),
        syndic.clone(),
    );
    let reservation = service
        .reserve_parent_dispatch(job, support::id(30), parent.turn_id())
        .unwrap();
    let (_, snapshot) = support::exact_cas::activate_turn(
        home,
        syndic.clone(),
        support::id(30),
        parent.turn_id(),
        support::timestamp(501),
    );
    let binding = syndic
        .current_binding(home, support::id(30), limit())
        .unwrap()
        .unwrap();
    let gate = syndic
        .input_gate(home, support::id(30), limit())
        .unwrap()
        .unwrap();
    let turn = syndic
        .turn_state(home, parent.turn_id(), limit())
        .unwrap()
        .unwrap();
    let activation = CancelBindingActivation::new(
        support::id(30),
        binding.binding().revision(),
        gate.revision(),
        turn.revision(),
        binding.binding().selected_path(),
        snapshot,
        parent.turn_id(),
    );
    let pending = reservation
        .nondispatched(DiscussionParentNondispatch::for_test(
            activation.clone(),
            HandoffFailureKind::CasRejectedBeforeAcceptance,
        ))
        .unwrap();
    drop(pending);
    assert_eq!(owner.settlements.pending_nondispatch_count(), 1);
    assert!(owner.settlements.retained_audit(job).is_none());
    (job, parent, activation)
}

#[test]
fn shutdown_preserves_original_preparation_failure_proof_in_complete_graph() {
    let (directory, mut owner, faults) = recovery_support::installed();
    owner
        .graph_mut()
        .unwrap()
        .handoff
        .as_mut()
        .unwrap()
        .shutdown()
        .unwrap();
    let (job_id, parent) = prepare_parent(&owner);
    let graph = owner.graph().unwrap();
    let original_graph = std::ptr::from_ref(graph);
    let expected = graph.home().health().generation().unwrap();
    let original_job = graph
        .state()
        .durable_jobs()
        .job(graph.home(), job_id)
        .unwrap()
        .unwrap();
    let original_input = graph
        .syndic()
        .accepted_input(graph.home(), parent.accepted_input_id(), limit())
        .unwrap();
    let original_pending = graph
        .syndic()
        .pending_dispatch_evidence(graph.home(), support::id(30), limit())
        .unwrap()
        .unwrap();
    let service = DiscussionSettlementService::new(
        owner.settlements.clone(),
        graph.home().service_reference(),
        graph.state().clone(),
        graph.syndic().clone(),
    );
    let proof = service
        .reserve_parent_dispatch(job_id, support::id(30), parent.turn_id())
        .unwrap()
        .preparation_failed_for_test(HandoffFailureKind::RuntimeUnavailable)
        .unwrap();
    drop(proof);
    drop(service);
    assert_eq!(owner.settlements.pending_nondispatch_count(), 1);
    assert_eq!(owner.enrollments.pending_count(), 0);
    owner.begin_shutdown().unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    let result = loop {
        let result = owner.poll_shutdown(&ProjectionCancellationToken::new());
        if matches!(result, Ok(AppServiceShutdownProgress::Waiting)) {
            assert!(
                Instant::now() < deadline,
                "shutdown did not classify retained custody"
            );
            std::thread::sleep(Duration::from_millis(10));
        } else {
            break result;
        }
    };
    assert!(
        matches!(
            result,
            Err(AppServiceCloseError::PendingCustody {
                enrollments: 0,
                nondispatch: 1
            })
        ),
        "shutdown discarded or misclassified the retained preparation failure: {result:?}"
    );
    assert_eq!(std::ptr::from_ref(owner.graph().unwrap()), original_graph);
    assert_eq!(owner.settlements.pending_nondispatch_count(), 1);
    let graph = owner.graph().unwrap();
    assert_eq!(
        graph
            .state()
            .durable_jobs()
            .job(graph.home(), job_id)
            .unwrap(),
        Some(original_job)
    );
    assert_eq!(
        graph
            .syndic()
            .accepted_input(graph.home(), parent.accepted_input_id(), limit())
            .unwrap(),
        original_input
    );
    assert_eq!(
        graph
            .syndic()
            .pending_dispatch_evidence(graph.home(), support::id(30), limit())
            .unwrap(),
        Some(original_pending)
    );
    assert!(graph.home().pending_reconciliations().is_empty());
    recovery_support::fail(&owner, &faults);
    owner.retire_failed_service_graph(expected).unwrap();
    assert_eq!(owner.settlements.pending_nondispatch_count(), 1);
    let mut candidate = owner.recover_retired_service_home(expected).unwrap();
    let state = BerylState::reacquire_candidate(&candidate).unwrap();
    let syndic = SyndicStorage::reacquire_candidate(&candidate).unwrap();
    let access = candidate.recovery_access().unwrap();
    owner
        .settle_retired_process_work(&access, &state, &syndic, &CommandCancellation::new())
        .unwrap();
    assert_eq!(owner.settlements.pending_nondispatch_count(), 0);
    let paused = state
        .durable_jobs()
        .job_candidate(&access, job_id)
        .unwrap()
        .unwrap();
    assert_eq!(
        paused.lifecycle(),
        BranchHandoffJobLifecycle::RetryableFailed
    );
    assert_eq!(paused.state().parent(), Some(parent));
    let pending = syndic
        .pending_dispatch_evidence_candidate(&access, support::id(30), limit())
        .unwrap()
        .unwrap();
    assert_eq!(pending.turn_id(), original_pending.turn_id());
    assert_eq!(pending.input(), original_pending.input());
    assert_eq!(
        pending.dispatch_provenance(),
        original_pending.dispatch_provenance()
    );
    assert!(access.pending_reconciliations().is_empty());
    candidate.abort().close().unwrap();
    assert_reopens(&directory);
    directory.close().unwrap();
}

#[test]
fn fresh_candidate_prefix_settles_original_activated_parent_before_replacement_graph_runs() {
    let (directory, mut owner, faults) = recovery_support::installed();
    owner
        .graph_mut()
        .unwrap()
        .handoff
        .as_mut()
        .unwrap()
        .shutdown()
        .unwrap();
    let (job_id, parent, activation) = retain_activated_parent(&owner);
    let graph = owner.graph().unwrap();
    let expected = graph.home().health().generation().unwrap();
    let original_job = graph
        .state()
        .durable_jobs()
        .job(graph.home(), job_id)
        .unwrap()
        .unwrap();
    let original_input = graph
        .syndic()
        .accepted_input(graph.home(), parent.accepted_input_id(), limit())
        .unwrap();
    assert!(original_input.is_some());
    let original_parent = graph
        .syndic()
        .thread(graph.home(), support::id(30), limit())
        .unwrap();
    assert_eq!(owner.enrollments.pending_count(), 0);
    recovery_support::fail(&owner, &faults);
    owner.retire_failed_service_graph(expected).unwrap();
    assert!(owner.graph().is_none());
    assert_eq!(owner.settlements.pending_nondispatch_count(), 1);
    let mut candidate = Some(owner.recover_retired_service_home(expected).unwrap());
    let generation = candidate.as_ref().unwrap().generation();
    assert_ne!(generation, expected);
    let error = match owner.prepare_recovery_service_graph(
        expected,
        &mut candidate,
        configuration(),
        support::timestamp(600),
        &CommandCancellation::new(),
    ) {
        Err(error) => error,
        Ok(_) => panic!("recovery graph preparation bypassed the original nondispatch proof"),
    };
    let crate::app_services::recovery_graph::RecoveryServicePreparationError::Refused(reason) =
        error
    else {
        panic!("pending nondispatch custody must refuse graph preparation before CAS construction");
    };
    assert_eq!(
        reason,
        AppServiceCloseError::PendingCustody {
            enrollments: 0,
            nondispatch: 1
        }
        .to_string()
    );
    assert!(candidate.is_some());
    assert!(owner.graph().is_none());
    assert_eq!(owner.settlements.pending_nondispatch_count(), 1);

    let retained = candidate.as_mut().unwrap();
    let state = BerylState::reacquire_candidate(retained).unwrap();
    let syndic = SyndicStorage::reacquire_candidate(retained).unwrap();
    let access = retained.recovery_access().unwrap();
    let cancelled = CommandCancellation::new();
    cancelled.cancel();
    assert!(matches!(
        owner.settle_retired_process_work(&access, &state, &syndic, &cancelled),
        Err(crate::app_services::recovery_retirement::RetiredProcessWorkError::Cancelled)
    ));
    assert_eq!(owner.settlements.pending_nondispatch_count(), 1);
    assert_eq!(
        state.durable_jobs().job_candidate(&access, job_id).unwrap(),
        Some(original_job.clone())
    );
    assert_eq!(
        syndic
            .cancelled_binding_activation_status_candidate(&access, &activation, limit())
            .unwrap(),
        BindingPublicationStatus::Prior
    );
    owner
        .settle_retired_process_work(&access, &state, &syndic, &CommandCancellation::new())
        .unwrap();
    assert_eq!(owner.settlements.pending_nondispatch_count(), 0);
    assert_eq!(owner.enrollments.pending_count(), 0);
    assert!(access.pending_reconciliations().is_empty());
    let paused = state
        .durable_jobs()
        .job_candidate(&access, job_id)
        .unwrap()
        .unwrap();
    assert_eq!(
        paused.lifecycle(),
        BranchHandoffJobLifecycle::RetryableFailed
    );
    assert_eq!(paused.state().parent(), Some(parent));
    assert_eq!(paused.resolution(), original_job.resolution());
    assert_eq!(
        syndic
            .cancelled_binding_activation_status_candidate(&access, &activation, limit())
            .unwrap(),
        BindingPublicationStatus::Exact
    );
    assert_eq!(
        syndic
            .thread_candidate(&access, support::id(30), limit())
            .unwrap(),
        original_parent
    );
    assert_eq!(
        syndic
            .pending_dispatch_evidence_candidate(&access, support::id(30), limit())
            .unwrap()
            .unwrap()
            .turn_id(),
        parent.turn_id()
    );

    let mut prepared = Some(
        owner
            .prepare_recovery_service_graph(
                expected,
                &mut candidate,
                configuration(),
                support::timestamp(601),
                &CommandCancellation::new(),
            )
            .unwrap(),
    );
    assert!(candidate.is_none());
    let start = owner
        .publish_recovery_service_graph(
            expected,
            generation,
            &mut prepared,
            &CommandCancellation::new(),
        )
        .unwrap();
    assert!(prepared.is_none());
    assert_eq!(
        owner
            .graph()
            .unwrap()
            .handoff
            .as_ref()
            .unwrap()
            .test_completed_passes(),
        0
    );
    owner.reopen_recovery_admission(generation).unwrap();
    assert!(owner.process.execution_permit().reserve().is_ok());
    assert!(start.release());
    let deadline = Instant::now() + Duration::from_secs(5);
    while owner
        .graph()
        .unwrap()
        .handoff
        .as_ref()
        .unwrap()
        .test_completed_passes()
        == 0
    {
        assert!(
            Instant::now() < deadline,
            "fresh handoff scan did not complete"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    owner
        .graph_mut()
        .unwrap()
        .handoff
        .as_mut()
        .unwrap()
        .shutdown()
        .unwrap();
    let graph = owner.graph().unwrap();
    assert_eq!(graph.home().health().generation(), Some(generation));
    assert_eq!(
        graph
            .state()
            .durable_jobs()
            .job(graph.home(), job_id)
            .unwrap(),
        Some(paused)
    );
    assert_eq!(
        graph
            .syndic()
            .accepted_input(graph.home(), parent.accepted_input_id(), limit())
            .unwrap(),
        original_input
    );
    assert_eq!(
        graph
            .syndic()
            .thread(graph.home(), support::id(30), limit())
            .unwrap(),
        original_parent
    );
    assert_eq!(
        graph
            .syndic()
            .pending_dispatch_evidence(graph.home(), support::id(30), limit())
            .unwrap()
            .unwrap()
            .turn_id(),
        parent.turn_id()
    );
    assert_eq!(owner.settlements.pending_nondispatch_count(), 0);
    recovery_support::fail(&owner, &faults);
    owner.retire_failed_service_graph(generation).unwrap();
    owner
        .take_retired_service_home(generation)
        .unwrap()
        .close()
        .unwrap();
    assert_reopens(&directory);
    directory.close().unwrap();
}
