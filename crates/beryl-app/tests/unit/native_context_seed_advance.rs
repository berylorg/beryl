use super::*;
use crate::support;
use beryl_home_store::{
    HomeOpenCandidate, HomeOpenOptions, HomeSchemaVersion,
    test_faults::{FaultController, FaultPoint},
};
use beryl_model::{ProjectionRevision, SyndicDraftId, SyndicItemId};
use syndic_storage::{
    CanonicalItemRecord, ComposerAtom, ComposerPayload, CreateThread, DraftEditHistoryPolicyV1,
    RecoveryProjectionError, SyndicPointReadLimit, TurnItemIndexRecord, TurnItemOrdinal,
    TurnLifecycle, TurnStateRevision, test_faults::FixtureRecord,
};

fn limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(400_000).unwrap()
}

struct Fixture {
    store: HomeStore,
    storage: SyndicStorage,
    faults: FaultController,
    _directory: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let faults = FaultController::new();
        let mut candidate = HomeOpenCandidate::open_with_faults(
            HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
            faults.clone(),
        )
        .unwrap();
        let storage = SyndicStorage::register(&mut candidate).unwrap();
        let store = candidate
            .prepare_publication(SyndicStorage::required_domains().unwrap())
            .unwrap()
            .publish()
            .unwrap();
        support::seed_populated(&store, storage.clone());
        let root = beryl_model::SyndicTurnId::from_bytes([29; 16]);
        let item = SyndicItemId::from_bytes([209; 16]);
        let payload =
            ComposerPayload::new(vec![ComposerAtom::text("source question").unwrap()]).unwrap();
        let (content, mut records) = support::composer_content_records(&payload);
        let revision = ProjectionRevision::new(1).unwrap();
        records.extend([
            FixtureRecord::TurnState(support::fixture_turn_state(
                root,
                TurnStateRevision::FIRST,
                TurnLifecycle::Interrupted,
                1,
                1,
                support::timestamp(2),
            )),
            FixtureRecord::CanonicalItem(CanonicalItemRecord::local_user_input(
                item,
                root,
                TurnItemOrdinal::FIRST,
                revision,
                content,
                None,
            )),
            FixtureRecord::TurnItem(TurnItemIndexRecord::new(
                root,
                TurnItemOrdinal::FIRST,
                item,
                revision,
            )),
        ]);
        support::commit(&store, storage.clone(), support::batch(records));
        support::exact_cas::project_item_if_needed(&store, storage.clone(), item);
        support::discussion_creation::create_child(&store, &storage, support::id(30), 210, 211);
        support::exact_cas::submit_current_draft(
            &store,
            storage.clone(),
            support::id(210),
            support::draft_id(212),
            SyndicItemId::from_bytes([213; 16]),
            "discuss exactly",
            support::timestamp(10),
        );
        Self {
            store,
            storage,
            faults,
            _directory: directory,
        }
    }

    fn basis(&self) -> NativeProjectionBasis {
        let thread = self
            .storage
            .thread(&self.store, support::id(210), limit())
            .unwrap()
            .unwrap();
        let execution = self
            .storage
            .thread_execution(&self.store, thread.id(), limit())
            .unwrap()
            .unwrap();
        self.storage
            .prepare_native_projection(
                &self.store,
                &NativeProjectionRequest::new(
                    thread.id(),
                    thread.selected_path(),
                    execution.execution().clone(),
                    support::test_tool_profile(),
                ),
                limit(),
            )
            .unwrap()
            .basis()
    }

    fn mutate(&self) {
        support::discussion_input::committed(
            &self.store,
            self.storage.create_thread(
                self.storage.revision(&self.store).unwrap(),
                CreateThread::ordinary(
                    support::id(240),
                    SyndicDraftId::from_bytes([241; 16]),
                    support::exact_cas::execution_binding(),
                    support::timestamp(20),
                    DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
                ),
            ),
        );
    }

    fn pending(&self) -> syndic_storage::PendingDispatchEvidence {
        self.storage
            .pending_dispatch_evidence(&self.store, support::id(210), limit())
            .unwrap()
            .unwrap()
    }
}

