#![cfg(feature = "test-faults")]
#[path = "discussion_resolution_admission/eligibility.rs"]
mod eligibility;
#[path = "discussion_resolution_admission/process_tools.rs"]
mod process_tools;
#[path = "discussion_resolution_admission/recovery.rs"]
mod recovery;
#[path = "../../syndic-storage/tests/support/mod.rs"]
mod support;

use beryl_app::{
    cas_projection::*, discussion_settlement::*, process_admission::ProcessAdmissionGate,
};
use beryl_home_store::{
    CommandCancellation, HomeOpenCandidate, HomeOpenOptions, HomeSchemaVersion,
    test_faults::{FaultController, FaultPoint},
};
use beryl_model::{DynamicToolCallId, JobId, ResolutionIntentId};
use beryl_state::{BerylState, ResolutionRequestIdentity, ResolutionText};
use std::num::NonZeroUsize;
use support::id;
use syndic_storage::*;

struct IdleProvider;
impl ScheduledOrdinaryExecutionProvider for IdleProvider {
    fn try_issue(
        &mut self,
        admission: ScheduledOrdinaryAdmission,
    ) -> Result<ScheduledOrdinaryAdmissionResult, ScheduledOrdinaryAdmissionError> {
        Ok(admission.decline(ScheduledOrdinaryExecutionUnavailable::RuntimeNotReady))
    }
    fn shutdown(&mut self) {}
}

struct Fixture {
    directory: tempfile::TempDir,
    service: ProjectionConnectionService,
    settlement: DiscussionSettlementService,
    operations: DiscussionSettlementOperations,
    state: BerylState,
    syndic: SyndicStorage,
    faults: FaultController,
    source: AdmitDiscussionHandoff,
}

impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let faults = FaultController::new();
        let mut candidate = HomeOpenCandidate::open_with_faults(
            HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
            faults.clone(),
        )
        .unwrap();
        let state = BerylState::register(&mut candidate).unwrap();
        let syndic = SyndicStorage::register(&mut candidate).unwrap();
        let store = candidate
            .prepare_publication(
                BerylState::required_domains()
                    .unwrap()
                    .merge(SyndicStorage::required_domains().unwrap())
                    .unwrap(),
            )
            .unwrap()
            .publish()
            .unwrap();
        let process = ProcessAdmissionGate::new();
        let operations =
            DiscussionSettlementOperations::new(process.clone(), NonZeroUsize::new(2).unwrap());
        let settlement = DiscussionSettlementService::new(
            operations.clone(),
            store.service_reference(),
            state.clone(),
            syndic.clone(),
        );
        let service = ProjectionConnectionService::new(
            process,
            store,
            syndic.clone(),
            ProjectionServiceConfig::try_new(8, 4, MinimumTurnCaptureReserve::try_new(1).unwrap())
                .unwrap(),
            Box::new(IdleProvider),
        )
        .unwrap();
        let command = service.live_home_command().unwrap();
        support::seed_populated(command.home(), syndic.clone());
        let source = support::discussion_handoff::active_request(
            command.home(),
            &syndic,
            ResolutionIntentId::from_bytes([210; 16]),
            JobId::from_bytes([210; 16]),
        );
        drop(command);
        Self {
            directory,
            service,
            settlement,
            operations,
            state,
            syndic,
            faults,
            source,
        }
    }
    fn context(&self, call: &str) -> BranchDiscussionResolutionContext {
        BranchDiscussionResolutionContext::for_test(
            &self.service,
            self.source.thread_id,
            self.source.resolving_target.pending().active_turn_id(),
            ResolutionRequestIdentity::new(
                self.source
                    .resolving_target
                    .pending()
                    .cas_thread_id()
                    .clone(),
                self.source.resolving_target.cas_turn_id().clone(),
                DynamicToolCallId::new(call).unwrap(),
            ),
        )
    }
    fn admit(
        &self,
        call: &str,
        intent: u8,
        text: &str,
    ) -> Result<DiscussionResolutionOutcome, DiscussionSettlementError> {
        self.service
            .admit_discussion_resolution(
                &self.settlement,
                &self.context(call),
                ResolutionIntentId::from_bytes([intent; 16]),
                ResolutionText::new(text).unwrap(),
                CommandCancellation::new(),
            )
            .map(|outcome| outcome.publish(|outcome| outcome))
    }
    fn close(self) {
        let Self {
            directory,
            service,
            settlement,
            operations,
            state,
            syndic,
            faults,
            source,
        } = self;
        drop((settlement, operations, state, syndic, faults, source));
        service.close().unwrap();
        drop(directory);
    }
}

fn admitted(outcome: DiscussionResolutionOutcome) -> JobId {
    match outcome {
        DiscussionResolutionOutcome::Command(DiscussionSettlementOutcome::Committed {
            result: DiscussionSettlementResult::ResolutionAdmitted(job),
            later_failure: None,
            local_finalization: None,
            ..
        }) => job,
        _ => panic!("expected clean atomic admission"),
    }
}

