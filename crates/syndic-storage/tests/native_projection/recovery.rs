use beryl_home_store::{CommandOutcome, HomeStore};
use beryl_model::{CasThreadId, SyndicDraftId, SyndicThreadId};
use syndic_storage::test_faults::{FixtureBatch, FixtureRecord};
use syndic_storage::*;

use super::{fixtures, point_limit, selected_path, support};

fn seed_idle_populated(store: &HomeStore, storage: &SyndicStorage) {
    support::seed_populated(store, storage.clone());
    support::converge_and_release_terminal_history(
        store,
        storage.clone(),
        support::id(30),
        support::populated::source_turn(),
    );
}

fn request(
    store: &HomeStore,
    storage: &SyndicStorage,
    thread: SyndicThreadId,
) -> NativeProjectionRequest {
    NativeProjectionRequest::new(
        thread,
        selected_path(store, storage, thread),
        storage
            .thread_execution(store, thread, point_limit())
            .unwrap()
            .unwrap()
            .execution()
            .clone(),
        support::test_tool_profile(),
    )
}

fn ready(
    store: &HomeStore,
    storage: &SyndicStorage,
    thread: SyndicThreadId,
) -> (NativeProjectionPlan, NativeProjectionRecoveryBasis) {
    match storage
        .prepare_native_projection_recovery(store, &request(store, storage, thread), point_limit())
        .unwrap()
    {
        NativeProjectionRecoveryPlan::Ready { plan, basis } => (plan, basis),
        other => panic!("expected exact recovery plan, got {other:?}"),
    }
}

fn fresh_publication(basis: NativeProjectionRecoveryBasis, cas_id: &str) -> PublishValidBinding {
    let prefix = basis.native_basis().represented_prefix();
    PublishValidBinding::from_native_recovery(
        basis,
        CasThreadId::new(cas_id).unwrap(),
        beryl_model::CasNativeTurnCount::ZERO,
        CasLineageProof::native(NativeCasLineage::Fresh, prefix).unwrap(),
    )
}