#[test]
fn first_context_seed_retains_pending_input_on_witnessed_advance_and_accepts_a_fresh_basis() {
    let fixture = Fixture::new();
    let coordinator = CasProjectionCoordinator::for_healthy_home(&fixture.store).unwrap();
    let basis = fixture.basis();
    assert!(basis.context().unwrap().needs_seed());
    let before = fixture.pending();
    let original_gate = fixture
        .storage
        .discussion_handoff_gate(&fixture.store, before.thread_id(), limit())
        .unwrap();
    let cancellation = ProjectionCancellationToken::new();
    let error = coordinator
        .prepare_native_context_seed_after_basis(
            &fixture.store,
            &fixture.storage,
            basis,
            Some(1_000_000),
            &cancellation,
            || fixture.mutate(),
        )
        .unwrap_err();
    let ProjectionExecutionError::NativeContextSeedSourceAdvanced {
        thread_id,
        expected,
        actual,
    } = error
    else {
        panic!("seed drift did not retain its exact pre-effect witness: {error:?}");
    };
    assert_eq!(thread_id, support::id(210));
    assert_eq!(expected, basis.context().unwrap().source_revision());
    assert_eq!(actual, fixture.storage.revision(&fixture.store).unwrap());
    assert_ne!(expected, actual);
    let after = fixture.pending();
    assert_eq!(before.turn_id(), after.turn_id());
    assert_eq!(before.item_id(), after.item_id());
    assert_eq!(before.input(), after.input());
    assert_eq!(before.gate_revision(), after.gate_revision());
    assert_eq!(before.state_revision(), after.state_revision());
    assert_eq!(
        fixture
            .storage
            .discussion_handoff_gate(&fixture.store, thread_id, limit())
            .unwrap(),
        original_gate,
    );
    let fresh = fixture.basis();
    assert_ne!(fresh.context().unwrap().source_revision(), expected);
    let projection = coordinator
        .prepare_native_context_seed_after_basis(
            &fixture.store,
            &fixture.storage,
            fresh,
            Some(1_000_000),
            &cancellation,
            || {},
        )
        .unwrap();
    assert_eq!(projection.source_revision(), actual);
    assert_eq!(projection.selected_path(), after.selected_path());
    fixture.store.close().unwrap();
}

#[test]
fn cancellation_and_failed_home_cannot_become_seed_progress() {
    for failed_home in [false, true] {
        let fixture = Fixture::new();
        let coordinator = CasProjectionCoordinator::for_healthy_home(&fixture.store).unwrap();
        let basis = fixture.basis();
        let cancellation = ProjectionCancellationToken::new();
        let error = coordinator
            .prepare_native_context_seed_after_basis(
                &fixture.store,
                &fixture.storage,
                basis,
                Some(1_000_000),
                &cancellation,
                || {
                    fixture.mutate();
                    if failed_home {
                        fixture.faults.fail_next(FaultPoint::BeforeReadConfirmation);
                        assert!(
                            fixture
                                .storage
                                .thread(&fixture.store, support::id(210), limit())
                                .is_err()
                        );
                    } else {
                        cancellation.cancel();
                    }
                },
            )
            .unwrap_err();
        assert!(!matches!(
            error,
            ProjectionExecutionError::NativeContextSeedSourceAdvanced { .. }
        ));
        if !failed_home {
            assert!(matches!(error, ProjectionExecutionError::Cancelled));
        }
        fixture.store.close().unwrap();
    }
}

#[test]
fn a_stable_nonseeding_basis_remains_a_plain_seed_refusal_after_unrelated_mutation() {
    let fixture = Fixture::new();
    let pending = fixture.pending();
    support::exact_cas::establish_turn_with_profile(
        &fixture.store,
        fixture.storage.clone(),
        pending.thread_id(),
        pending.turn_id(),
        support::timestamp(21),
        support::test_tool_profile(),
    );
    support::discussion_handoff::complete_resolving_turn_for(
        &fixture.store,
        &fixture.storage,
        pending.thread_id(),
        pending.item_id(),
    );
    support::exact_cas::submit_current_draft(
        &fixture.store,
        fixture.storage.clone(),
        pending.thread_id(),
        support::draft_id(214),
        SyndicItemId::from_bytes([215; 16]),
        "second child turn",
        support::timestamp(50),
    );
    let basis = fixture.basis();
    assert!(!basis.context().unwrap().needs_seed());
    let coordinator = CasProjectionCoordinator::for_healthy_home(&fixture.store).unwrap();
    let error = coordinator
        .prepare_native_context_seed_after_basis(
            &fixture.store,
            &fixture.storage,
            basis,
            Some(1_000_000),
            &ProjectionCancellationToken::new(),
            || fixture.mutate(),
        )
        .unwrap_err();
    assert!(matches!(
        error,
        ProjectionExecutionError::RecoveryProjection(RecoveryProjectionError::ConcurrentChange)
    ));
    fixture.store.close().unwrap();
}
