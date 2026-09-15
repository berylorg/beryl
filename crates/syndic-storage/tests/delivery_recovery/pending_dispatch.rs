use syndic_storage::test_faults::{
    FixtureBatch, FixtureDelete, pending_dispatch_evidence_with_confirmation_hook,
};
use syndic_storage::{
    PendingDispatchEvidence, SyndicReadError, SyndicStorage, TurnDispatchProvenance,
};

use crate::{
    recovery_support::{
        RecoveryHome, activate, cancel_active, pending_home, point_limit, publish_stale_valid,
    },
    support::{commit, open},
};

fn proof(fixture: &RecoveryHome) -> PendingDispatchEvidence {
    fixture
        .storage
        .pending_dispatch_evidence(&fixture.store, fixture.thread, point_limit())
        .unwrap()
        .unwrap()
}

#[test]
fn exact_pending_identity_survives_cancellation_retirement_and_reopen() {
    let fixture = pending_home("pending-dispatch-preservation", 710);
    let untouched = proof(&fixture);
    assert_eq!(
        untouched.dispatch_provenance(),
        TurnDispatchProvenance::Unattempted
    );
    assert_eq!(untouched.home_id(), fixture.store.home_id());
    assert_eq!(
        untouched.source_revision(),
        fixture.storage.revision(&fixture.store).unwrap()
    );
    let active = activate(
        &fixture.store,
        fixture.storage.clone(),
        fixture.thread,
        fixture.turn,
        false,
    );
    assert_eq!(
        fixture
            .storage
            .pending_dispatch_evidence(&fixture.store, fixture.thread, point_limit())
            .unwrap(),
        None
    );
    cancel_active(
        &fixture.store,
        fixture.storage.clone(),
        fixture.thread,
        fixture.turn,
        active.snapshot,
    );
    let cancelled = proof(&fixture);
    assert!(matches!(
        cancelled.dispatch_provenance(),
        TurnDispatchProvenance::Cancelled(_)
    ));
    assert_ne!(cancelled.state_revision(), untouched.state_revision());
    publish_stale_valid(&fixture.store, fixture.storage.clone(), fixture.thread);
    let retired = proof(&fixture);
    for evidence in [cancelled, retired] {
        assert_eq!(evidence.turn_id(), untouched.turn_id());
        assert_eq!(evidence.item_id(), untouched.item_id());
        assert_eq!(evidence.item_revision(), untouched.item_revision());
        assert_eq!(evidence.input(), untouched.input());
        assert_eq!(
            evidence.asset_reference_set(),
            untouched.asset_reference_set()
        );
        assert_eq!(evidence.selected_path(), untouched.selected_path());
    }
    fixture.store.close().unwrap();
    let mut reopened_candidate = open(fixture.home.path());
    let storage = SyndicStorage::register(&mut reopened_candidate).unwrap();
    let reopened = reopened_candidate
        .prepare_publication(SyndicStorage::required_domains().unwrap())
        .unwrap()
        .publish()
        .unwrap();
    let recovered = storage
        .pending_dispatch_evidence(&reopened, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(recovered.home_id(), untouched.home_id());
    assert_eq!(
        recovered.dispatch_provenance(),
        retired.dispatch_provenance()
    );
    assert_eq!(recovered.input(), untouched.input());
    reopened.close().unwrap();
}

#[test]
fn missing_canonical_anchors_are_corruption_but_mutating_anchors_are_drift() {
    for concurrent in [false, true] {
        for anchor in 0..5 {
            let fixture = pending_home("pending-dispatch-anchor", 720 + anchor);
            let pending = proof(&fixture);
            let source =
                crate::recovery_support::startup_source(&fixture.store, fixture.storage.clone());
            let deletion = match anchor {
                0 => FixtureDelete::Thread(fixture.thread),
                1 => FixtureDelete::TurnState(fixture.turn),
                2 => FixtureDelete::CanonicalItem(pending.item_id()),
                3 => FixtureDelete::ContentManifest(pending.input().id()),
                _ => FixtureDelete::HistorySummary(fixture.thread),
            };
            let remove = || {
                let mut changes = FixtureBatch::new();
                changes.delete(deletion).unwrap();
                commit(&fixture.store, fixture.storage.clone(), changes);
            };
            let result = if concurrent {
                pending_dispatch_evidence_with_confirmation_hook(
                    &fixture.storage,
                    &fixture.store,
                    fixture.thread,
                    point_limit(),
                    remove,
                )
            } else {
                remove();
                fixture.storage.pending_dispatch_evidence(
                    &fixture.store,
                    fixture.thread,
                    point_limit(),
                )
            };
            if concurrent {
                assert!(
                    matches!(result, Err(SyndicReadError::ConcurrentChange { .. })),
                    "anchor {anchor}: {result:?}"
                );
            } else {
                assert!(
                    matches!(result, Err(SyndicReadError::Invariant(_))),
                    "anchor {anchor}: {result:?}"
                );
            }
            assert!(matches!(
                fixture
                    .storage
                    .classify_delivery_recovery(&fixture.store, &source, point_limit()),
                Err(syndic_storage::DeliveryRecoveryClassificationError::Corruption(_))
            ));
            let limits = beryl_home_store::CursorReadLimits::new(
                1,
                syndic_storage::DELIVERY_RECOVERY_GATE_PAGE_MAX_BYTES,
            )
            .unwrap();
            assert!(matches!(
                fixture.storage.recovered_pending_page(
                    &fixture.store,
                    fixture.storage.revision(&fixture.store).unwrap(),
                    None,
                    limits,
                    point_limit()
                ),
                Err(SyndicReadError::Invariant(_))
            ));
            assert!(matches!(
                fixture.storage.recovered_pending_page(
                    &fixture.store,
                    pending.source_revision(),
                    None,
                    limits,
                    point_limit()
                ),
                Err(SyndicReadError::StaleRecoveredPendingScan)
            ));
        }
    }
}

#[test]
fn foreign_home_cannot_supply_pending_dispatch_evidence() {
    let first = pending_home("pending-dispatch-first-home", 730);
    let foreign = pending_home("pending-dispatch-foreign-home", 730);
    assert!(
        first
            .storage
            .pending_dispatch_evidence(&foreign.store, foreign.thread, point_limit())
            .is_err()
    );
    let first_proof = proof(&first);
    let foreign_proof = proof(&foreign);
    assert_ne!(first_proof.home_id(), foreign_proof.home_id());
    assert_ne!(first_proof, foreign_proof);
}

#[test]
fn ordinary_pending_turn_with_provider_marker_is_corruption() {
    let fixture = pending_home("pending-dispatch-kind-marker", 734);
    let source = crate::recovery_support::startup_source(&fixture.store, fixture.storage.clone());
    let state = fixture
        .storage
        .turn_state(&fixture.store, fixture.turn, point_limit())
        .unwrap()
        .unwrap();
    let invalid = syndic_storage::TurnStateRecord::with_capture_frontiers_and_issue(
        state.turn_id(),
        state.revision(),
        state.lifecycle(),
        state.source_event_count(),
        state.item_count(),
        state.finalized_item_count(),
        state.open_item_count(),
        state.history_blocking_item_count(),
        state.provider_observation_issue(),
        state.end_status(),
        state.updated_at(),
        TurnDispatchProvenance::ProviderOperation,
    )
    .unwrap();
    commit(
        &fixture.store,
        fixture.storage.clone(),
        crate::support::batch([syndic_storage::test_faults::FixtureRecord::TurnState(
            invalid,
        )]),
    );
    assert!(matches!(
        fixture
            .storage
            .pending_dispatch_evidence(&fixture.store, fixture.thread, point_limit()),
        Err(SyndicReadError::Invariant(_))
    ));
    assert!(matches!(
        fixture
            .storage
            .classify_delivery_recovery(&fixture.store, &source, point_limit()),
        Err(syndic_storage::DeliveryRecoveryClassificationError::Corruption(_))
    ));
}

#[test]
fn same_home_recovery_invalidates_the_old_storage_generation() {
    let fixture = pending_home("pending-dispatch-recovered-generation", 731);
    let before = proof(&fixture);
    let orphan = crate::recovery_support::ordered_id(732);
    commit(
        &fixture.store,
        fixture.storage.clone(),
        crate::support::batch([
            syndic_storage::test_faults::FixtureRecord::NonIdleGateSource {
                thread_id: orphan,
                source: syndic_storage::NonIdleGateSourceRecord::new(
                    orphan,
                    beryl_model::InputGateRevision::new(1).unwrap(),
                ),
            },
        ]),
    );
    assert!(
        fixture
            .store
            .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
            .is_err()
    );
    let candidate = fixture.store.recover_same_home().unwrap();
    let storage = SyndicStorage::reacquire_candidate(&candidate).unwrap();
    let recovered = candidate.publish();
    let after = storage
        .pending_dispatch_evidence(&recovered, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(before.home_id(), after.home_id());
    assert_ne!(before.home_generation(), after.home_generation());
    assert_eq!(before.input(), after.input());
    assert!(
        fixture
            .storage
            .pending_dispatch_evidence(&recovered, fixture.thread, point_limit())
            .is_err()
    );
    recovered.close().unwrap();
}
