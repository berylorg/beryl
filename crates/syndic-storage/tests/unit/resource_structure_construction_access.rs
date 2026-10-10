use super::*;
use crate::{SyndicStorage, indexed_resource as api};

fn state(store: &HomeStore, storage: &SyndicStorage) -> api::BuildState {
    let frozen = store
        .capture_frozen_read(&CommandCancellation::new())
        .unwrap();
    let state = storage
        .read_frozen_indexed_resource_build(store, &frozen, key())
        .unwrap()
        .unwrap();
    store.release_frozen_read(&frozen).unwrap();
    state
}

fn run_public_build(binding: Binding, bytes: &[u8]) {
    let home = TestHome::new();
    let (store, handle) = open(&home, &FaultController::new());
    let storage = SyndicStorage::reacquire(&store).unwrap();
    committed(store.execute_current(handle.current_command(Seed {
        binding: binding.clone(),
        bytes: bytes.to_vec(),
    })));
    let initial = store
        .capture_frozen_read(&CommandCancellation::new())
        .unwrap();
    assert!(
        storage
            .read_frozen_indexed_resource_build(&store, &initial, key())
            .unwrap()
            .is_none()
    );
    committed(store.execute_current(storage.current_start_indexed_resource_build(key())));
    assert!(
        storage
            .read_frozen_indexed_resource_build(&store, &initial, key())
            .unwrap()
            .is_none()
    );
    store.release_frozen_read(&initial).unwrap();
    let mut finished = false;
    let mut previous_revision = 0;
    for _ in 0..500 {
        let before = state(&store, &storage);
        assert_eq!(before.key(), key());
        assert!(before.revision() > previous_revision);
        previous_revision = before.revision();
        if before.status() == api::BuildStatus::Completed {
            finished = true;
            assert!(matches!(
                storage.current_advance_indexed_resource_build(before, 3, 1),
                Err(api::IndexedResourceError::Collision)
            ));
            break;
        }
        assert_eq!(before.status(), api::BuildStatus::Building);
        committed(
            store.execute_current(
                storage
                    .current_advance_indexed_resource_build(before, 3, 1)
                    .unwrap(),
            ),
        );
    }
    assert!(finished);
    let frozen = store
        .capture_frozen_read(&CommandCancellation::new())
        .unwrap();
    let old = api::SourceAnchor {
        source: binding.source,
        block_start: binding.range.start,
        kind: binding.metadata.kind(),
        position: binding.range.start,
        visual_offset: -3,
    };
    assert!(matches!(
        storage
            .read_frozen_indexed_resource_anchor(&store, &frozen, &binding, None, old, 65_536)
            .unwrap(),
        api::IndexedResourceResult::Ready { .. }
    ));
    store.release_frozen_read(&frozen).unwrap();
    assert_eq!(store.retained_frozen_read_count().unwrap(), 0);
    assert_eq!(
        read::<ResourcesCodec>(&store, &handle, &binding.resource.id).unwrap(),
        binding.metadata
    );
    store.close().unwrap();
}

#[test]
fn public_construction_resumes_code_and_all_table_passes_to_queryable_roots() {
    run_public_build(binding(), b"one\nthree\n");
    let bytes = b"a|b\n---|---:\nc||\nx|y|z\n";
    run_public_build(fixture_support::table_build(bytes, 2, 2).binding, bytes);
}

#[test]
fn public_construction_refuses_stale_frontier_and_invalid_quantum_without_writes() {
    let home = TestHome::new();
    let (store, handle) = open(&home, &FaultController::new());
    let storage = SyndicStorage::reacquire(&store).unwrap();
    let binding = binding();
    committed(store.execute_current(handle.current_command(Seed {
        binding,
        bytes: b"one\nthree\n".to_vec(),
    })));
    committed(store.execute_current(storage.current_start_indexed_resource_build(key())));
    let stale = state(&store, &storage);
    let before = state(&store, &storage);
    committed(
        store.execute_current(
            storage
                .current_advance_indexed_resource_build(before, 3, 1)
                .unwrap(),
        ),
    );
    let revision = store.home_revision().unwrap();
    assert!(matches!(
        store.execute_current(
            storage
                .current_advance_indexed_resource_build(stale, 3, 1)
                .unwrap()
        ),
        CommandOutcome::NotCommitted { .. }
    ));
    for (bytes, descriptors) in [(65_537, 1), (3, 0), (3, 257)] {
        assert!(matches!(
            storage.current_advance_indexed_resource_build(
                state(&store, &storage),
                bytes,
                descriptors
            ),
            Err(api::IndexedResourceError::Limit)
        ));
    }
    assert_eq!(store.home_revision().unwrap(), revision);
    assert_eq!(store.retained_frozen_read_count().unwrap(), 0);
    store.close().unwrap();
}

#[test]
fn public_construction_stop_commands_persist_terminal_frontier_without_head() {
    for (stop, status) in [
        (api::StopBuild::Cancel, api::BuildStatus::Cancelled),
        (api::StopBuild::Supersede, api::BuildStatus::Superseded),
        (api::StopBuild::Refuse, api::BuildStatus::Refused),
    ] {
        let home = TestHome::new();
        let (store, handle) = open(&home, &FaultController::new());
        let storage = SyndicStorage::reacquire(&store).unwrap();
        let binding = binding();
        committed(store.execute_current(handle.current_command(Seed {
            binding: binding.clone(),
            bytes: b"one\nthree\n".to_vec(),
        })));
        committed(store.execute_current(storage.current_start_indexed_resource_build(key())));
        committed(
            store.execute_current(
                storage
                    .current_stop_indexed_resource_build(state(&store, &storage), stop)
                    .unwrap(),
            ),
        );
        let terminal = state(&store, &storage);
        assert_eq!(terminal.status(), status);
        assert!(matches!(
            storage.current_stop_indexed_resource_build(terminal, stop),
            Err(api::IndexedResourceError::Collision)
        ));
        assert!(read::<HeadsCodec>(&store, &handle, &binding.resource).is_none());
        assert!(read::<RootsCodec>(&store, &handle, &key()).is_none());
        assert_eq!(store.retained_frozen_read_count().unwrap(), 0);
        store.close().unwrap();
    }
}
