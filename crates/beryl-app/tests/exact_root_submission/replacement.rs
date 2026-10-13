use super::base as host_support;
use super::*;
#[path = "../historical_replacement_host/support.rs"]
mod history;
use beryl_app::composer_host::{
    ComposerHostError, ComposerHostSubmissionError, ComposerHostSubmissionFaultPoint,
};
use beryl_home_store::test_faults::{FaultPoint, FreeSpaceTestObservation};
use syndic_storage::{
    ComposerAtom, ComposerPayload, DraftSubmissionIntent, InputGateRecord, InputGateState,
};

fn request(
    execution: beryl_app::cas_projection::SubmissionExecutionWake,
) -> ComposerHostSubmissionRequest {
    ComposerHostSubmissionRequest::new(
        execution,
        SyndicDraftId::from_bytes([231; 16]),
        SyndicItemId::from_bytes([232; 16]),
        DraftComposerMaterializationOperationIdV1::from_bytes([233; 16]),
        DraftPieceOperationIdV1::from_bytes([234; 16]),
        SyndicTimestamp::from_unix_millis(1000),
        admission_requirement(),
    )
}

fn import(
    store: &beryl_home_store::HomeStore,
    storage: &syndic_storage::SyndicStorage,
    assets: &AssetState,
    thread: beryl_model::SyndicThreadId,
) -> (
    beryl_app::composer_host::SyndicComposerHost,
    beryl_app::composer_host::ComposerHostHistoricalTarget,
) {
    let target = history::seed_source(
        store,
        storage,
        thread,
        &ComposerPayload::new(vec![ComposerAtom::text("replacement exact input").unwrap()])
            .unwrap(),
    );
    let (mut host, _) = activated(storage.clone(), store, thread, 211, 212);
    let ticket = host
        .begin_historical_replacement(
            store,
            host.binding().unwrap(),
            target,
            history::operation(221),
        )
        .unwrap();
    let completion = history::finish(&mut host, store, assets, ticket);
    host.acknowledge_historical_replacement(ticket, completion.binding())
        .unwrap();
    (host, target)
}

#[test]
fn replacement_submission_refuses_steering_pending_turn_and_compaction_routes() {
    for state in [
        InputGateState::Steerable(beryl_model::SyndicTurnId::from_bytes([203; 16])),
        InputGateState::PendingTurn(beryl_model::SyndicTurnId::from_bytes([203; 16])),
        InputGateState::compacting(
            beryl_model::SyndicTurnId::from_bytes([203; 16]),
            syndic_storage::CompactionOperationNonce::from_bytes([229; 16]),
        ),
    ] {
        let (_home, store, state_owner, storage, thread, faults) =
            base::fault_fixture_with_state("replacement-idle-only", 201);
        let assets = state_owner.assets();
        let seals = service(&store, storage.clone(), assets.clone(), 1, 1);
        let (mut host, target) = import(&store, &storage, &assets, thread);
        let prior = host.binding().unwrap();
        let intent = host.submission_intent();
        let gate = storage
            .input_gate(&store, thread, point_limit())
            .unwrap()
            .unwrap();
        let changed = InputGateRecord::new(
            thread,
            gate.revision().checked_next().unwrap(),
            state,
            gate.accepted_high_water(),
            gate.route_generation_high_water(),
            gate.selected_route(),
            gate.live_steering_count(),
            gate.live_next_turn_count(),
            gate.live_logical_utf8_bytes(),
        )
        .unwrap();
        let mut batch = syndic_storage::test_faults::FixtureBatch::new();
        batch
            .put(syndic_storage::test_faults::FixtureRecord::InputGate(
                changed,
            ))
            .unwrap();
        base::committed(base::execute(
            &store,
            storage.fixture_contribution(storage.revision(&store).unwrap(), batch),
        ));
        let ticket = host
            .begin_submission(request(
                beryl_app::cas_projection::SubmissionExecutionWake::storage_only_for_test(),
            ))
            .unwrap();
        let mut rejected = false;
        for _ in 0..128 {
            match host.advance_submission(
                &store,
                ticket,
                assets.clone(),
                &seals,
                history::operation(222),
                None,
                history::timestamp(),
                &CommandCancellation::new(),
            ) {
                Err(ComposerHostSubmissionError::Host(ComposerHostError::LifecycleBlocked)) => {
                    rejected = true;
                    break;
                }
                Ok(ComposerHostSubmissionAdvance::Progress(_)) => {}
                other => panic!("replacement used a non-idle route: {other:?}"),
            }
        }
        assert!(rejected);
        assert_eq!(host.binding(), Some(prior));
        assert_eq!(host.submission_intent(), intent);
        assert_eq!(faults.free_space_observation_count(), 0);
        assert_eq!(
            storage
                .thread(&store, thread, point_limit())
                .unwrap()
                .unwrap()
                .committed_tail(),
            Some(target.turn_id)
        );
        assert!(
            storage
                .accepted_input(
                    &store,
                    prior.candidate().draft_id().accepted_input_id(),
                    point_limit()
                )
                .unwrap()
                .is_none()
        );
    }
}