#[test]
fn exact_request_is_idempotent_and_different_request_preserves_original_payload() {
    let fixture = Fixture::new();
    let text = "🙂".repeat(65_536);
    let job = admitted(fixture.admit("resolve", 210, &text).unwrap());
    assert!(
        matches!(fixture.admit("resolve", 211, "changed").unwrap(), DiscussionResolutionOutcome::Existing(id) if id == job)
    );
    assert!(
        matches!(fixture.admit("different", 212, "changed").unwrap(), DiscussionResolutionOutcome::AlreadyAdmitted(id) if id == job)
    );
    let command = fixture.service.live_home_command().unwrap();
    let stored = fixture
        .state
        .durable_jobs()
        .job(command.home(), job)
        .unwrap()
        .unwrap();
    assert_eq!(stored.resolution().as_str(), text);
    assert_eq!(
        stored.request(),
        fixture.context("resolve").request_identity()
    );
    let gate = fixture
        .syndic
        .discussion_handoff_gate(
            command.home(),
            id(36),
            SyndicPointReadLimit::new(256).unwrap(),
        )
        .unwrap()
        .unwrap();
    assert_eq!(
        gate.state(),
        DiscussionHandoffGateState::Pending {
            intent_id: stored.intent_id(),
            job_id: job,
            resolving_turn_id: stored.resolving_turn_id()
        }
    );
    drop(command);
    fixture.close();
}

#[test]
fn queued_input_defers_without_job_and_cancelled_admission_writes_nothing() {
    let fixture = Fixture::new();
    let cancellation = CommandCancellation::new();
    cancellation.cancel();
    assert!(matches!(
        fixture.service.admit_discussion_resolution(
            &fixture.settlement,
            &fixture.context("cancel"),
            ResolutionIntentId::from_bytes([213; 16]),
            ResolutionText::new("cancel").unwrap(),
            cancellation
        ),
        Err(DiscussionSettlementError::Cancelled)
    ));
    let command = fixture.service.live_home_command().unwrap();
    queue_future_input(
        command.home(),
        &fixture.syndic,
        fixture.source.resolving_target.pending().active_turn_id(),
    );
    drop(command);
    assert!(matches!(
        fixture.admit("defer", 210, "later").unwrap(),
        DiscussionResolutionOutcome::DeferredQueuedInput
    ));
    let command = fixture.service.live_home_command().unwrap();
    assert!(
        fixture
            .state
            .durable_jobs()
            .latest_attempt(command.home(), id(36))
            .unwrap()
            .is_none()
    );
    assert_eq!(
        fixture
            .syndic
            .discussion_handoff_gate(
                command.home(),
                id(36),
                SyndicPointReadLimit::new(256).unwrap()
            )
            .unwrap()
            .unwrap()
            .state(),
        DiscussionHandoffGateState::Open
    );
    drop(command);
    fixture.close();
}

#[test]
fn uncertain_admission_retains_job_custody_and_blocks_duplicate_acknowledgment() {
    let fixture = Fixture::new();
    fixture
        .faults
        .fail_next(FaultPoint::AfterCommitBeforePersist);
    let audit = match fixture.admit("resolve", 210, "exact").unwrap() {
        DiscussionResolutionOutcome::Command(DiscussionSettlementOutcome::Indeterminate {
            audit,
            ..
        }) => audit,
        _ => panic!("expected uncertainty"),
    };
    assert!(fixture.admit("resolve", 211, "replacement").is_err());
    drop(audit);
    assert!(fixture.admit("resolve", 211, "replacement").is_err());
    let audit = fixture
        .operations
        .retained_audit(JobId::from_bytes([210; 16]))
        .unwrap();
    let command = fixture.service.live_home_command().unwrap();
    assert!(matches!(
        audit
            .reconcile(command.home(), &fixture.syndic, &fixture.state)
            .unwrap(),
        DiscussionSettlementAuditOutcome::Settled(DiscussionSettlementResult::ResolutionAdmitted(
            _
        ))
    ));
    drop(command);
    drop(audit);
    assert!(
        fixture
            .operations
            .retained_audit(JobId::from_bytes([210; 16]))
            .is_none()
    );
    assert!(matches!(
        fixture.admit("resolve", 212, "replacement").unwrap(),
        DiscussionResolutionOutcome::Existing(_)
    ));
    fixture.close();
}