fn replace_gate(
    store: &HomeStore,
    storage: &SyndicStorage,
    thread: SyndicThreadId,
    state: InputGateState,
) {
    let gate = storage
        .input_gate(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let mut batch = FixtureBatch::new();
    batch
        .put(FixtureRecord::InputGate(
            InputGateRecord::new(
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
            .unwrap(),
        ))
        .unwrap();
    support::commit(store, storage.clone(), batch);
}

fn assert_rejected(outcome: CommandOutcome) {
    assert!(
        matches!(outcome, CommandOutcome::NotCommitted { .. }),
        "unexpected outcome: {outcome:?}"
    );
}

#[test]
fn idle_empty_recovery_publishes_only_binding_and_rejects_duplicate_basis() {
    let home = support::TestHome::new("recovery-idle-empty");
    let mut candidate = support::open(home.path());
    let storage = SyndicStorage::register(&mut candidate).unwrap();
    let store = candidate
        .prepare_publication(SyndicStorage::required_domains().unwrap())
        .unwrap()
        .publish()
        .unwrap();
    let thread = support::id(156);
    support::seed_canonical_empty_thread(
        &store,
        storage.clone(),
        thread,
        SyndicDraftId::from_bytes([157; 16]),
    );
    let initial_thread = storage.thread(&store, thread, point_limit()).unwrap();
    let initial_gate = storage.input_gate(&store, thread, point_limit()).unwrap();
    let initial_draft = storage
        .current_draft(&store, thread, point_limit())
        .unwrap();
    let before = storage.revision(&store).unwrap();
    let (plan, basis) = ready(&store, &storage, thread);
    assert!(matches!(plan, NativeProjectionPlan::Fresh { .. }));
    assert_eq!(basis.native_basis().represented_prefix().tail(), None);
    assert_eq!(storage.revision(&store).unwrap(), before);
    assert!(
        storage
            .validate_native_projection_recovery_basis(&store, &basis, point_limit())
            .unwrap()
    );
    let publication = fresh_publication(basis.clone(), "no-input-fresh");
    assert!(matches!(
        store.execute_current(storage.current_publish_valid_binding(publication.clone())),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    assert_eq!(
        storage.thread(&store, thread, point_limit()).unwrap(),
        initial_thread
    );
    assert_eq!(
        storage.input_gate(&store, thread, point_limit()).unwrap(),
        initial_gate
    );
    assert_eq!(
        storage
            .current_draft(&store, thread, point_limit())
            .unwrap(),
        initial_draft
    );
    assert!(
        !storage
            .validate_native_projection_recovery_basis(&store, &basis, point_limit())
            .unwrap()
    );
    let current = storage
        .current_binding(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    let successor = storage
        .native_projection_recovery_successor_basis(
            &store,
            &basis,
            current.binding().revision(),
            point_limit(),
        )
        .unwrap()
        .unwrap();
    assert_eq!(
        successor
            .source()
            .unwrap()
            .binding()
            .cas_thread_id()
            .as_str(),
        "no-input-fresh"
    );
    assert!(
        storage
            .validate_native_projection_recovery_basis(&store, &successor, point_limit())
            .unwrap()
    );
    let committed = storage.revision(&store).unwrap();
    assert_rejected(store.execute_current(storage.current_publish_valid_binding(publication)));
    assert_eq!(storage.revision(&store).unwrap(), committed);
    store.close().unwrap();
}

#[test]
fn idle_committed_recovery_uses_exact_current_prefix_without_pending_input() {
    let home = support::TestHome::new("recovery-idle-committed");
    let mut candidate = support::open(home.path());
    let storage = SyndicStorage::register(&mut candidate).unwrap();
    let store = candidate
        .prepare_publication(SyndicStorage::required_domains().unwrap())
        .unwrap()
        .publish()
        .unwrap();
    seed_idle_populated(&store, &storage);
    let thread = support::id(30);
    let (plan, basis) = ready(&store, &storage, thread);
    assert!(matches!(plan, NativeProjectionPlan::Current { .. }));
    assert_eq!(
        basis.native_basis().represented_prefix().tail(),
        Some(support::populated::source_turn())
    );
    assert!(
        storage
            .native_projection_recovery_successor_basis(
                &store,
                &basis,
                basis.native_basis().expected_binding_revision(),
                point_limit()
            )
            .unwrap()
            .is_some()
    );
    let source = basis.source().unwrap();
    let publication = PublishValidBinding::from_native_recovery(
        basis.clone(),
        source.binding().cas_thread_id().clone(),
        source.binding().native_turn_count(),
        source.binding().lineage(),
    );
    let gate = storage.input_gate(&store, thread, point_limit()).unwrap();
    assert!(matches!(
        store.execute_current(storage.current_publish_valid_binding(publication)),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    assert_eq!(
        storage.input_gate(&store, thread, point_limit()).unwrap(),
        gate
    );
    store.close().unwrap();
}

#[test]
fn admitted_pending_recovery_preserves_input_and_delivery_accounting() {
    let home = support::TestHome::new("recovery-pending-accounting");
    let mut candidate = support::open(home.path());
    let storage = SyndicStorage::register(&mut candidate).unwrap();
    let store = candidate
        .prepare_publication(SyndicStorage::required_domains().unwrap())
        .unwrap()
        .publish()
        .unwrap();
    let fixture = fixtures::seed_root_pending(&store, &storage, 160, false);
    let admission = fixtures::seed_accepted_input_admission_descendant(&store, &storage, &fixture);
    let accepted = storage
        .accepted_input(&store, admission.input, point_limit())
        .unwrap();
    let gate = storage
        .input_gate(&store, fixture.thread, point_limit())
        .unwrap();
    let turn = storage
        .turn(&store, fixture.pending, point_limit())
        .unwrap();
    let state = storage
        .turn_state(&store, fixture.pending, point_limit())
        .unwrap();
    let (plan, basis) = ready(&store, &storage, fixture.thread);
    assert!(matches!(plan, NativeProjectionPlan::Fresh { .. }));
    assert_eq!(basis.native_basis().selected_path(), admission.selected);
    assert_eq!(basis.native_basis().represented_prefix().tail(), None);
    assert!(matches!(
        store.execute_current(
            storage.current_publish_valid_binding(fresh_publication(basis, "no-input-pending"))
        ),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    assert_eq!(
        storage
            .accepted_input(&store, admission.input, point_limit())
            .unwrap(),
        accepted
    );
    assert_eq!(
        storage
            .input_gate(&store, fixture.thread, point_limit())
            .unwrap(),
        gate
    );
    assert_eq!(
        storage
            .turn(&store, fixture.pending, point_limit())
            .unwrap(),
        turn
    );
    assert_eq!(
        storage
            .turn_state(&store, fixture.pending, point_limit())
            .unwrap(),
        state
    );
    store.close().unwrap();
}

#[test]
fn recovery_publication_rejects_changed_gate_even_when_prefix_stays_exact() {
    let home = support::TestHome::new("recovery-stale-gate");
    let mut candidate = support::open(home.path());
    let storage = SyndicStorage::register(&mut candidate).unwrap();
    let store = candidate
        .prepare_publication(SyndicStorage::required_domains().unwrap())
        .unwrap()
        .publish()
        .unwrap();
    let fixture = fixtures::seed_root_pending(&store, &storage, 164, false);
    let (_, basis) = ready(&store, &storage, fixture.thread);
    let publication = fresh_publication(basis.clone(), "stale-gate-target");
    replace_gate(
        &store,
        &storage,
        fixture.thread,
        InputGateState::PendingTurn(fixture.pending),
    );
    assert!(
        !storage
            .validate_native_projection_recovery_basis(&store, &basis, point_limit())
            .unwrap()
    );
    let before = storage.revision(&store).unwrap();
    assert_rejected(store.execute_current(storage.current_publish_valid_binding(publication)));
    assert_eq!(storage.revision(&store).unwrap(), before);
    store.close().unwrap();
}

#[test]
fn recovery_rejects_compatible_older_selection_at_read_and_atomic_publication() {
    let home = support::TestHome::new("recovery-stale-selection");
    let mut candidate = support::open(home.path());
    let storage = SyndicStorage::register(&mut candidate).unwrap();
    let store = candidate
        .prepare_publication(SyndicStorage::required_domains().unwrap())
        .unwrap()
        .publish()
        .unwrap();
    let fixture = fixtures::seed_root_pending(&store, &storage, 168, false);
    let initial_request = request(&store, &storage, fixture.thread);
    let (_, basis) = ready(&store, &storage, fixture.thread);
    let publication = fresh_publication(basis.clone(), "stale-selection-target");
    let admission = fixtures::seed_accepted_input_admission_descendant(&store, &storage, &fixture);
    assert!(
        admission
            .selected
            .is_compatible_descendant_of(initial_request.selected_path())
    );
    assert!(matches!(
        storage.prepare_native_projection_recovery(&store, &initial_request, point_limit()),
        Err(NativeProjectionError::StaleSelectedPath)
    ));
    assert!(
        !storage
            .validate_native_projection_recovery_basis(&store, &basis, point_limit())
            .unwrap()
    );
    assert_rejected(store.execute_current(storage.current_publish_valid_binding(publication)));
    store.close().unwrap();
}

#[test]
fn recovery_current_source_cannot_be_substituted_or_broadened() {
    let home = support::TestHome::new("recovery-source-substitution");
    let mut candidate = support::open(home.path());
    let storage = SyndicStorage::register(&mut candidate).unwrap();
    let store = candidate
        .prepare_publication(SyndicStorage::required_domains().unwrap())
        .unwrap()
        .publish()
        .unwrap();
    seed_idle_populated(&store, &storage);
    let (_, basis) = ready(&store, &storage, support::id(30));
    let source = basis.source().unwrap();
    let substituted = PublishValidBinding::from_native_recovery(
        basis.clone(),
        CasThreadId::new("substitute-source").unwrap(),
        source.binding().native_turn_count(),
        source.binding().lineage(),
    );
    assert_rejected(store.execute_current(storage.current_publish_valid_binding(substituted)));
    let broadened = PublishValidBinding::from_native_recovery(
        basis.clone(),
        source.binding().cas_thread_id().clone(),
        beryl_model::CasNativeTurnCount::new(2),
        source.binding().lineage(),
    );
    assert_rejected(store.execute_current(storage.current_publish_valid_binding(broadened)));
    store.close().unwrap();
}

#[test]
fn recovery_returns_typed_active_and_unknown_terminal_unavailability() {
    for (lifecycle, expected) in [
        (
            TurnLifecycle::Active,
            NativeProjectionRecoveryUnavailable::Active,
        ),
        (
            TurnLifecycle::UnknownTerminal,
            NativeProjectionRecoveryUnavailable::UnknownTerminal,
        ),
    ] {
        let home = support::TestHome::new("recovery-unavailable-lifecycle");
        let mut candidate = support::open(home.path());
        let storage = SyndicStorage::register(&mut candidate).unwrap();
        let store = candidate
            .prepare_publication(SyndicStorage::required_domains().unwrap())
            .unwrap()
            .publish()
            .unwrap();
        let fixture = fixtures::seed_root_pending(&store, &storage, 172, false);
        let mut batch = FixtureBatch::new();
        batch
            .put(FixtureRecord::TurnState(support::fixture_turn_state(
                fixture.pending,
                TurnStateRevision::FIRST,
                lifecycle,
                0,
                0,
                support::timestamp(2),
            )))
            .unwrap();
        support::commit(&store, storage.clone(), batch);
        replace_gate(
            &store,
            &storage,
            fixture.thread,
            InputGateState::AwaitingTerminal(fixture.pending),
        );
        assert_eq!(
            storage
                .prepare_native_projection_recovery(
                    &store,
                    &request(&store, &storage, fixture.thread),
                    point_limit()
                )
                .unwrap(),
            NativeProjectionRecoveryPlan::Unavailable(expected)
        );
        store.close().unwrap();
    }
}

#[test]
fn recovery_point_limit_failure_has_no_durable_effect() {
    let home = support::TestHome::new("recovery-read-capacity");
    let mut candidate = support::open(home.path());
    let storage = SyndicStorage::register(&mut candidate).unwrap();
    let store = candidate
        .prepare_publication(SyndicStorage::required_domains().unwrap())
        .unwrap()
        .publish()
        .unwrap();
    let fixture = fixtures::seed_root_pending(&store, &storage, 176, false);
    let before = storage.revision(&store).unwrap();
    assert!(matches!(
        storage.prepare_native_projection_recovery(
            &store,
            &request(&store, &storage, fixture.thread),
            SyndicPointReadLimit::new(1).unwrap()
        ),
        Err(NativeProjectionError::Read(_))
    ));
    assert_eq!(storage.revision(&store).unwrap(), before);
    store.close().unwrap();
}

#[test]
fn recovery_authenticates_pending_parent_and_idle_fork_source() {
    let home = support::TestHome::new("recovery-parent-and-fork");
    let mut candidate = support::open(home.path());
    let storage = SyndicStorage::register(&mut candidate).unwrap();
    let store = candidate
        .prepare_publication(SyndicStorage::required_domains().unwrap())
        .unwrap()
        .publish()
        .unwrap();
    seed_idle_populated(&store, &storage);
    let child = support::id(180);
    fixtures::seed_child_at_tail(
        &store,
        &storage,
        support::id(30),
        child,
        SyndicDraftId::from_bytes([181; 16]),
    );
    let (plan, basis) = ready(&store, &storage, child);
    let NativeProjectionPlan::Fork {
        native_turn_count, ..
    } = plan
    else {
        panic!("idle inherited prefix requires native fork")
    };
    let prefix = basis.native_basis().represented_prefix();
    assert_eq!(prefix.tail(), Some(support::populated::source_turn()));
    let publication = PublishValidBinding::from_native_recovery(
        basis.clone(),
        CasThreadId::new("idle-native-fork").unwrap(),
        native_turn_count,
        CasLineageProof::native(NativeCasLineage::Fork, prefix).unwrap(),
    );
    let pending = fixtures::append_pending(
        &store,
        &storage,
        support::id(30),
        beryl_model::SyndicTurnId::from_bytes([182; 16]),
        support::populated::source_turn(),
    );
    let (plan, pending_basis) = ready(&store, &storage, pending.thread);
    assert!(matches!(plan, NativeProjectionPlan::Resume { .. }));
    assert_eq!(
        pending_basis.native_basis().represented_prefix().tail(),
        Some(support::populated::source_turn())
    );
    assert!(
        !storage
            .validate_native_projection_recovery_basis(&store, &basis, point_limit())
            .unwrap()
    );
    let before = storage.revision(&store).unwrap();
    assert_rejected(store.execute_current(storage.current_publish_valid_binding(publication)));
    assert_eq!(storage.revision(&store).unwrap(), before);
    store.close().unwrap();
}

#[test]
fn recovery_returns_typed_repair_and_discussion_context_unavailability() {
    let home = support::TestHome::new("recovery-repair-and-context");
    let mut candidate = support::open(home.path());
    let storage = SyndicStorage::register(&mut candidate).unwrap();
    let store = candidate
        .prepare_publication(SyndicStorage::required_domains().unwrap())
        .unwrap()
        .publish()
        .unwrap();
    seed_idle_populated(&store, &storage);
    let before = storage.revision(&store).unwrap();
    assert_eq!(
        storage
            .prepare_native_projection_recovery(
                &store,
                &request(&store, &storage, support::id(36)),
                point_limit()
            )
            .unwrap(),
        NativeProjectionRecoveryPlan::Unavailable(
            NativeProjectionRecoveryUnavailable::UnsupportedContext
        )
    );
    assert_eq!(storage.revision(&store).unwrap(), before);
    let source = support::populated::source_turn();
    let target = RepairRequiredTarget::new(
        source,
        CasTurnSource::new(
            CasThreadId::new("repair-source").unwrap(),
            beryl_model::CasTurnId::new("repair-turn").unwrap(),
        ),
        RepairCaptureGap::new(
            RepairSourceEventWitness::new(
                SourceEventSequence::new(1).unwrap(),
                RepairSourceEventDigest::from_bytes([7; 32]),
            ),
            TurnEndStatus::incomplete(TurnIncompleteReason::StreamLost),
            RepairCaptureGapReason::TerminalCaptureIncomplete,
            None,
        )
        .unwrap(),
        RepairRequestDisposition::Available,
    );
    replace_gate(
        &store,
        &storage,
        support::id(30),
        InputGateState::RepairRequired(target),
    );
    assert_eq!(
        storage
            .prepare_native_projection_recovery(
                &store,
                &request(&store, &storage, support::id(30)),
                point_limit()
            )
            .unwrap(),
        NativeProjectionRecoveryPlan::Unavailable(
            NativeProjectionRecoveryUnavailable::RepairPending
        )
    );
    store.close().unwrap();
}
