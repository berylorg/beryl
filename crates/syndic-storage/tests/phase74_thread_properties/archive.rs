use beryl_home_store::{
    CommandOutcome, HomeCommand, HomeOpenOptions, HomeSchemaVersion, HomeStore,
    test_faults::{FaultController, FaultPoint},
};
use beryl_model::{
    CasThreadId, CasTurnId, DynamicToolCallId, JobId, ResolutionIntentId, SyndicAcceptedInputId,
    SyndicDraftId, SyndicTurnId,
};
use beryl_state::{
    AdmitBranchHandoffJob, BerylState, BranchHandoffJobAdmission, BranchHandoffJobLifecycle,
    CompleteResolvingTurn, DiscussionContextDigest, DiscussionContextOwnerId, ParentCasIdentity,
    ParentHandoffIdentity, ParentQueueOrdinal, RecordParentCasAcceptance, ResolutionAttemptOrdinal,
    ResolutionRequestIdentity, ResolutionText, StartParentHandoff, SucceedBranchHandoff,
};
use syndic_storage::{
    ArchiveBranchDiscussionThread, SyndicPointReadLimit, SyndicStorage, ThreadArchiveState,
    ThreadAttributesRevision,
};

use crate::support::{TestHome, batch, commit, id, populated::populated_records, timestamp};

fn limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(1_000_000).unwrap()
}

fn open_with_faults(path: &std::path::Path, faults: FaultController) -> HomeStore {
    HomeStore::open_with_faults(
        HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT),
        faults,
    )
    .unwrap()
}

fn execute(store: &HomeStore, contribution: beryl_home_store::MutationContribution) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    match store.execute(command) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected clean archive fixture command, got {outcome:?}"),
    }
}

fn job(store: &HomeStore, state: BerylState, job_id: JobId) -> beryl_state::BranchHandoffJobRecord {
    state.durable_jobs().job(store, job_id).unwrap().unwrap()
}

fn admission() -> BranchHandoffJobAdmission {
    BranchHandoffJobAdmission::new(
        ResolutionIntentId::from_bytes([0x74; 16]),
        ResolutionAttemptOrdinal::new(1).unwrap(),
        id(36),
        id(30),
        DiscussionContextOwnerId::Draft(SyndicDraftId::from_bytes([37; 16])),
        DiscussionContextDigest::from_bytes([0x75; 32]),
        SyndicTurnId::from_bytes([32; 16]),
        ResolutionRequestIdentity::new(
            CasThreadId::new("phase74-child-cas").unwrap(),
            CasTurnId::new("phase74-child-turn").unwrap(),
            DynamicToolCallId::new("phase74-resolve-tool").unwrap(),
        ),
        ParentQueueOrdinal::new(1),
        ResolutionText::new("Apply the exact resolved branch result.").unwrap(),
    )
}

fn prepare_parent_active_job(store: &HomeStore, state: BerylState, syndic: SyndicStorage) -> JobId {
    commit(store, syndic, batch(populated_records()));
    let admission = admission();
    let job_id = admission.job_id();
    execute(
        store,
        state.durable_jobs().admit_branch_handoff(
            state.durable_jobs().revision(store).unwrap(),
            AdmitBranchHandoffJob::new(admission),
        ),
    );
    execute(
        store,
        state.durable_jobs().complete_resolving_turn(
            state.durable_jobs().revision(store).unwrap(),
            CompleteResolvingTurn::new(job_id, job(store, state, job_id).revision()),
        ),
    );
    execute(
        store,
        state.durable_jobs().start_parent_handoff(
            state.durable_jobs().revision(store).unwrap(),
            StartParentHandoff::new(
                job_id,
                job(store, state, job_id).revision(),
                ParentHandoffIdentity::new(
                    SyndicAcceptedInputId::from_bytes([0x76; 16]),
                    SyndicTurnId::from_bytes([0x77; 16]),
                ),
            ),
        ),
    );
    execute(
        store,
        state.durable_jobs().record_parent_cas_acceptance(
            state.durable_jobs().revision(store).unwrap(),
            RecordParentCasAcceptance::new(
                job_id,
                job(store, state, job_id).revision(),
                ParentCasIdentity::new(
                    CasThreadId::new("phase74-parent-cas").unwrap(),
                    CasTurnId::new("phase74-parent-turn").unwrap(),
                ),
            ),
        ),
    );
    assert_eq!(
        job(store, state, job_id).lifecycle(),
        BranchHandoffJobLifecycle::ParentActive
    );
    assert_eq!(
        syndic
            .thread_attributes(store, id(36), limit())
            .unwrap()
            .unwrap()
            .archive(),
        ThreadArchiveState::BranchDiscussionOpen
    );
    job_id
}

