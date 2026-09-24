use super::*;

#[test]
fn fresh_candidate_inspects_both_sides_of_admission_without_creating_an_attempt() {
    for point in [
        FaultPoint::BeforeCommit,
        FaultPoint::AfterCommitBeforePersist,
    ] {
        let fixture = Fixture::new();
        let context = fixture.context("recover");
        let Fixture {
            directory,
            service,
            settlement,
            operations,
            state,
            syndic,
            faults,
            source,
        } = fixture;
        drop((settlement, operations, state, syndic, faults, source));
        service.close().unwrap();
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
        let operations = DiscussionSettlementOperations::new(
            ProcessAdmissionGate::new(),
            NonZeroUsize::new(1).unwrap(),
        );
        let settlement = DiscussionSettlementService::new(
            operations.clone(),
            store.service_reference(),
            state.clone(),
            syndic.clone(),
        );
        let DiscussionResolutionAdmission::Prepared(prepared) = settlement
            .prepare_resolution_for_test(
                &context,
                ResolutionIntentId::from_bytes([210; 16]),
                ResolutionText::new("recover exact admission").unwrap(),
                CommandCancellation::new(),
            )
            .unwrap()
        else {
            panic!("prepared admission");
        };
        let audit = prepared.audit();
        faults.fail_next(point);
        match prepared.execute() {
            DiscussionSettlementOutcome::NotCommitted { .. }
                if point == FaultPoint::BeforeCommit => {}
            DiscussionSettlementOutcome::Indeterminate { .. }
                if point == FaultPoint::AfterCommitBeforePersist => {}
            _ => panic!("wrong injected outcome"),
        }
        if store.health().state() == beryl_home_store::HomeHealthState::Healthy {
            faults.fail_next(FaultPoint::BeforeReadConfirmation);
            assert!(store.home_revision().is_err());
        }
        drop(settlement);
        let mut candidate = store.recover_same_home().unwrap();
        let fresh_state = BerylState::reacquire_candidate(&candidate).unwrap();
        let fresh_syndic = SyndicStorage::reacquire_candidate(&candidate).unwrap();
        let access = candidate.recovery_access().unwrap();
        assert!(audit.reconcile_candidate(&access, &syndic, &state).is_err());
        let before = access.home_revision().unwrap();
        let expected = if point == FaultPoint::BeforeCommit {
            DiscussionSettlementAuditOutcome::NotCommitted
        } else {
            DiscussionSettlementAuditOutcome::Settled(
                DiscussionSettlementResult::ResolutionAdmitted(JobId::from_bytes([210; 16])),
            )
        };
        assert_eq!(
            audit
                .reconcile_candidate(&access, &fresh_syndic, &fresh_state)
                .unwrap(),
            expected
        );
        assert_eq!(access.home_revision().unwrap(), before);
        assert!(access.pending_reconciliations().is_empty());
        assert!(
            operations
                .retained_audit(JobId::from_bytes([210; 16]))
                .is_none()
        );
        let job = fresh_state
            .durable_jobs()
            .admitted_handoff_request_candidate(&access, context.request_identity())
            .unwrap();
        assert_eq!(job.is_some(), point == FaultPoint::AfterCommitBeforePersist);
        drop(audit);
        candidate.publish().unwrap().close().unwrap();
    }
}
