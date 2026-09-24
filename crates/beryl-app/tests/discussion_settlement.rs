#![cfg(feature = "test-faults")]
#[path = "discussion_settlement/archived_parent.rs"]
mod archived_parent;
#[path = "discussion_settlement/atomicity.rs"]
mod atomicity;
#[path = "discussion_settlement/cases.rs"]
mod cases;
#[path = "discussion_settlement/parent_input.rs"]
mod parent_input;
#[path = "discussion_settlement/parent_execution.rs"]
mod parent_execution;
#[path = "discussion_settlement/recovery.rs"]
mod recovery;
#[path = "../../syndic-storage/tests/support/mod.rs"]
mod support;

use beryl_app::{discussion_settlement::*, process_admission::ProcessAdmissionGate};
use beryl_home_store::{
    CommandCancellation, CommandOutcome, HomeCommand, HomeOpenCandidate, HomeOpenOptions,
    HomeSchemaVersion, HomeStore,
    test_faults::{FaultController, FaultPoint},
};
use beryl_model::{JobId, ResolutionIntentId};
use beryl_state::{
    AdmitBranchHandoffJob, BerylState, BranchHandoffJobAdmission, BranchHandoffJobLifecycle,
    ParentQueueOrdinal, ResolutionAttemptOrdinal, ResolutionRequestIdentity, ResolutionText,
};
use std::num::NonZeroUsize;
use support::id;
use syndic_storage::*;

fn limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(400_000).unwrap()
}

struct Fixture {
    _directory: tempfile::TempDir,
    store: HomeStore,
    state: BerylState,
    syndic: SyndicStorage,
    faults: FaultController,
    process: ProcessAdmissionGate,
    operations: DiscussionSettlementOperations,
    job: JobId,
    gate: DiscussionHandoffGateRecord,
}
impl Fixture {
    fn new(queued: bool) -> Self {
        Self::with_resolution(queued, "Resolution result")
    }
    fn with_resolution(queued: bool, resolution: &str) -> Self {
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
        support::seed_populated(&store, syndic.clone());
        let mut request = support::discussion_handoff::active_request(
            &store,
            &syndic,
            ResolutionIntentId::from_bytes([210; 16]),
            JobId::from_bytes([211; 16]),
        );
        if queued {
            support::discussion_input::accept_next(&store, &syndic);
            request.thread_revision = syndic
                .thread(&store, id(36), limit())
                .unwrap()
                .unwrap()
                .revision();
            request.input_gate_revision = syndic
                .input_gate(&store, id(36), limit())
                .unwrap()
                .unwrap()
                .revision();
        }
        let (job, gate) =
            admit_request_with_resolution(&store, &state, &syndic, request, resolution);
        let process = ProcessAdmissionGate::new();
        let operations =
            DiscussionSettlementOperations::new(process.clone(), NonZeroUsize::new(1).unwrap());
        Self {
            _directory: directory,
            store,
            state,
            syndic,
            faults,
            process,
            operations,
            job,
            gate,
        }
    }
    fn service(&self) -> DiscussionSettlementService {
        DiscussionSettlementService::new(
            self.operations.clone(),
            self.store.service_reference(),
            self.state.clone(),
            self.syndic.clone(),
        )
    }
    fn finish_child(&self) {
        support::discussion_handoff::complete_resolving_turn(&self.store, &self.syndic);
    }
    fn prepare(&self) -> PreparedDiscussionSettlement<'static> {
        self.service()
            .prepare(self.job, CommandCancellation::new())
            .unwrap()
            .unwrap()
    }
    fn audit(&self, audit: &DiscussionSettlementAudit) -> DiscussionSettlementAuditOutcome {
        audit
            .reconcile(&self.store, &self.syndic, &self.state)
            .unwrap()
    }
}

fn admit_request(
    store: &HomeStore,
    state: &BerylState,
    syndic: &SyndicStorage,
    request: AdmitDiscussionHandoff,
) -> (JobId, DiscussionHandoffGateRecord) {
    admit_request_with_resolution(store, state, syndic, request, "Resolution result")
}

fn admit_request_with_resolution(
    store: &HomeStore,
    state: &BerylState,
    syndic: &SyndicStorage,
    mut request: AdmitDiscussionHandoff,
    resolution: &str,
) -> (JobId, DiscussionHandoffGateRecord) {
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
        ResolutionText::new(resolution).unwrap(),
    );
    let job = admission.job_id();
    request.job_id = job;
    let handoff = syndic
        .prepare_discussion_handoff(store, DiscussionHandoffMutation::Admit(request))
        .unwrap();
    let gate = handoff.intent().new_gate();
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(handoff.contribution()).unwrap();
    command
        .add(state.durable_jobs().admit_branch_handoff(
            state.durable_jobs().revision(store).unwrap(),
            AdmitBranchHandoffJob::new(admission),
        ))
        .unwrap();
    assert!(matches!(
        store.execute(command),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    (job, gate)
}
