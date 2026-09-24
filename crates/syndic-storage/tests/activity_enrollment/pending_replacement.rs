use super::*;
use beryl_home_store::{HomeCommand, WholeHomeScrubTrigger};

fn fingerprint(f: &Fixture) -> ActivityRetirementFingerprint {
    f.storage
        .activity_retirement_fingerprint(&f.store, ActivityQuerySource::new(id(30), f.turn))
        .unwrap()
        .unwrap()
}

fn prepare(f: &Fixture) -> PreparedActivityEnrollment {
    match f
        .storage
        .prepare_activity_enrollment(
            &f.store,
            f.request(None).replace_retired_pending(fingerprint(f)),
        )
        .unwrap()
    {
        ActivityEnrollmentPreparation::Prepared(prepared) => prepared,
        _ => panic!("fresh replacement"),
    }
}

fn cancel(f: &Fixture, snapshot: beryl_model::SyndicExecutionSnapshotId) {
    let binding = f
        .storage
        .current_binding(&f.store, id(30), limit())
        .unwrap()
        .unwrap();
    let gate = f
        .storage
        .input_gate(&f.store, id(30), limit())
        .unwrap()
        .unwrap();
    let state = f
        .storage
        .turn_state(&f.store, f.turn, limit())
        .unwrap()
        .unwrap();
    let request = CancelBindingActivation::new(
        id(30),
        binding.binding().revision(),
        gate.revision(),
        state.revision(),
        binding.binding().selected_path(),
        snapshot,
        f.turn,
    );
    let mut command = HomeCommand::new(f.store.home_revision().unwrap());
    command
        .add(
            f.storage
                .cancel_binding_activation(f.storage.revision(&f.store).unwrap(), request),
        )
        .unwrap();
    assert!(matches!(
        f.store.execute(command),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
}

#[test]
fn retired_pending_replacement_preserves_unattempted_and_cancelled_canonical_authority() {
    for cancelled in [false, true] {
        let f = Fixture::new();
        let old = f.enroll();
        if cancelled {
            let (_, snapshot) = support::exact_cas::activate_turn(
                &f.store,
                f.storage.clone(),
                id(30),
                f.turn,
                timestamp(101),
            );
            cancel(&f, snapshot);
        }
        let pending = f
            .storage
            .pending_dispatch_evidence(&f.store, id(30), limit())
            .unwrap()
            .unwrap();
        let turn = f.storage.turn(&f.store, f.turn, limit()).unwrap();
        let state = f.storage.turn_state(&f.store, f.turn, limit()).unwrap();
        let input = f
            .storage
            .canonical_item(&f.store, pending.item_id(), limit())
            .unwrap();
        let gate = f.storage.input_gate(&f.store, id(30), limit()).unwrap();
        assert!(
            f.storage
                .prepare_activity_enrollment(&f.store, f.request(None))
                .is_err()
        );
        assert!(
            f.storage
                .prepare_activity_enrollment(
                    &f.store,
                    f.request(Some(&old))
                        .replace_retired_pending(fingerprint(&f))
                )
                .is_err()
        );
        let (command, witness) = prepare(&f).into_command();
        assert!(matches!(
            f.store.execute(command),
            CommandOutcome::Committed {
                later_failure: None,
                ..
            }
        ));
        let ActivityEnrollmentStatus::Committed { token: Some(new) } = f
            .storage
            .activity_enrollment_status(&f.store, &witness)
            .unwrap()
        else {
            panic!("committed fresh token")
        };
        assert!(new.work_period() > old.work_period());
        assert_eq!(
            f.head().source(),
            Some(ActivityQuerySource::new(id(30), f.turn))
        );
        assert_eq!(f.head().logical_row_count(), 0);
        assert_eq!(f.storage.turn(&f.store, f.turn, limit()).unwrap(), turn);
        assert_eq!(
            f.storage.turn_state(&f.store, f.turn, limit()).unwrap(),
            state
        );
        assert_eq!(
            f.storage
                .canonical_item(&f.store, pending.item_id(), limit())
                .unwrap(),
            input
        );
        assert_eq!(
            f.storage.input_gate(&f.store, id(30), limit()).unwrap(),
            gate
        );
        let after = f
            .storage
            .pending_dispatch_evidence(&f.store, id(30), limit())
            .unwrap()
            .unwrap();
        assert_eq!(after.dispatch_provenance(), pending.dispatch_provenance());
        assert_eq!(after.input(), pending.input());
        f.store
            .scrub_whole_home(WholeHomeScrubTrigger::Explicit)
            .unwrap();
    }
}

#[test]
fn replacement_rejects_substitution_possible_dispatch_and_changed_preparation() {
    let f = Fixture::new();
    let old = f.enroll();
    for active in [false, true] {
        commit(
            &f.store,
            f.storage.clone(),
            batch([FixtureRecord::ActivityQuerySource(
                ActivityQuerySourceRecord::new(
                    id(30),
                    old.work_period(),
                    ActivityQuerySource::new(id(30), f.turn),
                    None,
                    0,
                    active,
                    None,
                ),
            )]),
        );
        if !active {
            assert!(
                f.storage
                    .prepare_activity_enrollment(
                        &f.store,
                        f.request(None).replace_retired_pending(fingerprint(&f))
                    )
                    .is_err()
            );
        }
    }
    let foreign = ActivityPeriodToken::for_fixture(
        beryl_model::BerylHomeId::from_bytes([249; 16]),
        old.runtime_id(),
        old.work_period(),
    );
    for wrong in [
        ActivityRetirementFingerprint::from_enrollment(
            &foreign,
            ActivityQuerySource::new(id(30), f.turn),
        ),
        ActivityRetirementFingerprint::from_enrollment(
            &old,
            ActivityQuerySource::new(id(30), SyndicTurnId::from_bytes([250; 16])),
        ),
    ] {
        assert!(
            f.storage
                .prepare_activity_enrollment(
                    &f.store,
                    f.request(None).replace_retired_pending(wrong)
                )
                .is_err()
        );
    }
    let (command, witness) = prepare(&f).into_command();
    let (_, snapshot) = support::exact_cas::activate_turn(
        &f.store,
        f.storage.clone(),
        id(30),
        f.turn,
        timestamp(101),
    );
    assert!(matches!(
        f.store.execute(command),
        CommandOutcome::NotCommitted { .. }
    ));
    assert!(matches!(
        f.storage
            .activity_enrollment_status(&f.store, &witness)
            .unwrap(),
        ActivityEnrollmentStatus::NotCommitted
    ));
    assert!(
        f.storage
            .prepare_activity_enrollment(
                &f.store,
                f.request(None).replace_retired_pending(fingerprint(&f))
            )
            .is_err()
    );
    cancel(&f, snapshot);
    let _ = prepare(&f);
    let mut corrupt = syndic_storage::test_faults::FixtureBatch::new();
    corrupt
        .delete(syndic_storage::test_faults::FixtureDelete::ExecutionSnapshot(snapshot))
        .unwrap();
    commit(&f.store, f.storage.clone(), corrupt);
    assert!(
        f.storage
            .prepare_activity_enrollment(
                &f.store,
                f.request(None).replace_retired_pending(fingerprint(&f))
            )
            .is_err()
    );
}

#[test]
fn replacement_uncertainty_preserves_exact_old_or_new_enrollment() {
    for point in [
        FaultPoint::BeforeCommit,
        FaultPoint::AfterCommitBeforePersist,
    ] {
        let f = Fixture::new();
        let old = f.enroll();
        let (command, witness) = prepare(&f).into_command();
        f.faults.fail_next(point);
        match f.store.execute(command) {
            CommandOutcome::NotCommitted { .. } if point == FaultPoint::BeforeCommit => {
                let mut candidate = f.store.recover_same_home().unwrap();
                let fresh = SyndicStorage::reacquire_candidate(&candidate).unwrap();
                let access = candidate.recovery_access().unwrap();
                assert!(matches!(
                    fresh
                        .activity_enrollment_status_candidate(&access, &witness)
                        .unwrap(),
                    ActivityEnrollmentStatus::NotCommitted
                ));
                let recovered = candidate.publish().unwrap();
                recovered
                    .scrub_whole_home(WholeHomeScrubTrigger::Explicit)
                    .unwrap();
                recovered.close().unwrap();
                continue;
            }
            CommandOutcome::Indeterminate { reconciliation, .. } => {
                let handle = reconciliation.install_and_handle();
                assert!(matches!(
                    f.store.retry_reconciliation(&handle).unwrap(),
                    beryl_home_store::ReconciliationResolution::ExactNew { .. }
                ));
                let ActivityEnrollmentStatus::Committed { token: Some(new) } = f
                    .storage
                    .activity_enrollment_status(&f.store, &witness)
                    .unwrap()
                else {
                    panic!("exact new replacement")
                };
                assert!(new.work_period() > old.work_period());
            }
            _ => panic!("expected fault outcome"),
        }
        f.store
            .scrub_whole_home(WholeHomeScrubTrigger::Explicit)
            .unwrap();
    }
}

#[test]
fn pending_replacement_preserves_completed_rows_from_the_ended_runtime() {
    let mut f = Fixture::new();
    let old = f.enroll();
    retention::retire(&mut f, 0);
    let (command, _) = f.prepare(Some(&old)).into_command();
    assert!(matches!(
        f.store.execute(command),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    assert_eq!(f.head().source_count(), 2);
    assert_eq!(f.head().completed_row_count(), 1);
    let limits = beryl_home_store::CursorReadLimits::new(32, 65_536).unwrap();
    let before = f
        .storage
        .fixture_activity_query_entry_count(&f.store, id(30), old.work_period(), limits)
        .unwrap();
    assert_eq!(before.0, 1);
    let (command, _) = prepare(&f).into_command();
    assert!(matches!(
        f.store.execute(command),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    assert_eq!(f.head().logical_row_count(), 0);
    assert!(f.head().work_period() > old.work_period());
    assert_eq!(
        f.storage
            .fixture_activity_query_entry_count(&f.store, id(30), old.work_period(), limits)
            .unwrap(),
        before
    );
    f.store
        .scrub_whole_home(WholeHomeScrubTrigger::Explicit)
        .unwrap();
}
