use super::*;

#[test]
fn candidate_recovery_proves_original_pending_noncommit_without_admitting_replacement() {
    let fixture = Fixture::new();
    let original = prepared(&fixture, None, 9);
    let journal_fault = beryl_home_store::test_faults::fail_next_journal_write();
    let outcome = original.commit(&fixture.store, &fixture.state);
    drop(journal_fault);
    let SameWindowThreadOutcome::Pending(pending) = outcome else {
        panic!("expected original indeterminate journal failure")
    };
    fixture.faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(fixture.store.home_revision().is_err());
    let mut recovery = fixture.store.recover_same_home().unwrap();
    let fresh = BerylState::reacquire_candidate(&recovery).unwrap();
    let access = recovery.recovery_access().unwrap();
    let revision = access.home_revision().unwrap();
    assert!(matches!(
        pending.reconcile_candidate(&access, &fresh),
        SameWindowThreadOutcome::NotCommitted(SameWindowThreadError::ReconciledOld)
    ));
    assert!(
        fresh
            .catalog()
            .current_row_source_candidate(
                &access,
                thread(9),
                CatalogPointReadLimit::schema_maximum()
            )
            .unwrap()
            .is_none()
    );
    assert_eq!(access.home_revision().unwrap(), revision);
    let store = recovery.publish().unwrap();
    assert_eq!(store.home_revision().unwrap(), revision);
}

#[test]
fn candidate_recovery_settles_original_pending_claim_without_another_acquisition() {
    let fixture = Fixture::new();
    let original = prepared(&fixture, None, 9);
    let expected = original.future_selection();
    fixture
        .faults
        .fail_next(FaultPoint::AfterCommitBeforePersist);
    let SameWindowThreadOutcome::Pending(pending) = original.commit(&fixture.store, &fixture.state)
    else {
        panic!("expected original pending claim");
    };
    fixture.faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(fixture.store.home_revision().is_err());
    let mut recovery = fixture.store.recover_same_home().unwrap();
    let fresh = BerylState::reacquire_candidate(&recovery).unwrap();
    let access = recovery.recovery_access().unwrap();
    let revision = access.home_revision().unwrap();
    let SameWindowThreadOutcome::Settled(commit) = pending.reconcile_candidate(&access, &fresh)
    else {
        panic!("expected exact original candidate commitment");
    };
    assert_eq!(commit.selection, expected);
    assert!(commit.validate_candidate(&access, &fresh).is_ok());
    assert!(commit.validate_candidate(&access, &fixture.state).is_err());
    let foreign = Fixture::new();
    assert!(commit.validate_candidate(&access, &foreign.state).is_err());
    assert_eq!(access.home_revision().unwrap(), revision);
    let store = recovery.publish().unwrap();
    assert_eq!(store.home_revision().unwrap(), revision);
}

#[test]
fn known_commit_candidate_validation_retains_original_joined_facts_and_receipt() {
    let fixture = Fixture::new();
    let first = settled(&fixture, prepared(&fixture, None, 9));
    assert_eq!(first.selection.thread_id(), thread(3));
    disqualify_submission(&fixture, 3);
    let mut second = settled(&fixture, prepared(&fixture, Some(first.selection), 10));
    assert_eq!(second.selection.thread_id(), thread(4));
    fixture.faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(fixture.store.home_revision().is_err());
    let mut recovery = fixture.store.recover_same_home().unwrap();
    let fresh = BerylState::reacquire_candidate(&recovery).unwrap();
    let access = recovery.recovery_access().unwrap();
    let revision = access.home_revision().unwrap();
    for _ in 0..2 {
        assert!(second.validate_candidate(&access, &fresh).is_ok());
    }
    assert!(first.validate_candidate(&access, &fresh).is_err());
    second.receipt = first.receipt;
    assert!(second.validate_candidate(&access, &fresh).is_err());
    assert_eq!(access.home_revision().unwrap(), revision);
    let store = recovery.publish().unwrap();
    assert_eq!(store.home_revision().unwrap(), revision);
}

#[test]
fn never_admitted_candidate_retirement_proves_only_original_source_and_writes_nothing() {
    let fixture = Fixture::new();
    let retired = prepared(&fixture, None, 9).retire_unadmitted();
    fixture.faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(fixture.store.home_revision().is_err());
    let mut recovery = fixture.store.recover_same_home().unwrap();
    let fresh = BerylState::reacquire_candidate(&recovery).unwrap();
    let access = recovery.recovery_access().unwrap();
    let revision = access.home_revision().unwrap();
    assert!(retired.qualify_candidate_original(&access, &fresh).is_ok());
    assert!(
        retired
            .qualify_candidate_original(&access, &fixture.state)
            .is_err()
    );
    assert!(
        fresh
            .catalog()
            .current_row_source_candidate(
                &access,
                thread(9),
                CatalogPointReadLimit::schema_maximum()
            )
            .unwrap()
            .is_none()
    );
    assert_eq!(access.home_revision().unwrap(), revision);
    let store = recovery.publish().unwrap();
    assert_eq!(store.home_revision().unwrap(), revision);
}

#[test]
fn candidate_recovery_retains_terminal_collision_when_later_window_sources_changed() {
    let fixture = Fixture::new();
    let original = prepared(&fixture, None, 9);
    let future = original.future_window().clone();
    fixture
        .faults
        .fail_next(FaultPoint::AfterCommitBeforePersist);
    let SameWindowThreadOutcome::Pending(pending) = original.commit(&fixture.store, &fixture.state)
    else {
        panic!("expected pending")
    };
    let header = fixture
        .state
        .session()
        .minimal_bootstrap(&fixture.store)
        .unwrap()
        .unwrap()
        .header()
        .revision();
    execute(
        &fixture.store,
        fixture.state.session().update_placement(
            fixture.state.session().revision(&fixture.store).unwrap(),
            UpdateWindowPlacement::new(
                header,
                fixture.window,
                future.revision(),
                WindowPlacement::new(
                    WindowBounds::new(1, 0, 900, 700).unwrap(),
                    WindowDisplayState::Normal,
                    None,
                    None,
                ),
            ),
        ),
    );
    fixture.faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(fixture.store.home_revision().is_err());
    let mut recovery = fixture.store.recover_same_home().unwrap();
    let fresh = BerylState::reacquire_candidate(&recovery).unwrap();
    let access = recovery.recovery_access().unwrap();
    let revision = access.home_revision().unwrap();
    let SameWindowThreadOutcome::Unavailable(pending) =
        pending.reconcile_candidate(&access, &fresh)
    else {
        panic!("changed sources must collide")
    };
    assert!(matches!(
        pending.reconcile_candidate(&access, &fresh),
        SameWindowThreadOutcome::Unavailable(_)
    ));
    assert_eq!(access.home_revision().unwrap(), revision);
    let store = recovery.publish().unwrap();
    assert_eq!(store.home_revision().unwrap(), revision);
}
