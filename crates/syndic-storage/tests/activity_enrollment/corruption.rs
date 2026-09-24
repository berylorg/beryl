use super::*;

#[test]
fn miskeyed_turn_and_state_cannot_authenticate_another_source() {
    let f = Fixture::new();
    let turn = f.storage.turn(&f.store, f.turn, limit()).unwrap().unwrap();
    let state = f
        .storage
        .turn_state(&f.store, f.turn, limit())
        .unwrap()
        .unwrap();
    let alias = SyndicTurnId::from_bytes([225; 16]);
    let mut command = beryl_home_store::HomeCommand::new(f.store.home_revision().unwrap());
    command
        .add(
            syndic_storage::test_faults::turn_alias_contribution_for_test(
                &f.store, &f.storage, alias, turn, state,
            ),
        )
        .unwrap();
    assert!(matches!(
        f.store.execute(command),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    let execution = f
        .storage
        .thread_execution(&f.store, id(30), limit())
        .unwrap()
        .unwrap()
        .execution()
        .clone();
    let request = ActivityEnrollmentRequest::first(
        ActivityQuerySource::new(id(30), alias),
        execution,
        f.head().revision(),
    );
    assert!(
        f.storage
            .prepare_activity_enrollment(&f.store, request)
            .is_err()
    );
}

#[test]
fn occupied_future_period_and_impossible_stored_period_are_rejected() {
    let f = Fixture::new();
    let future = ActivityWorkPeriod::new(f.store.home_revision().unwrap().get() + 2).unwrap();
    let member = ActivityQuerySourceRecord::new(
        id(30),
        future,
        ActivityQuerySource::new(id(30), f.turn),
        None,
        0,
        true,
        None,
    );
    commit(
        &f.store,
        f.storage.clone(),
        batch([FixtureRecord::ActivityQuerySource(member)]),
    );
    assert!(
        f.storage
            .prepare_activity_enrollment(&f.store, f.request(None))
            .is_err()
    );
    let invalid = ActivityQueryHeadRecord::new(
        id(30),
        ActivityWorkPeriod::new(u64::MAX).unwrap(),
        None,
        false,
        0,
        f.head().revision().checked_next().unwrap(),
        0,
        0,
        0,
        0,
        0,
        None,
        ProjectionLifecycle::Current,
    )
    .unwrap();
    commit(
        &f.store,
        f.storage.clone(),
        batch([FixtureRecord::ActivityQueryHead(invalid)]),
    );
    assert!(
        f.storage
            .prepare_activity_enrollment(&f.store, f.request(None))
            .is_err()
    );
}

#[test]
fn missing_committed_membership_is_conflict_not_success() {
    let f = Fixture::new();
    let (command, witness) = f.prepare(None).into_command();
    assert!(matches!(
        f.store.execute(command),
        CommandOutcome::Committed { .. }
    ));
    let mut deletion = syndic_storage::test_faults::FixtureBatch::new();
    deletion
        .delete(
            syndic_storage::test_faults::FixtureDelete::ActivityQuerySource {
                thread: id(30),
                work_period: f.head().work_period(),
                source_thread: id(30),
                source_turn: f.turn,
            },
        )
        .unwrap();
    commit(&f.store, f.storage.clone(), deletion);
    assert!(matches!(
        f.storage
            .activity_enrollment_status(&f.store, &witness)
            .unwrap(),
        ActivityEnrollmentStatus::Conflict
    ));
}
