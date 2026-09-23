use super::*;

#[test]
fn creation_publishes_one_unclaimed_catalog_closure_and_survives_lost_acknowledgement() {
    let fixture = Fixture::new(1);
    let before = fixture
        .syndic
        .thread(&fixture.store, id(30), limit())
        .unwrap();
    let draft = fixture
        .syndic
        .current_draft(&fixture.store, id(30), limit())
        .unwrap()
        .unwrap()
        .draft()
        .clone();
    let session_revision = fixture.state.session().revision(&fixture.store).unwrap();
    let prepared = fixture.prepare(210, CommandCancellation::new());
    let audit = prepared.audit();
    assert_eq!(
        audit
            .reconcile(&fixture.store, &fixture.syndic, &fixture.state)
            .unwrap(),
        DiscussionCreationAuditOutcome::Pending
    );
    let DiscussionCreationOutcome::Committed {
        created,
        later_failure: None,
        ..
    } = prepared.execute()
    else {
        panic!("creation must commit")
    };
    assert_eq!(
        created,
        CreatedDiscussion {
            thread_id: id(210),
            draft_id: draft_id(211)
        }
    );
    assert_eq!(
        audit
            .reconcile(&fixture.store, &fixture.syndic, &fixture.state)
            .unwrap(),
        DiscussionCreationAuditOutcome::Created(created)
    );
    let row = fixture
        .state
        .catalog()
        .row(
            &fixture.store,
            created.thread_id,
            CatalogPointReadLimit::schema_maximum(),
        )
        .unwrap()
        .unwrap();
    assert_eq!(row.facts().claim(), CatalogClaimSummary::Unclaimed);
    assert_eq!(row.sources().claim(), None);
    assert_eq!(
        fixture.state.session().revision(&fixture.store).unwrap(),
        session_revision
    );
    assert_eq!(
        fixture
            .syndic
            .thread(&fixture.store, id(30), limit())
            .unwrap(),
        before
    );
    assert_eq!(
        fixture
            .syndic
            .current_draft(&fixture.store, id(30), limit())
            .unwrap()
            .unwrap()
            .draft(),
        &draft
    );
    fixture
        .store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    drop(audit);
    let retry = fixture.prepare(210, CommandCancellation::new());
    assert!(matches!(
        retry.execute(),
        DiscussionCreationOutcome::NotCommitted { .. }
    ));
    fixture.store.close().unwrap();
}

#[test]
fn cancellation_and_retained_audit_share_capacity_across_service_replacement() {
    let fixture = Fixture::new(1);
    let cancelled = CommandCancellation::new();
    cancelled.cancel();
    assert!(matches!(
        fixture.service().prepare(
            source(&fixture.store, &fixture.syndic),
            request(210),
            cancelled
        ),
        Err(DiscussionCreationError::Cancelled)
    ));
    let cancellation = CommandCancellation::new();
    let prepared = fixture.prepare(210, cancellation.clone());
    let audit = prepared.audit();
    assert!(matches!(
        fixture.service().prepare(
            source(&fixture.store, &fixture.syndic),
            request(210),
            CommandCancellation::new()
        ),
        Err(DiscussionCreationError::DuplicateIdentity)
    ));
    assert!(matches!(
        fixture.service().prepare(
            source(&fixture.store, &fixture.syndic),
            request(212),
            CommandCancellation::new()
        ),
        Err(DiscussionCreationError::Capacity)
    ));
    cancellation.cancel();
    assert!(matches!(
        prepared.execute(),
        DiscussionCreationOutcome::NotCommitted { .. }
    ));
    assert_eq!(
        audit
            .reconcile(&fixture.store, &fixture.syndic, &fixture.state)
            .unwrap(),
        DiscussionCreationAuditOutcome::NotCommitted
    );
    assert!(matches!(
        fixture.service().prepare(
            source(&fixture.store, &fixture.syndic),
            request(212),
            CommandCancellation::new()
        ),
        Err(DiscussionCreationError::Capacity)
    ));
    no_child(&fixture.store, &fixture.syndic, &fixture.state, 210);
    drop(audit);
    let next = fixture.prepare(212, CommandCancellation::new());
    let abandoned = next.audit();
    drop(next);
    assert_eq!(
        abandoned
            .reconcile(&fixture.store, &fixture.syndic, &fixture.state)
            .unwrap(),
        DiscussionCreationAuditOutcome::NotCommitted
    );
    drop(abandoned);
    drop(fixture.prepare(214, CommandCancellation::new()));
    fixture.store.close().unwrap();
}

#[test]
fn changed_runtime_rejects_the_whole_prepared_command() {
    let fixture = Fixture::new(1);
    let prepared = fixture.prepare(210, CommandCancellation::new());
    let binding = fixture
        .syndic
        .thread_catalog_summary(&fixture.store, id(30), limit())
        .unwrap()
        .unwrap()
        .execution()
        .clone();
    let runtime = fixture
        .state
        .runtime_roots()
        .runtime(&fixture.store, binding.runtime_id())
        .unwrap()
        .unwrap();
    let mut command = HomeCommand::new(fixture.store.home_revision().unwrap());
    command
        .add(
            fixture.state.runtime_roots().set_runtime_availability(
                fixture
                    .state
                    .runtime_roots()
                    .revision(&fixture.store)
                    .unwrap(),
                beryl_state::SetRuntimeAvailability::new(
                    runtime.runtime_id(),
                    runtime.revision(),
                    AvailabilitySnapshot::observed(
                        beryl_model::Availability::Available,
                        UnixMillis::new(2),
                    )
                    .unwrap(),
                ),
            ),
        )
        .unwrap();
    assert!(matches!(
        fixture.store.execute(command),
        CommandOutcome::Committed { .. }
    ));
    assert!(matches!(
        prepared.execute(),
        DiscussionCreationOutcome::NotCommitted { .. }
    ));
    no_child(&fixture.store, &fixture.syndic, &fixture.state, 210);
    fixture.store.close().unwrap();
}

