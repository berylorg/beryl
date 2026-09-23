use beryl_home_store::{
    CommandOutcome, CursorReadLimits, HomeCandidateRecoveryAccess, HomeCommand, HomeOpenCandidate,
    HomeOpenOptions, HomeOpenPublication, HomeSchemaVersion,
    test_faults::{FaultController, FaultPoint},
};
use beryl_model::{
    CasThreadId, CasTurnId, DynamicToolCallId, JobId, ResolutionIntentId, SyndicDraftId,
    SyndicThreadId, SyndicTurnId,
};
use beryl_state::{
    AdmitBranchHandoffJob, BerylState, BranchHandoffJobAdmission, BranchHandoffJobRecord,
    DiscussionContextDigest, DiscussionContextOwnerId, DurableJobState, ParentQueueOrdinal,
    ResolutionAttemptOrdinal, ResolutionRequestIdentity, ResolutionText,
};

fn candidate(
    directory: &tempfile::TempDir,
    faults: FaultController,
) -> (HomeOpenPublication, BerylState) {
    let mut opening = HomeOpenCandidate::open_with_faults(
        HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
        faults,
    )
    .unwrap();
    let state = BerylState::register(&mut opening).unwrap();
    (
        opening
            .prepare_publication(BerylState::required_domains().unwrap())
            .unwrap(),
        state,
    )
}

