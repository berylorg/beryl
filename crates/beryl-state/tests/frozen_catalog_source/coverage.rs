use super::*;

#[test]
fn frozen_coverage_rejects_primary_only_and_recency_only_orphans_without_omission() {
    for remove_primary in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let (store, state) = support::open(directory.path());
        let row = seed_rows(&store, &state, 1).pop().unwrap();
        committed(support::execute(
            &store,
            state.catalog().remove_copy_for_test(
                state.catalog().revision(&store).unwrap(),
                row,
                remove_primary,
            ),
        ));
        let frozen = store
            .capture_frozen_read(&CommandCancellation::new())
            .unwrap();
        if remove_primary {
            assert!(
                state
                    .catalog()
                    .frozen_recency_page(&store, &frozen, None, limits(1))
                    .is_err()
            );
        } else {
            assert!(
                state
                    .catalog()
                    .frozen_primary_page(&store, &frozen, None, limits(1))
                    .is_err()
            );
            assert!(
                state
                    .catalog()
                    .frozen_row(
                        &store,
                        &frozen,
                        thread(1),
                        CatalogPointReadLimit::schema_maximum()
                    )
                    .is_err()
            );
        }
        store.release_frozen_read(&frozen).unwrap();
    }
}

#[test]
fn frozen_coverage_rejects_wrong_primary_identity_and_wrong_recency_identity() {
    for corrupt_primary in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let (store, state) = support::open(directory.path());
        let rows = seed_rows(&store, &state, 2);
        let catalog = state.catalog();
        let mutation = if corrupt_primary {
            catalog.corrupt_primary_copy_for_test(
                catalog.revision(&store).unwrap(),
                thread(1),
                rows[1].clone(),
            )
        } else {
            catalog.corrupt_recency_copy_for_test(
                catalog.revision(&store).unwrap(),
                rows[0].recency_cursor(),
                rows[1].clone(),
            )
        };
        committed(support::execute(&store, mutation));
        let frozen = store
            .capture_frozen_read(&CommandCancellation::new())
            .unwrap();
        assert!(
            catalog
                .frozen_primary_page(&store, &frozen, None, limits(2))
                .is_err()
        );
        assert!(
            catalog
                .frozen_recency_page(&store, &frozen, None, limits(2))
                .is_err()
        );
        store.release_frozen_read(&frozen).unwrap();
    }
}

#[test]
fn frozen_coverage_rejects_full_fact_copy_disagreement_and_propagates_page_limits() {
    let directory = tempfile::tempdir().unwrap();
    let (store, state) = support::open(directory.path());
    let row = seed_rows(&store, &state, 1).pop().unwrap();
    let frozen = store
        .capture_frozen_read(&CommandCancellation::new())
        .unwrap();
    assert!(
        state
            .catalog()
            .frozen_primary_page(&store, &frozen, None, CursorReadLimits::new(1, 1).unwrap())
            .is_err()
    );
    store.release_frozen_read(&frozen).unwrap();
    publish(
        &store,
        &state,
        1,
        CatalogRowExpectation::Revision(row.revision()),
        2,
        100,
    );
    let replacement = state
        .catalog()
        .row(&store, thread(1), CatalogPointReadLimit::schema_maximum())
        .unwrap()
        .unwrap();
    assert_eq!(row.recency_cursor(), replacement.recency_cursor());
    committed(support::execute(
        &store,
        state.catalog().corrupt_recency_copy_for_test(
            state.catalog().revision(&store).unwrap(),
            row.recency_cursor(),
            row,
        ),
    ));
    let frozen = store
        .capture_frozen_read(&CommandCancellation::new())
        .unwrap();
    assert!(
        state
            .catalog()
            .frozen_primary_page(&store, &frozen, None, limits(1))
            .is_err()
    );
    assert!(
        state
            .catalog()
            .frozen_recency_page(&store, &frozen, None, limits(1))
            .is_err()
    );
    store.release_frozen_read(&frozen).unwrap();
}