#[test]
fn replacement_indeterminate_acceptance_retains_intent_and_blocks_repeat_until_exact_reconcile() {
    let (_home, store, state, storage, thread, faults) =
        base::fault_fixture_with_state("replacement-original-outcome", 201);
    let assets = state.assets();
    let seals = service(&store, storage.clone(), assets.clone(), 1, 1);
    let (mut host, target) = import(&store, &storage, &assets, thread);
    let prior = host.binding().unwrap();
    let intent = host.submission_intent();
    assert!(matches!(
        intent,
        Some(DraftSubmissionIntent::Replacement(_))
    ));
    let (execution, wakes) = beryl_app::cas_projection::SubmissionExecutionWake::test_for_home(
        &store,
        Default::default(),
    );
    let ticket = host.begin_submission(request(execution)).unwrap();
    advance_to_accepting(
        &mut host,
        &store,
        assets.clone(),
        &seals,
        ticket,
        history::operation(222),
        history::timestamp(),
    );
    assert_eq!(
        storage
            .thread(&store, thread, point_limit())
            .unwrap()
            .unwrap()
            .committed_tail(),
        Some(target.turn_id)
    );
    faults.push_free_space_observation(FreeSpaceTestObservation::Observed {
        available_bytes: u64::MAX,
        total_free_bytes: u64::MAX,
        total_bytes: u64::MAX,
    });
    let injected = faults.clone();
    host.test_arm_submission_before_execute_fault(move |_, _| {
        injected.fail_next(FaultPoint::AfterCommitBeforePersist)
    });
    host.test_arm_submission_transition_fault(
        ComposerHostSubmissionFaultPoint::AcceptanceAfterAttempt,
    );
    assert!(matches!(
        host.advance_submission(
            &store,
            ticket,
            assets.clone(),
            &seals,
            history::operation(222),
            None,
            history::timestamp(),
            &CommandCancellation::new()
        ),
        Err(ComposerHostSubmissionError::InjectedFault(
            ComposerHostSubmissionFaultPoint::AcceptanceAfterAttempt
        ))
    ));
    assert_eq!(host.binding(), Some(prior));
    assert_eq!(host.submission_intent(), intent);
    assert!(host.submission_diagnostics().command_attempted());
    assert!(
        host.begin_submission(request(
            beryl_app::cas_projection::SubmissionExecutionWake::storage_only_for_test()
        ))
        .is_err()
    );
    assert_eq!(faults.free_space_observation_count(), 1);
    assert_eq!(wakes.wake_count(), 0);
    assert_eq!(
        drive_submission(
            &mut host,
            &store,
            assets,
            &seals,
            ticket,
            history::operation(222)
        ),
        ComposerHostSubmissionAdvance::ExactSuccess(FirstAcceptanceKind::Idle {
            user_item_id: SyndicItemId::from_bytes([232; 16])
        })
    );
    assert_eq!(faults.free_space_observation_count(), 1);
    assert_eq!(wakes.wake_count(), 1);
    let current = storage
        .current_draft(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        current.thread().committed_tail(),
        Some(prior.candidate().draft_id().submitted_turn_id())
    );
    assert_ne!(current.thread().committed_tail(), Some(target.turn_id));
    assert_eq!(
        current.draft().submission_intent(),
        DraftSubmissionIntent::Ordinary
    );
    assert!(
        storage
            .canonical_item(&store, target.item_id, point_limit())
            .unwrap()
            .is_some()
    );
    assert!(
        storage
            .accepted_input(
                &store,
                prior.candidate().draft_id().accepted_input_id(),
                point_limit()
            )
            .unwrap()
            .is_none()
    );
    assert!(!host.submission_diagnostics().pending());
}
