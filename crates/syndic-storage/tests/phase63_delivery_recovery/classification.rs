use syndic_storage::test_faults::FixtureRecord;
use syndic_storage::{
    BindingState, CompactionOperationNonce, DELIVERY_RECOVERY_GATE_PAGE_MAX_BYTES,
    DeliveryRecoveryCase, DeliveryRecoveryClassificationError, HistorySummaryRecord,
    InputGateState, SyndicReadError, TurnLifecycle,
};

use crate::{
    recovery_support::{
        activate, cancel_active, execute, pending_home, point_limit, publish_stale_valid,
        replace_gate_state, startup_source,
    },
    support::{batch, commit, fixture_turn_state, timestamp},
};

#[test]
fn safe_pending_accepts_unbound_valid_and_stale_non_active_bindings() {
    let unbound = pending_home("phase63-safe-pending-unbound", 401);
    assert!(matches!(
        unbound
            .storage.clone()
            .classify_delivery_recovery(
                &unbound.store,
                &startup_source(&unbound.store, unbound.storage.clone()),
                point_limit(),
            )
            .unwrap(),
        DeliveryRecoveryCase::Pending {
            thread_id,
            turn_id,
            minimum_timestamp,
        } if thread_id == unbound.thread
            && turn_id == unbound.turn
            && minimum_timestamp == timestamp(3)
    ));

    let valid = pending_home("phase63-safe-pending-valid", 402);
    let active = activate(
        &valid.store,
        valid.storage.clone(),
        valid.thread,
        valid.turn,
        false,
    );
    cancel_active(
        &valid.store,
        valid.storage.clone(),
        valid.thread,
        valid.turn,
        active.snapshot,
    );
    assert!(matches!(
        valid
            .storage
            .clone()
            .classify_delivery_recovery(
                &valid.store,
                &startup_source(&valid.store, valid.storage.clone()),
                point_limit(),
            )
            .unwrap(),
        DeliveryRecoveryCase::Pending { .. }
    ));
    assert!(matches!(
        valid
            .storage
            .clone()
            .current_binding(&valid.store, valid.thread, point_limit())
            .unwrap()
            .unwrap()
            .binding()
            .state(),
        BindingState::Valid(_)
    ));

    publish_stale_valid(&valid.store, valid.storage.clone(), valid.thread);
    assert!(matches!(
        valid
            .storage
            .clone()
            .classify_delivery_recovery(
                &valid.store,
                &startup_source(&valid.store, valid.storage.clone()),
                point_limit(),
            )
            .unwrap(),
        DeliveryRecoveryCase::Pending { .. }
    ));
    assert!(matches!(
        valid
            .storage
            .clone()
            .current_binding(&valid.store, valid.thread, point_limit())
            .unwrap()
            .unwrap()
            .binding()
            .state(),
        BindingState::Stale(_)
    ));
}

#[test]
fn active_classification_covers_pre_and_post_cas_turn_publication() {
    for (value, publish_cas_turn) in [(410, false), (411, true)] {
        let recovery = pending_home("phase63-active-classification", value);
        let expected = activate(
            &recovery.store,
            recovery.storage.clone(),
            recovery.thread,
            recovery.turn,
            publish_cas_turn,
        );
        let source = startup_source(&recovery.store, recovery.storage.clone());
        let DeliveryRecoveryCase::Active(active) = recovery
            .storage
            .clone()
            .classify_delivery_recovery(&recovery.store, &source, point_limit())
            .unwrap()
        else {
            panic!("activated fixture did not classify as active");
        };
        assert_eq!(active.thread_id(), recovery.thread);
        assert_eq!(active.turn_id(), recovery.turn);
        assert_eq!(active.snapshot_id(), expected.snapshot);
        assert_eq!(active.cas_thread_id(), &expected.cas_thread);
        assert_eq!(
            active.loaded_generation(),
            crate::recovery_support::loaded_generation()
        );
        assert_eq!(active.observed_at(), timestamp(5));
        assert_eq!(
            active.state_revision(),
            syndic_storage::TurnStateRevision::FIRST
        );
        assert_eq!(
            expected.cas_turn.is_some(),
            matches!(source.gate_state(), InputGateState::Steerable(_))
        );
        let request = active
            .generic_abandonment("phase63 startup authority lost", active.minimum_timestamp())
            .unwrap();
        assert_eq!(request.thread_id(), recovery.thread);
        assert_eq!(
            request.expected_binding_revision(),
            active.binding_revision()
        );
        assert_eq!(
            request.stale().loaded_generation(),
            Some(active.loaded_generation())
        );
    }
}

