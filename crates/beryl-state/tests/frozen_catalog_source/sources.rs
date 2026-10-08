use super::*;
use beryl_model::Availability;

#[test]
fn frozen_runtime_root_sources_keep_full_records_and_bounded_scope_after_live_changes() {
    let directory = tempfile::tempdir().unwrap();
    let (store, state) = support::open(directory.path());
    support::create_host_runtime(
        &store,
        &state,
        10,
        11,
        r"C:\Codex\codex.exe",
        r"C:\Work\beryl",
    );
    let roots = state.runtime_roots();
    let original = roots.catalog_source(&store, runtime(), root()).unwrap();
    let frozen = store
        .capture_frozen_read(&CommandCancellation::new())
        .unwrap();
    let revision = roots.frozen_revision(&store, &frozen).unwrap();
    committed(support::execute(
        &store,
        roots.set_runtime_availability(
            roots.revision(&store).unwrap(),
            SetRuntimeAvailability::new(
                runtime(),
                original.runtime().revision(),
                AvailabilitySnapshot::unknown(),
            ),
        ),
    ));
    committed(support::execute(
        &store,
        roots.set_root_availability(
            roots.revision(&store).unwrap(),
            SetRootAvailability::new(
                root(),
                original.root().revision(),
                AvailabilitySnapshot::observed(Availability::Available, UnixMillis::new(123))
                    .unwrap(),
            ),
        ),
    ));
    support::create_host_runtime(
        &store,
        &state,
        12,
        13,
        r"C:\Other\codex.exe",
        r"C:\Other\work",
    );
    assert_ne!(
        roots.catalog_source(&store, runtime(), root()).unwrap(),
        original
    );
    assert_eq!(
        roots
            .frozen_catalog_source(&store, &frozen, runtime(), root())
            .unwrap(),
        original
    );
    assert_eq!(roots.frozen_revision(&store, &frozen).unwrap(), revision);
    let runtimes = roots
        .frozen_runtimes_page(
            &store,
            &frozen,
            None,
            CursorReadLimits::new(1, 256 * 1024).unwrap(),
        )
        .unwrap();
    assert_eq!(runtimes.records(), &[original.runtime().clone()]);
    assert!(!runtimes.has_more());
    let configured = roots
        .frozen_roots_page(
            &store,
            &frozen,
            runtime(),
            None,
            CursorReadLimits::new(1, 256 * 1024).unwrap(),
        )
        .unwrap();
    assert_eq!(configured.records(), &[original.root().clone()]);
    assert!(!configured.has_more());
    assert!(
        roots
            .frozen_catalog_source(
                &store,
                &frozen,
                RuntimeId::from_bytes([12; 16]),
                RootId::from_bytes([13; 16])
            )
            .is_err()
    );
    assert!(matches!(
        roots.frozen_catalog_source(&store, &frozen, runtime(), RootId::from_bytes([13; 16])),
        Err(RuntimeRootCatalogSourceError::RootMissing { .. })
    ));
    store.release_frozen_read(&frozen).unwrap();
}

