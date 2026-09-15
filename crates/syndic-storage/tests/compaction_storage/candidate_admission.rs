use super::compaction_support::{CompactionFixture, open_candidate, point_limit};
use beryl_home_store::{
    CommandOutcome, HomeCommand,
    test_faults::{FaultController, FaultPoint},
};
use beryl_model::SyndicThreadId;
use std::{thread, time::Duration};
use syndic_storage::{
    BindingState, CompactionAdmissionIneligibility, CompactionAdmissionRead, InputGateRecord,
    SyndicPointReadLimit, SyndicReadError, SyndicStorage,
    test_faults::{FixtureBatch, FixtureDelete, FixtureRecord},
};

#[test]
fn candidate_compaction_admission_preserves_current_authority_and_publication_parity() {
    let foreign = CompactionFixture::new("candidate-admission-foreign", 189);
    for existing in [false, true] {
        let fixture =
            CompactionFixture::new("candidate-compaction-admission", 190 + u8::from(existing));
        if existing {
            fixture.admit(192, 10);
        }
        let expected = fixture
            .storage
            .compaction_admission_read(&fixture.store, fixture.thread, point_limit())
            .unwrap();
        assert_eq!(
            matches!(expected, CompactionAdmissionRead::Existing(_)),
            existing
        );
        fixture.store.close().unwrap();
        let faults = FaultController::new();
        let (mut publication, storage) = open_candidate(fixture.home.path(), faults.clone());
        let access = publication.recovery_access().unwrap();
        assert_eq!(
            storage
                .compaction_admission_read_candidate(&access, fixture.thread, point_limit())
                .unwrap(),
            expected
        );
        for stale in [&foreign.storage, &fixture.storage] {
            assert!(
                stale
                    .compaction_admission_read_candidate(&access, fixture.thread, point_limit())
                    .is_err()
            );
        }
        assert!(matches!(
            storage
                .compaction_admission_read_candidate(
                    &access,
                    SyndicThreadId::from_bytes([188; 16]),
                    point_limit()
                )
                .unwrap(),
            CompactionAdmissionRead::Ineligible(CompactionAdmissionIneligibility::MissingThread)
        ));
        assert!(
            storage
                .compaction_admission_read_candidate(
                    &access,
                    fixture.thread,
                    SyndicPointReadLimit::new(1).unwrap()
                )
                .is_err()
        );
        beryl_home_store::test_faults::with_initial_publication_store(&publication, |store| {
            assert!(
                storage
                    .compaction_admission_read(store, fixture.thread, point_limit())
                    .is_err()
            );
        });
        let store = publication.publish().unwrap();
        assert_eq!(
            storage
                .compaction_admission_read(&store, fixture.thread, point_limit())
                .unwrap(),
            expected
        );
        faults.fail_next(FaultPoint::BeforeReadConfirmation);
        assert!(store.home_revision().is_err());
        let mut recovered = store.recover_same_home().unwrap();
        let fresh = SyndicStorage::reacquire_candidate(&recovered).unwrap();
        let access = recovered.recovery_access().unwrap();
        assert!(
            storage
                .compaction_admission_read_candidate(&access, fixture.thread, point_limit())
                .is_err()
        );
        assert_eq!(
            fresh
                .compaction_admission_read_candidate(&access, fixture.thread, point_limit())
                .unwrap(),
            expected
        );
        recovered.publish().unwrap().close().unwrap();
    }
    foreign.store.close().unwrap();
}

#[test]
fn candidate_compaction_admission_rejects_missing_owner_or_selected_operation() {
    for existing in [false, true] {
        let fixture =
            CompactionFixture::new("candidate-admission-missing", 194 + u8::from(existing));
        let deletion = if existing {
            FixtureDelete::CompactionOperation(fixture.admit(196, 10))
        } else {
            let BindingState::Valid(binding) = fixture.binding_state() else {
                panic!("valid fixture binding required")
            };
            FixtureDelete::CasThread(binding.cas_thread_id().clone())
        };
        let mut changes = FixtureBatch::new();
        changes.delete(deletion).unwrap();
        crate::support::commit(&fixture.store, fixture.storage.clone(), changes);
        fixture.store.close().unwrap();
        let (mut publication, storage) =
            open_candidate(fixture.home.path(), FaultController::new());
        assert!(matches!(
            storage.compaction_admission_read_candidate(
                &publication.recovery_access().unwrap(),
                fixture.thread,
                point_limit()
            ),
            Err(SyndicReadError::Invariant(_))
        ));
        publication.close().unwrap();
    }
}

#[test]
fn candidate_compaction_admission_rejects_a_gate_change_between_passes() {
    let fixture = CompactionFixture::new("candidate-admission-drift", 197);
    let gate = fixture.gate();
    let revision = fixture.storage.revision(&fixture.store).unwrap();
    fixture.store.close().unwrap();
    let faults = FaultController::new();
    let (mut publication, storage) = open_candidate(fixture.home.path(), faults.clone());
    let access = publication.recovery_access().unwrap();
    let replacement = InputGateRecord::new(
        gate.thread_id(),
        gate.revision().checked_next().unwrap(),
        gate.state().clone(),
        gate.accepted_high_water(),
        gate.route_generation_high_water(),
        gate.selected_route(),
        gate.live_steering_count(),
        gate.live_next_turn_count(),
        gate.live_logical_utf8_bytes(),
    )
    .unwrap();
    let mut command = HomeCommand::new(access.home_revision().unwrap());
    command
        .add(storage.clone().fixture_contribution(
            revision,
            crate::support::batch([FixtureRecord::InputGate(replacement)]),
        ))
        .unwrap();
    let blocks = (0..8)
        .map(|_| faults.block_next(FaultPoint::BeforeReadConfirmation))
        .collect::<Vec<_>>();
    for block in &blocks[..7] {
        block.release();
    }
    let result = thread::scope(|scope| {
        let reader = scope.spawn(|| {
            storage.compaction_admission_read_candidate(&access, fixture.thread, point_limit())
        });
        assert!(blocks[7].wait_until_reached(Duration::from_secs(10)));
        assert!(matches!(
            access.execute(command),
            CommandOutcome::Committed {
                later_failure: None,
                ..
            }
        ));
        blocks[7].release();
        reader.join().unwrap()
    });
    assert!(
        matches!(result, Err(SyndicReadError::ConcurrentChange { .. })),
        "{result:?}"
    );
    publication.close().unwrap();
}