#[test]
fn active_abandonment_authenticates_same_turn_post_abandonment_recovery() {
    let recovery = pending_home("phase63-post-abandonment", 420);
    activate(
        &recovery.store,
        recovery.storage.clone(),
        recovery.thread,
        recovery.turn,
        true,
    );
    let active_source = startup_source(&recovery.store, recovery.storage.clone());
    let DeliveryRecoveryCase::Active(active) = recovery
        .storage
        .clone()
        .classify_delivery_recovery(&recovery.store, &active_source, point_limit())
        .unwrap()
    else {
        panic!("activated fixture did not classify as active");
    };
    let request = active
        .generic_abandonment("phase63 recovered authority was lost", timestamp(7))
        .unwrap();
    execute(
        &recovery.store,
        recovery.storage.clone().abandon_active_binding(
            recovery.storage.clone().revision(&recovery.store).unwrap(),
            request,
        ),
    );

    assert!(matches!(
        recovery.storage.clone().classify_delivery_recovery(
            &recovery.store,
            &active_source,
            point_limit(),
        ),
        Err(DeliveryRecoveryClassificationError::SourceDrift)
    ));
    let recovered_source = startup_source(&recovery.store, recovery.storage.clone());
    assert!(matches!(
        recovery.storage.clone().classify_delivery_recovery(
            &recovery.store,
            &recovered_source,
            point_limit(),
        ),
        Ok(DeliveryRecoveryCase::PostAbandonment {
            thread_id,
            turn_id,
            minimum_timestamp,
        }) if thread_id == recovery.thread
            && turn_id == recovery.turn
            && minimum_timestamp == timestamp(7)
    ));
    assert!(matches!(
        recovery
            .storage
            .clone()
            .current_binding(&recovery.store, recovery.thread, point_limit())
            .unwrap()
            .unwrap()
            .binding()
            .state(),
        BindingState::Stale(_)
    ));
}

#[test]
fn compaction_defers_work_and_idle_successor_settles_old_source() {
    let compacting = pending_home("phase63-deferred-compaction", 430);
    replace_gate_state(
        &compacting.store,
        compacting.storage.clone(),
        compacting.thread,
        InputGateState::Compacting {
            turn_id: compacting.turn,
            operation_nonce: CompactionOperationNonce::from_bytes([63; 16]),
        },
    );
    assert!(matches!(
        compacting.storage.clone().classify_delivery_recovery(
            &compacting.store,
            &startup_source(&compacting.store, compacting.storage.clone()),
            point_limit(),
        ),
        Ok(DeliveryRecoveryCase::DeferredCompaction {
            thread_id,
            turn_id,
        }) if thread_id == compacting.thread && turn_id == compacting.turn
    ));

    let settled = pending_home("phase63-settled-successor", 431);
    let source = startup_source(&settled.store, settled.storage.clone());
    replace_gate_state(
        &settled.store,
        settled.storage.clone(),
        settled.thread,
        InputGateState::Idle,
    );
    assert!(matches!(
        settled
            .storage.clone()
            .classify_delivery_recovery(&settled.store, &source, point_limit()),
        Ok(DeliveryRecoveryCase::Settled { thread_id }) if thread_id == settled.thread
    ));
}

