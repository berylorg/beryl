#![cfg(feature = "test-faults")]

mod support;

use beryl_home_store::{
    CommandError, CursorReadLimits, HomeCommand, HomeHealthState, HomeOpenOptions,
    HomeSchemaVersion, HomeStore,
    test_faults::{FaultController, FaultPoint},
};
use beryl_model::SyndicItemId;
use syndic_storage::{
    AdvanceItemProjectionBuild, CreateThread, DraftEditHistoryPolicyV1, ItemProjectionBuildPhase,
    ItemProjectionBuildRecord, ItemProjectionGeneration, ProjectionLifecycle,
    StartItemProjectionBuild, SyndicPointReadLimit, SyndicStorage,
};

use support::{TestHome, draft_id, exact_cas::submit_current_draft, id, open, timestamp};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RecoveredState {
    Old,
    New,
}

struct PendingPublication {
    item: SyndicItemId,
    generation: ItemProjectionGeneration,
    build: ItemProjectionBuildRecord,
}

fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(1_000_000).unwrap()
}

fn open_with_faults(path: &std::path::Path, faults: FaultController) -> HomeStore {
    HomeStore::open_with_faults(
        HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT),
        faults,
    )
    .unwrap()
}

fn execute(store: &HomeStore, contribution: beryl_home_store::MutationContribution) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    match store.execute(command) {
        beryl_home_store::CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected clean item-projection fixture command, got {outcome:?}"),
    }
}

fn prepare_final_publication(store: &HomeStore, storage: SyndicStorage) -> PendingPublication {
    let thread = id(1);
    let draft = draft_id(2);
    execute(
        store,
        storage.create_thread(
            storage.revision(store).unwrap(),
            CreateThread::ordinary(
                thread,
                draft,
                support::exact_cas::execution_binding(),
                timestamp(1),
                DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
            ),
        ),
    );

    let item = SyndicItemId::from_bytes([4; 16]);
    submit_current_draft(
        store,
        storage,
        thread,
        draft_id(3),
        item,
        "atomic projection",
        timestamp(3),
    );

    let canonical = storage
        .canonical_item(store, item, point_limit())
        .unwrap()
        .unwrap();
    let generation = ItemProjectionGeneration::FIRST;
    execute(
        store,
        storage.start_item_projection_build(
            storage.revision(store).unwrap(),
            StartItemProjectionBuild::new(item, canonical.revision(), generation),
        ),
    );
    let initial_build = storage
        .item_projection_build(store, item, generation, point_limit())
        .unwrap()
        .unwrap();
    execute(
        store,
        storage.advance_item_projection_build(
            storage.revision(store).unwrap(),
            AdvanceItemProjectionBuild::new(item, generation, initial_build.revision()),
        ),
    );

    let build = storage
        .item_projection_build(store, item, generation, point_limit())
        .unwrap()
        .unwrap()
        .clone();
    assert!(matches!(
        build.phase(),
        ItemProjectionBuildPhase::Parsing(_)
    ));
    assert_eq!(build.projection_count(), 0);
    assert!(
        storage
            .item_projection_set(store, item, generation, point_limit())
            .unwrap()
            .is_none()
    );
    assert!(
        storage
            .item_projection_head(store, item, point_limit())
            .unwrap()
            .is_none()
    );
    PendingPublication {
        item,
        generation,
        build,
    }
}

fn assert_recovered_state(
    store: &HomeStore,
    storage: SyndicStorage,
    pending: &PendingPublication,
    expected: RecoveredState,
) {
    let build = storage
        .item_projection_build(store, pending.item, pending.generation, point_limit())
        .unwrap();
    let set = storage
        .item_projection_set(store, pending.item, pending.generation, point_limit())
        .unwrap();
    let head = storage
        .item_projection_head(store, pending.item, point_limit())
        .unwrap();

    match (expected, build, set, head) {
        (RecoveredState::Old, Some(build), None, None) => {
            assert_eq!(build, pending.build);
        }
        (RecoveredState::New, None, Some(set), Some(head)) => {
            assert_eq!(set.item_id(), pending.item);
            assert_eq!(set.generation(), pending.generation);
            assert_eq!(set.projection_count(), 1);
            assert_eq!(set.resource_count(), 0);
            assert!(set.stable_eof_resolved());
            assert_eq!(head.generation(), pending.generation);
            assert_eq!(head.lifecycle(), ProjectionLifecycle::Current);

            let page = storage
                .item_projections(
                    store,
                    pending.item,
                    pending.generation,
                    None,
                    CursorReadLimits::new(2, 1_000_000).unwrap(),
                )
                .unwrap();
            assert_eq!(page.records().len(), 1);
            assert!(!page.has_more());
            let projection = storage
                .projection(store, page.records()[0].projection_id(), point_limit())
                .unwrap()
                .unwrap();
            assert_eq!(projection.item_id(), pending.item);
        }
        (expected, build, set, head) => panic!(
            "mixed projection-publication state after {expected:?}: build={}, set={}, head={}",
            build.is_some(),
            set.is_some(),
            head.is_some()
        ),
    }
}