#[test]
fn frozen_claim_absence_active_restoring_and_removal_preserve_exact_paired_records() {
    let directory = tempfile::tempdir().unwrap();
    let (store, state) = support::open(directory.path());
    let session = state.session();
    let absent = store
        .capture_frozen_read(&CommandCancellation::new())
        .unwrap();
    assert_eq!(
        session
            .frozen_thread_claim_catalog_source(&store, &absent, thread(1))
            .unwrap()
            .claim(),
        None
    );
    claim(&store, &state, 1);
    let window = WindowId::from_bytes([1; 16]);
    let original = session
        .thread_claim_catalog_source(&store, thread(1))
        .unwrap();
    assert_eq!(original.claim().unwrap().state(), ThreadClaimState::Active);
    let active = store
        .capture_frozen_read(&CommandCancellation::new())
        .unwrap();
    let bootstrap = session.minimal_bootstrap(&store).unwrap().unwrap();
    committed(support::execute(
        &store,
        session.mark_orderly_exit(
            session.revision(&store).unwrap(),
            MarkOrderlyExit::new(bootstrap.header().revision()),
        ),
    ));
    let exited = session.minimal_bootstrap(&store).unwrap().unwrap();
    committed(support::execute(
        &store,
        session.begin_restore(
            session.revision(&store).unwrap(),
            BeginSessionRestore::new(exited.header().revision()),
        ),
    ));
    let restored = session
        .thread_claim_catalog_source(&store, thread(1))
        .unwrap();
    assert_eq!(
        restored.claim().unwrap().state(),
        ThreadClaimState::Restoring
    );
    let restoring = store
        .capture_frozen_read(&CommandCancellation::new())
        .unwrap();
    let bootstrap = session.minimal_bootstrap(&store).unwrap().unwrap();
    committed(support::execute(
        &store,
        session.remove_window(
            session.revision(&store).unwrap(),
            RemoveSessionWindow::new(
                bootstrap.header().revision(),
                window,
                bootstrap.windows()[0].revision(),
                bootstrap.windows()[0].selected_thread(),
            ),
        ),
    ));
    assert_eq!(
        session
            .thread_claim_catalog_source(&store, thread(1))
            .unwrap()
            .claim(),
        None
    );
    assert_eq!(
        session
            .frozen_thread_claim_catalog_source(&store, &absent, thread(1))
            .unwrap()
            .claim(),
        None
    );
    assert_eq!(
        session
            .frozen_thread_claim_catalog_source(&store, &active, thread(1))
            .unwrap(),
        original
    );
    assert_eq!(
        session
            .frozen_window_claim_catalog_source(&store, &active, window)
            .unwrap()
            .claim(),
        original.claim()
    );
    assert_eq!(
        session
            .frozen_thread_claim_catalog_source(&store, &restoring, thread(1))
            .unwrap(),
        restored
    );
    assert_eq!(
        session
            .frozen_window_claim_catalog_source(&store, &restoring, window)
            .unwrap()
            .claim(),
        restored.claim()
    );
    for frozen in [absent, active, restoring] {
        store.release_frozen_read(&frozen).unwrap();
    }
}

#[test]
fn frozen_claim_reader_rejects_reverse_copy_disagreement_and_wrong_runtime_root_pair() {
    let directory = tempfile::tempdir().unwrap();
    let (store, state) = support::open(directory.path());
    claim(&store, &state, 1);
    let window = WindowId::from_bytes([1; 16]);
    committed(support::execute(
        &store,
        state.session().delete_thread_claim_copy_for_test(
            state.session().revision(&store).unwrap(),
            window,
            thread(1),
        ),
    ));
    let frozen = store
        .capture_frozen_read(&CommandCancellation::new())
        .unwrap();
    assert!(matches!(
        state
            .session()
            .frozen_window_claim_catalog_source(&store, &frozen, window),
        Err(ThreadClaimCatalogSourceError::ReverseCopiesDisagree { .. })
    ));
    store.release_frozen_read(&frozen).unwrap();
    support::create_host_runtime(
        &store,
        &state,
        10,
        11,
        r"C:\Codex\codex.exe",
        r"C:\Work\beryl",
    );
    support::create_host_runtime(
        &store,
        &state,
        12,
        13,
        r"C:\Other\codex.exe",
        r"C:\Other\work",
    );
    let frozen = store
        .capture_frozen_read(&CommandCancellation::new())
        .unwrap();
    assert!(matches!(
        state.runtime_roots().frozen_catalog_source(
            &store,
            &frozen,
            runtime(),
            RootId::from_bytes([13; 16])
        ),
        Err(RuntimeRootCatalogSourceError::RootRuntimeMismatch { .. })
    ));
    store.release_frozen_read(&frozen).unwrap();
}