fn admit(access: &HomeCandidateRecoveryAccess<'_>, jobs: &DurableJobState, seed: u8) -> JobId {
    let admission = BranchHandoffJobAdmission::new(
        ResolutionIntentId::from_bytes([seed; 16]),
        ResolutionAttemptOrdinal::FIRST,
        SyndicThreadId::from_bytes([seed; 16]),
        SyndicThreadId::from_bytes([90; 16]),
        DiscussionContextOwnerId::Draft(SyndicDraftId::from_bytes([40; 16])),
        DiscussionContextDigest::from_bytes([50; 32]),
        SyndicTurnId::from_bytes([60; 16]),
        ResolutionRequestIdentity::new(
            CasThreadId::new("c".repeat(256)).unwrap(),
            CasTurnId::new("t".repeat(256)).unwrap(),
            DynamicToolCallId::new(format!("{seed:03}{}", "r".repeat(253))).unwrap(),
        ),
        ParentQueueOrdinal::new(7),
        ResolutionText::new("🦀".repeat(beryl_state::RESOLUTION_TEXT_MAX_SCALARS)).unwrap(),
    );
    let id = admission.job_id();
    let mut command = HomeCommand::new(access.home_revision().unwrap());
    command
        .add(jobs.admit_branch_handoff(
            jobs.revision_candidate(access).unwrap(),
            AdmitBranchHandoffJob::new(admission),
        ))
        .unwrap();
    assert!(matches!(
        access.execute(command),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    id
}

fn assert_point_closure(
    jobs: &DurableJobState,
    access: &HomeCandidateRecoveryAccess<'_>,
    expected: &BranchHandoffJobRecord,
) {
    assert_eq!(
        jobs.job_candidate(access, expected.job_id())
            .unwrap()
            .as_ref(),
        Some(expected)
    );
    assert_eq!(
        jobs.request_admission_candidate(access, expected.request())
            .unwrap()
            .unwrap()
            .job_id(),
        expected.job_id()
    );
    assert_eq!(
        jobs.latest_attempt_candidate(access, expected.discussion_thread_id())
            .unwrap()
            .unwrap()
            .job_id(),
        expected.job_id()
    );
}

#[test]
fn candidate_jobs_preserve_maximum_payload_and_independent_cursor_caps() {
    let directory = tempfile::tempdir().unwrap();
    let (mut candidate, state) = candidate(&directory, FaultController::new());
    let jobs = state.durable_jobs();
    let reference = candidate.service_reference();
    let access = candidate.recovery_access().unwrap();
    let mut ids = [admit(&access, &jobs, 1), admit(&access, &jobs, 2)];
    ids.sort();
    let first = jobs.job_candidate(&access, ids[0]).unwrap().unwrap();
    assert_eq!(
        first.resolution().as_str().len(),
        beryl_state::RESOLUTION_TEXT_MAX_BYTES
    );
    assert_point_closure(&jobs, &access, &first);
    assert!(jobs.revision(&reference).is_err());
    assert!(jobs.job(&reference, ids[0]).is_err());
    let missing = JobId::from_bytes([0; 16]);
    assert_eq!(jobs.job_candidate(&access, missing).unwrap(), None);
    let page = jobs
        .list_live_candidate(&access, None, CursorReadLimits::new(2, 400_000).unwrap())
        .unwrap();
    assert_eq!(page.records().len(), 1);
    assert!(page.has_more());
    assert!(page.stored_bytes() <= 400_000);
    assert_eq!(page.records()[0], first);
    let count_page = jobs
        .list_live_candidate(&access, None, CursorReadLimits::new(1, 800_000).unwrap())
        .unwrap();
    assert_eq!(count_page.records(), page.records());
    assert_eq!(count_page.stored_bytes(), page.stored_bytes());
    assert_eq!(count_page.decoded_bytes(), page.decoded_bytes());
    assert!(
        jobs.list_live_candidate(
            &access,
            None,
            CursorReadLimits::new(2, page.stored_bytes() - 1).unwrap()
        )
        .is_err()
    );
    let exact = jobs
        .list_live_candidate(
            &access,
            None,
            CursorReadLimits::new(2, page.stored_bytes()).unwrap(),
        )
        .unwrap();
    assert_eq!(exact.records(), page.records());
    let next = jobs
        .list_live_candidate(
            &access,
            Some(ids[0]),
            CursorReadLimits::new(2, 400_000).unwrap(),
        )
        .unwrap();
    assert_eq!(next.records().len(), 1);
    assert_eq!(next.records()[0].job_id(), ids[1]);
    assert!(!next.has_more());
    let end = jobs
        .list_live_candidate(
            &access,
            Some(ids[1]),
            CursorReadLimits::new(2, 400_000).unwrap(),
        )
        .unwrap();
    assert!(end.records().is_empty());
    let store = candidate.publish().unwrap();
    let ordinary = jobs
        .list_live(&store, None, CursorReadLimits::new(2, 400_000).unwrap())
        .unwrap();
    assert_eq!(ordinary.records(), page.records());
    assert_eq!(ordinary.stored_bytes(), page.stored_bytes());
    assert_eq!(ordinary.decoded_bytes(), page.decoded_bytes());
    store.close().unwrap();
    let (mut reopened, fresh) = self::candidate(&directory, FaultController::new());
    assert_point_closure(
        &fresh.durable_jobs(),
        &reopened.recovery_access().unwrap(),
        &first,
    );
    reopened.publish().unwrap().close().unwrap();
}

#[test]
fn recovered_job_reads_require_fresh_exact_home_handles() {
    let directory = tempfile::tempdir().unwrap();
    let foreign_directory = tempfile::tempdir().unwrap();
    let faults = FaultController::new();
    let (mut candidate, state) = candidate(&directory, faults.clone());
    let (foreign, foreign_state) = self::candidate(&foreign_directory, FaultController::new());
    let jobs = state.durable_jobs();
    let access = candidate.recovery_access().unwrap();
    let id = admit(&access, &jobs, 3);
    let expected = jobs.job_candidate(&access, id).unwrap().unwrap();
    let revision = jobs.revision_candidate(&access).unwrap();
    let store = candidate.publish().unwrap();
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(store.home_revision().is_err());
    let mut recovered = store.recover_same_home().unwrap();
    let reference = recovered.service_reference();
    let fresh = BerylState::reacquire_candidate(&recovered)
        .unwrap()
        .durable_jobs();
    let access = recovered.recovery_access().unwrap();
    for stale in [&jobs, &foreign_state.durable_jobs()] {
        assert!(stale.revision_candidate(&access).is_err());
        assert!(stale.job_candidate(&access, id).is_err());
        assert!(
            stale
                .request_admission_candidate(&access, expected.request())
                .is_err()
        );
        assert!(
            stale
                .latest_attempt_candidate(&access, expected.discussion_thread_id())
                .is_err()
        );
        assert!(
            stale
                .list_live_candidate(&access, None, CursorReadLimits::new(1, 400_000).unwrap())
                .is_err()
        );
    }
    assert_eq!(fresh.revision_candidate(&access).unwrap(), revision);
    assert_point_closure(&fresh, &access, &expected);
    assert!(fresh.job(&reference, id).is_err());
    assert_eq!(
        fresh
            .list_live_candidate(&access, None, CursorReadLimits::new(1, 400_000).unwrap())
            .unwrap()
            .records(),
        &[expected]
    );
    recovered.publish().unwrap().close().unwrap();
    foreign.publish().unwrap().close().unwrap();
}

#[test]
fn failed_job_candidate_read_prevents_publication() {
    for recovering in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let faults = FaultController::new();
        let (mut candidate, state) = candidate(&directory, faults.clone());
        if recovering {
            let store = candidate.publish().unwrap();
            faults.fail_next(FaultPoint::BeforeReadConfirmation);
            assert!(store.home_revision().is_err());
            let mut recovered = store.recover_same_home().unwrap();
            let jobs = BerylState::reacquire_candidate(&recovered)
                .unwrap()
                .durable_jobs();
            let access = recovered.recovery_access().unwrap();
            faults.fail_next(FaultPoint::BeforeReadConfirmation);
            assert!(
                jobs.list_live_candidate(&access, None, CursorReadLimits::new(1, 400_000).unwrap())
                    .is_err()
            );
            assert!(jobs.revision_candidate(&access).is_err());
            assert!(recovered.publish().is_err());
        } else {
            let access = candidate.recovery_access().unwrap();
            faults.fail_next(FaultPoint::BeforeReadConfirmation);
            assert!(
                state
                    .durable_jobs()
                    .job_candidate(&access, JobId::from_bytes([0; 16]))
                    .is_err()
            );
            assert!(state.durable_jobs().revision_candidate(&access).is_err());
            assert!(candidate.publish().is_err());
        }
    }
}