#[test]
fn ambiguous_visible_records_are_settled_before_audit_reports_created() {
    let fixture = Fixture::new(1);
    let prepared = fixture.prepare(210, CommandCancellation::new());
    let retained = prepared.audit();
    fixture
        .faults
        .fail_next(FaultPoint::AfterCommitBeforePersist);
    let DiscussionCreationOutcome::Indeterminate { audit, .. } = prepared.execute() else {
        panic!("expected ambiguous creation")
    };
    assert_eq!(fixture.store.pending_reconciliations().len(), 1);
    assert!(
        fixture
            .syndic
            .thread(&fixture.store, id(210), limit())
            .unwrap()
            .is_some()
    );
    assert!(
        fixture
            .state
            .catalog()
            .row(
                &fixture.store,
                id(210),
                CatalogPointReadLimit::schema_maximum()
            )
            .unwrap()
            .is_some()
    );
    drop(audit);
    fixture
        .faults
        .fail_next(FaultPoint::BeforeReconciliationSnapshot);
    assert!(
        retained
            .reconcile(&fixture.store, &fixture.syndic, &fixture.state)
            .is_err()
    );
    let pending = fixture.store.pending_reconciliations();
    assert_eq!(pending.len(), 1);
    assert!(fixture.store.reconcile(&pending[0]).is_err());
    assert_eq!(
        retained
            .reconcile(&fixture.store, &fixture.syndic, &fixture.state)
            .unwrap(),
        DiscussionCreationAuditOutcome::Created(retained.identity())
    );
    assert!(fixture.store.pending_reconciliations().is_empty());
    fixture.store.close().unwrap();
}

#[test]
fn candidate_recovery_uses_same_identity_fresh_handles_and_retained_registry_scope() {
    for (fault, committed) in [
        (FaultPoint::BeforeCommit, false),
        (FaultPoint::AfterPersist, true),
        (FaultPoint::AfterCommitBeforePersist, true),
    ] {
        let fixture = Fixture::new(2);
        let prepared = fixture.prepare(210, CommandCancellation::new());
        let audit = prepared.audit();
        let stale = fixture.prepare(212, CommandCancellation::new());
        fixture.faults.fail_next(fault);
        let outcome = prepared.execute();
        assert!(match fault {
            FaultPoint::BeforeCommit =>
                matches!(outcome, DiscussionCreationOutcome::NotCommitted { .. }),
            FaultPoint::AfterPersist => matches!(
                outcome,
                DiscussionCreationOutcome::Committed {
                    later_failure: Some(_),
                    ..
                }
            ),
            _ => matches!(outcome, DiscussionCreationOutcome::Indeterminate { .. }),
        });
        if fixture.store.health().state() == beryl_home_store::HomeHealthState::Healthy {
            fixture.faults.fail_next(FaultPoint::BeforeReadConfirmation);
            assert!(fixture.store.home_revision().is_err());
        }
        let mut candidate = fixture.store.recover_same_home().unwrap();
        let syndic = SyndicStorage::reacquire_candidate(&candidate).unwrap();
        let state = BerylState::reacquire_candidate(&candidate).unwrap();
        let access = candidate.recovery_access().unwrap();
        assert!(
            audit
                .reconcile_candidate(&access, &fixture.syndic, &fixture.state)
                .is_err()
        );
        let expected = if committed {
            DiscussionCreationAuditOutcome::Created(audit.identity())
        } else {
            DiscussionCreationAuditOutcome::NotCommitted
        };
        assert_eq!(
            audit.reconcile_candidate(&access, &syndic, &state).unwrap(),
            expected
        );
        assert!(access.pending_reconciliations().is_empty());
        let store = candidate.publish().unwrap();
        assert!(matches!(
            stale.execute(),
            DiscussionCreationOutcome::NotCommitted { .. }
        ));
        assert_eq!(audit.reconcile(&store, &syndic, &state).unwrap(), expected);
        no_child(&store, &syndic, &state, 212);
        store
            .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
            .unwrap();
        store.close().unwrap();
    }
}

#[test]
fn partial_ambiguous_creation_never_reports_created_or_releases_registry_scope() {
    let fixture = Fixture::new(1);
    let prepared = fixture.prepare(210, CommandCancellation::new());
    let audit = prepared.audit();
    fixture
        .faults
        .fail_next(FaultPoint::AfterCommitBeforePersist);
    assert!(matches!(
        prepared.execute(),
        DiscussionCreationOutcome::Indeterminate { .. }
    ));
    let mut batch = syndic_storage::test_faults::FixtureBatch::new();
    batch
        .delete(syndic_storage::test_faults::FixtureDelete::DiscussionHandoffGate(id(210)))
        .unwrap();
    support::commit(&fixture.store, fixture.syndic.clone(), batch);
    assert_eq!(
        audit
            .reconcile(&fixture.store, &fixture.syndic, &fixture.state)
            .unwrap(),
        DiscussionCreationAuditOutcome::Collision
    );
    assert_eq!(fixture.store.pending_reconciliations().len(), 1);
    assert_eq!(
        audit
            .reconcile(&fixture.store, &fixture.syndic, &fixture.state)
            .unwrap(),
        DiscussionCreationAuditOutcome::Collision
    );
    fixture.store.close().unwrap();
}