#[test]
fn final_item_projection_publication_reconciles_to_wholly_old_or_wholly_new() {
    for (name, point, expected) in [
        (
            "phase7-projection-publication-before-commit",
            FaultPoint::BeforeCommit,
            RecoveredState::Old,
        ),
        (
            "phase7-projection-publication-after-commit-before-persist",
            FaultPoint::AfterCommitBeforePersist,
            RecoveredState::New,
        ),
        (
            "phase7-projection-publication-after-persist",
            FaultPoint::AfterPersist,
            RecoveredState::New,
        ),
    ] {
        let home = TestHome::new(name);
        let faults = FaultController::new();
        let mut store = open_with_faults(home.path(), faults.clone());
        let storage = SyndicStorage::register(&mut store).unwrap();
        let pending = prepare_final_publication(&store, storage);

        let mut command = HomeCommand::new(store.home_revision().unwrap());
        command
            .add(storage.advance_item_projection_build(
                storage.revision(&store).unwrap(),
                AdvanceItemProjectionBuild::new(
                    pending.item,
                    pending.generation,
                    pending.build.revision(),
                ),
            ))
            .unwrap();
        faults.fail_next(point);
        let outcome = store.execute(command);
        let retained_custody = matches!(
            &outcome,
            beryl_home_store::CommandOutcome::Indeterminate { .. }
        );
        let expected_health = match (point, outcome) {
            (
                FaultPoint::BeforeCommit,
                beryl_home_store::CommandOutcome::NotCommitted {
                    evidence: CommandError::Commit { .. },
                },
            ) => HomeHealthState::Failed,
            (
                FaultPoint::AfterPersist,
                beryl_home_store::CommandOutcome::Committed {
                    later_failure: Some(CommandError::Persistence { .. }),
                    ..
                },
            ) => HomeHealthState::Failed,
            (
                FaultPoint::AfterCommitBeforePersist,
                beryl_home_store::CommandOutcome::Indeterminate {
                    failure: CommandError::Persistence { .. },
                    reconciliation,
                },
            ) => {
                reconciliation.install();
                HomeHealthState::Healthy
            }
            (_, outcome) => panic!("unexpected projection fault outcome: {outcome:?}"),
        };
        assert_eq!(store.health().state(), expected_health);
        if expected_health == HomeHealthState::Failed {
            assert!(!retained_custody);
            store.close().unwrap();
            let mut reopened = open(home.path());
            let reopened_storage = SyndicStorage::register(&mut reopened).unwrap();
            reopened
                .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
                .unwrap();
            assert_recovered_state(&reopened, reopened_storage, &pending, expected);
            reopened.close().unwrap();
            continue;
        }
        assert_recovered_state(&store, storage, &pending, expected);
        store
            .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
            .unwrap();
        if retained_custody {
            let close_error = store.close().unwrap_err();
            assert_eq!(close_error.pending_reconciliation_scopes(), Some(1));
            drop(close_error);
            assert!(
                HomeStore::open(HomeOpenOptions::new(
                    home.path(),
                    HomeSchemaVersion::CURRENT
                ))
                .is_err()
            );
            continue;
        }
        store.close().unwrap();

        let mut reopened = open(home.path());
        let storage = SyndicStorage::register(&mut reopened).unwrap();
        reopened
            .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
            .unwrap();
        assert_recovered_state(&reopened, storage, &pending, expected);
        reopened.close().unwrap();
    }
}
