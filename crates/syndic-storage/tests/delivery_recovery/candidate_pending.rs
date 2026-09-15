use beryl_home_store::{
    CommandOutcome, HomeCommand, HomeOpenCandidate, HomeOpenOptions, HomeSchemaVersion,
    test_faults::{FaultController, FaultPoint},
};
use syndic_storage::{
    SyndicPointReadLimit, SyndicReadError, SyndicStorage, TurnDispatchProvenance,
    test_faults::{
        FixtureBatch, FixtureDelete, pending_dispatch_evidence_candidate_with_confirmation_hook,
    },
};

use crate::{
    recovery_support::{
        activate, cancel_active, ordered_id, pending_home, point_limit, publish_stale_valid,
    },
    support::open,
};

#[test]
fn candidate_pending_evidence_preserves_identity_provenance_and_publication_parity() {
    for cancelled in [false, true] {
        let fixture = pending_home("candidate-pending", 900 + u64::from(cancelled));
        if cancelled {
            let active = activate(
                &fixture.store,
                fixture.storage.clone(),
                fixture.thread,
                fixture.turn,
                false,
            );
            cancel_active(
                &fixture.store,
                fixture.storage.clone(),
                fixture.thread,
                fixture.turn,
                active.snapshot,
            );
            publish_stale_valid(&fixture.store, fixture.storage.clone(), fixture.thread);
        }
        let original = fixture
            .storage
            .pending_dispatch_evidence(&fixture.store, fixture.thread, point_limit())
            .unwrap()
            .unwrap();
        assert_eq!(
            matches!(
                original.dispatch_provenance(),
                TurnDispatchProvenance::Cancelled(_)
            ),
            cancelled
        );
        fixture.store.close().unwrap();
        let faults = FaultController::new();
        let mut candidate = HomeOpenCandidate::open_with_faults(
            HomeOpenOptions::new(fixture.home.path(), HomeSchemaVersion::CURRENT),
            faults.clone(),
        )
        .unwrap();
        let storage = SyndicStorage::register(&mut candidate).unwrap();
        let mut publication = candidate
            .prepare_publication(SyndicStorage::required_domains().unwrap())
            .unwrap();
        let access = publication.recovery_access().unwrap();
        let pending = storage
            .pending_dispatch_evidence_candidate(&access, fixture.thread, point_limit())
            .unwrap()
            .unwrap();
        assert_eq!(pending.input(), original.input());
        assert_eq!(pending.turn_id(), original.turn_id());
        assert_eq!(pending.item_id(), original.item_id());
        assert_eq!(
            pending.dispatch_provenance(),
            original.dispatch_provenance()
        );
        assert!(
            fixture
                .storage
                .pending_dispatch_evidence_candidate(&access, fixture.thread, point_limit())
                .is_err()
        );
        assert_eq!(
            storage
                .pending_dispatch_evidence_candidate(&access, ordered_id(9999), point_limit())
                .unwrap(),
            None
        );
        assert!(
            storage
                .pending_dispatch_evidence_candidate(
                    &access,
                    fixture.thread,
                    SyndicPointReadLimit::new(1).unwrap()
                )
                .is_err()
        );
        beryl_home_store::test_faults::with_initial_publication_store(&publication, |store| {
            assert!(
                storage
                    .pending_dispatch_evidence(store, fixture.thread, point_limit())
                    .is_err()
            );
        });
        let store = publication.publish().unwrap();
        assert_eq!(
            storage
                .pending_dispatch_evidence(&store, fixture.thread, point_limit())
                .unwrap(),
            Some(pending)
        );
        faults.fail_next(FaultPoint::BeforeReadConfirmation);
        assert!(store.home_revision().is_err());
        let mut recovered = store.recover_same_home().unwrap();
        let fresh = SyndicStorage::reacquire_candidate(&recovered).unwrap();
        let access = recovered.recovery_access().unwrap();
        assert!(
            storage
                .pending_dispatch_evidence_candidate(&access, fixture.thread, point_limit())
                .is_err()
        );
        let fresh_pending = fresh
            .pending_dispatch_evidence_candidate(&access, fixture.thread, point_limit())
            .unwrap()
            .unwrap();
        assert_eq!(fresh_pending.input(), pending.input());
        assert_eq!(
            fresh_pending.dispatch_provenance(),
            pending.dispatch_provenance()
        );
        assert_ne!(fresh_pending.home_generation(), pending.home_generation());
        let store = recovered.publish().unwrap();
        assert_eq!(
            fresh
                .pending_dispatch_evidence(&store, fixture.thread, point_limit())
                .unwrap(),
            Some(fresh_pending)
        );
        store.close().unwrap();
    }
}

#[test]
fn candidate_pending_evidence_distinguishes_mutating_from_missing_input() {
    for concurrent in [false, true] {
        let fixture = pending_home("candidate-pending-input", 910 + u64::from(concurrent));
        let original = fixture
            .storage
            .pending_dispatch_evidence(&fixture.store, fixture.thread, point_limit())
            .unwrap()
            .unwrap();
        fixture.store.close().unwrap();
        let mut candidate = open(fixture.home.path());
        let storage = SyndicStorage::register(&mut candidate).unwrap();
        let mut publication = candidate
            .prepare_publication(SyndicStorage::required_domains().unwrap())
            .unwrap();
        let access = publication.recovery_access().unwrap();
        let remove = || {
            let mut batch = FixtureBatch::new();
            batch
                .delete(FixtureDelete::ContentManifest(original.input().id()))
                .unwrap();
            let mut command = HomeCommand::new(access.home_revision().unwrap());
            command
                .add(
                    storage
                        .clone()
                        .fixture_contribution(original.source_revision(), batch),
                )
                .unwrap();
            assert!(matches!(
                access.execute(command),
                CommandOutcome::Committed { .. }
            ));
        };
        let result = if concurrent {
            pending_dispatch_evidence_candidate_with_confirmation_hook(
                &storage,
                &access,
                fixture.thread,
                point_limit(),
                remove,
            )
        } else {
            remove();
            storage.pending_dispatch_evidence_candidate(&access, fixture.thread, point_limit())
        };
        if concurrent {
            assert!(
                matches!(result, Err(SyndicReadError::ConcurrentChange { .. })),
                "{result:?}"
            );
        } else {
            assert!(
                matches!(result, Err(SyndicReadError::Invariant(_))),
                "{result:?}"
            );
        }
        publication.close().unwrap();
    }
}