#[test]
fn non_idle_gate_successor_is_reported_as_source_drift() {
    let recovery = pending_home("phase63-source-drift", 440);
    let source = startup_source(&recovery.store, recovery.storage.clone());
    replace_gate_state(
        &recovery.store,
        recovery.storage.clone(),
        recovery.thread,
        InputGateState::PendingTurn(recovery.turn),
    );
    assert!(matches!(
        recovery.storage.clone().classify_delivery_recovery(
            &recovery.store,
            &source,
            point_limit()
        ),
        Err(DeliveryRecoveryClassificationError::SourceDrift)
    ));
}

#[test]
fn stable_unsupported_turn_state_is_reported_as_corruption() {
    let recovery = pending_home("phase63-corrupt-turn-state", 450);
    let source = startup_source(&recovery.store, recovery.storage.clone());
    let state = recovery
        .storage
        .clone()
        .turn_state(&recovery.store, recovery.turn, point_limit())
        .unwrap()
        .unwrap();
    commit(
        &recovery.store,
        recovery.storage.clone(),
        batch([FixtureRecord::TurnState(fixture_turn_state(
            recovery.turn,
            state.revision().checked_next().unwrap(),
            TurnLifecycle::Complete,
            0,
            0,
            timestamp(8),
        ))]),
    );
    assert!(matches!(
        recovery.storage.clone().classify_delivery_recovery(
            &recovery.store,
            &source,
            point_limit()
        ),
        Err(DeliveryRecoveryClassificationError::Corruption(_))
    ));
}

#[test]
fn pending_gate_for_a_noncurrent_tail_is_never_recoverable() {
    let recovery = pending_home("phase63-corrupt-pending-tail", 460);
    let summary = recovery
        .storage
        .clone()
        .history_summary(&recovery.store, recovery.thread, point_limit())
        .unwrap()
        .unwrap();
    let wrong_tail =
        beryl_model::SyndicTurnId::from_bytes(*crate::recovery_support::ordered_id(461).as_bytes());
    commit(
        &recovery.store,
        recovery.storage.clone(),
        batch([FixtureRecord::HistorySummary(HistorySummaryRecord::new(
            recovery.thread,
            summary.revision().checked_next().unwrap(),
            summary.thread_revision(),
            Some(wrong_tail),
            summary.selected_path_digest(),
            false,
            summary.last_activity_at(),
        ))]),
    );

    let source = startup_source(&recovery.store, recovery.storage.clone());
    assert!(matches!(
        recovery.storage.clone().classify_delivery_recovery(
            &recovery.store,
            &source,
            point_limit()
        ),
        Err(DeliveryRecoveryClassificationError::Corruption(_))
    ));

    let revision = recovery.storage.clone().revision(&recovery.store).unwrap();
    assert!(matches!(
        recovery.storage.clone().recovered_pending_page(
            &recovery.store,
            revision,
            None,
            beryl_home_store::CursorReadLimits::new(1, DELIVERY_RECOVERY_GATE_PAGE_MAX_BYTES)
                .unwrap(),
            point_limit(),
        ),
        Err(SyndicReadError::Invariant(_))
    ));
}

#[test]
fn active_snapshot_must_match_the_current_history_path() {
    let recovery = pending_home("phase63-corrupt-active-path", 470);
    activate(
        &recovery.store,
        recovery.storage.clone(),
        recovery.thread,
        recovery.turn,
        false,
    );
    let summary = recovery
        .storage
        .clone()
        .history_summary(&recovery.store, recovery.thread, point_limit())
        .unwrap()
        .unwrap();
    commit(
        &recovery.store,
        recovery.storage.clone(),
        batch([FixtureRecord::HistorySummary(HistorySummaryRecord::new(
            recovery.thread,
            summary.revision().checked_next().unwrap(),
            summary.thread_revision(),
            summary.committed_tail(),
            beryl_model::SyndicPathDigest::from_bytes([0xA5; 32]),
            summary.complete(),
            summary.last_activity_at(),
        ))]),
    );

    let source = startup_source(&recovery.store, recovery.storage.clone());
    assert!(matches!(
        recovery.storage.clone().classify_delivery_recovery(
            &recovery.store,
            &source,
            point_limit()
        ),
        Err(DeliveryRecoveryClassificationError::Corruption(_))
    ));
}