fn terminal_command(
    store: &HomeStore,
    state: BerylState,
    syndic: SyndicStorage,
    job_id: JobId,
) -> HomeCommand {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(state.durable_jobs().succeed_branch_handoff(
            state.durable_jobs().revision(store).unwrap(),
            SucceedBranchHandoff::new(job_id, job(store, state, job_id).revision()),
        ))
        .unwrap();
    command
        .add(syndic.archive_branch_discussion(
            syndic.revision(store).unwrap(),
            ArchiveBranchDiscussionThread::new(
                id(36),
                ThreadAttributesRevision::FIRST,
                job_id,
                timestamp(20),
            ),
        ))
        .unwrap();
    command
}

#[test]
fn durable_job_success_and_intrinsic_archive_publish_atomically() {
    let home = TestHome::new("phase74-archive-atomic-success");
    let mut store = open_with_faults(home.path(), FaultController::new());
    let state = BerylState::register(&mut store).unwrap();
    let syndic = SyndicStorage::register(&mut store).unwrap();
    let job_id = prepare_parent_active_job(&store, state, syndic);

    match store.execute(terminal_command(&store, state, syndic, job_id)) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected clean terminal archive command, got {outcome:?}"),
    }
    assert_eq!(
        job(&store, state, job_id).lifecycle(),
        BranchHandoffJobLifecycle::Succeeded
    );
    let attributes = syndic
        .thread_attributes(&store, id(36), limit())
        .unwrap()
        .unwrap();
    assert_eq!(attributes.revision().get(), 2);
    assert_eq!(
        attributes.archive(),
        ThreadArchiveState::BranchDiscussionArchived {
            handoff_job_id: job_id,
            archived_at: timestamp(20),
        }
    );
    store.validate_registered_domains().unwrap();
}

#[test]
fn commit_fault_leaves_both_job_and_archive_at_their_pre_success_state() {
    let home = TestHome::new("phase74-archive-atomic-fault");
    let faults = FaultController::new();
    let mut store = open_with_faults(home.path(), faults.clone());
    let state = BerylState::register(&mut store).unwrap();
    let syndic = SyndicStorage::register(&mut store).unwrap();
    let job_id = prepare_parent_active_job(&store, state, syndic);

    faults.fail_next(FaultPoint::BeforeCommit);
    match store.execute(terminal_command(&store, state, syndic, job_id)) {
        CommandOutcome::NotCommitted { .. } => {}
        outcome => panic!("expected pre-commit archive fault, got {outcome:?}"),
    }
    store.verify_health().unwrap();
    assert_eq!(
        job(&store, state, job_id).lifecycle(),
        BranchHandoffJobLifecycle::ParentActive
    );
    let attributes = syndic
        .thread_attributes(&store, id(36), limit())
        .unwrap()
        .unwrap();
    assert_eq!(attributes.revision(), ThreadAttributesRevision::FIRST);
    assert_eq!(
        attributes.archive(),
        ThreadArchiveState::BranchDiscussionOpen
    );
    store.validate_registered_domains().unwrap();
}