fn queue_future_input(
    store: &beryl_home_store::HomeStore,
    syndic: &SyndicStorage,
    turn: beryl_model::SyndicTurnId,
) {
    let gate = syndic
        .input_gate(store, id(36), SyndicPointReadLimit::new(400_000).unwrap())
        .unwrap()
        .unwrap();
    let waiting = InputGateRecord::new(
        id(36),
        gate.revision().checked_next().unwrap(),
        InputGateState::AwaitingTerminal(turn),
        gate.accepted_high_water(),
        gate.route_generation_high_water(),
        gate.selected_route(),
        gate.live_steering_count(),
        gate.live_next_turn_count(),
        gate.live_logical_utf8_bytes(),
    )
    .unwrap();
    support::commit(
        store,
        syndic.clone(),
        support::batch([syndic_storage::test_faults::FixtureRecord::InputGate(
            waiting,
        )]),
    );
    support::discussion_input::accept_next(store, syndic);
    assert_eq!(
        syndic
            .input_gate(store, id(36), SyndicPointReadLimit::new(400_000).unwrap())
            .unwrap()
            .unwrap()
            .live_next_turn_count(),
        1
    );
}

#[test]
fn intervening_input_rejects_both_participants_and_stale_cas_cannot_admit() {
    let fixture = Fixture::new();
    let wrong = BranchDiscussionResolutionContext::for_test(
        &fixture.service,
        id(36),
        fixture.source.resolving_target.pending().active_turn_id(),
        ResolutionRequestIdentity::new(
            fixture
                .source
                .resolving_target
                .pending()
                .cas_thread_id()
                .clone(),
            beryl_model::CasTurnId::new("different-turn").unwrap(),
            DynamicToolCallId::new("wrong").unwrap(),
        ),
    );
    let rejected = fixture
        .service
        .admit_discussion_resolution(
            &fixture.settlement,
            &wrong,
            ResolutionIntentId::from_bytes([215; 16]),
            ResolutionText::new("wrong").unwrap(),
            CommandCancellation::new(),
        )
        .unwrap()
        .publish(|outcome| outcome);
    assert!(matches!(
        rejected,
        DiscussionResolutionOutcome::Command(DiscussionSettlementOutcome::NotCommitted { .. })
    ));
    let DiscussionResolutionAdmission::Prepared(prepared) = fixture
        .settlement
        .prepare_resolution_for_test(
            &fixture.context("race"),
            ResolutionIntentId::from_bytes([216; 16]),
            ResolutionText::new("race").unwrap(),
            CommandCancellation::new(),
        )
        .unwrap()
    else {
        panic!("prepared admission");
    };
    let command = fixture.service.live_home_command().unwrap();
    queue_future_input(
        command.home(),
        &fixture.syndic,
        fixture.source.resolving_target.pending().active_turn_id(),
    );
    assert!(matches!(
        prepared.execute(),
        DiscussionSettlementOutcome::NotCommitted { .. }
    ));
    assert!(
        fixture
            .state
            .durable_jobs()
            .latest_attempt(command.home(), id(36))
            .unwrap()
            .is_none()
    );
    assert_eq!(
        fixture
            .syndic
            .discussion_handoff_gate(
                command.home(),
                id(36),
                SyndicPointReadLimit::new(256).unwrap()
            )
            .unwrap()
            .unwrap()
            .state(),
        DiscussionHandoffGateState::Open
    );
    drop(command);
    fixture.close();
}

#[test]
fn terminal_request_remains_idempotent_after_a_new_attempt() {
    let fixture = Fixture::new();
    let job = admitted(fixture.admit("old", 210, "old text").unwrap());
    let command = fixture.service.live_home_command().unwrap();
    let store = command.home();
    let prior = fixture
        .state
        .durable_jobs()
        .job(store, job)
        .unwrap()
        .unwrap();
    let state = fixture
        .state
        .durable_jobs()
        .prepare_handoff_job_transition(
            store,
            job,
            prior.revision(),
            beryl_state::HandoffJobTransition::TerminalFailure(
                beryl_state::HandoffFailureEvidence::new(
                    beryl_state::HandoffFailureKind::InvariantViolation,
                    None,
                )
                .unwrap(),
            ),
        )
        .unwrap();
    let gate = fixture
        .syndic
        .discussion_handoff_gate(store, id(36), SyndicPointReadLimit::new(256).unwrap())
        .unwrap()
        .unwrap();
    let syndic = fixture
        .syndic
        .prepare_discussion_handoff(store, DiscussionHandoffMutation::Release { expected: gate })
        .unwrap();
    let mut mutation = beryl_home_store::HomeCommand::new(store.home_revision().unwrap());
    mutation.add(state.contribution()).unwrap();
    mutation.add(syndic.contribution()).unwrap();
    assert!(matches!(
        store.execute(mutation),
        beryl_home_store::CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    drop(command);
    let new = admitted(fixture.admit("new", 211, "new text").unwrap());
    assert_ne!(new, job);
    assert!(
        matches!(fixture.admit("old", 212, "changed").unwrap(), DiscussionResolutionOutcome::Existing(id) if id == job)
    );
    let command = fixture.service.live_home_command().unwrap();
    let new = fixture
        .state
        .durable_jobs()
        .job(command.home(), new)
        .unwrap()
        .unwrap();
    assert_eq!(new.attempt_ordinal().get(), 2);
    assert_eq!(new.resolution().as_str(), "new text");
    drop(command);
    fixture.close();
}
