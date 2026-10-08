#![cfg(feature = "test-faults")]

#[path = "frozen_catalog_source/support.rs"]
mod frozen_support;
#[path = "catalog_rebuild/support.rs"]
mod rebuild_support;
mod support;

use beryl_home_store::{CommandOutcome, CursorReadLimits, HomeStore};
use beryl_model::{ClaimRevision, ProjectionRevision, RootId, RuntimeId, SyndicThreadId, WindowId};
use beryl_state::*;
use frozen_support::{committed, limits, root, runtime, seed_rows, thread};
use rebuild_support::*;

#[test]
fn exact_rebuild_accepts_recreated_summary_scalar_without_weakening_ordinary_publication() {
    let directory = tempfile::tempdir().unwrap();
    let (store, state) = support::open(directory.path());
    let original = seed_high(&store, &state, false);
    let next_sources = sources(1, 5, 5, None);
    let next_facts = facts(&original, CatalogClaimSummary::Unclaimed, 500);
    let ordinary = PublishCatalogRow::new(
        thread(1),
        CatalogRowExpectation::Revision(original.revision()),
        next_sources,
        next_facts.clone(),
    )
    .unwrap();
    let before = store.home_revision().unwrap();
    assert!(matches!(
        support::execute(
            &store,
            state
                .catalog()
                .publish(state.catalog().revision(&store).unwrap(), ordinary)
        ),
        CommandOutcome::NotCommitted { .. }
    ));
    assert_eq!(store.home_revision().unwrap(), before);
    committed(rebuild(
        &store,
        &state,
        Some(original.clone()),
        next_sources,
        next_facts.clone(),
    ));
    let current = paired(&store, &state);
    assert_eq!(current.sources(), next_sources);
    assert_eq!(current.facts(), &next_facts);
    assert_eq!(current.revision().get(), original.revision().get() + 1);
    assert_eq!(current.freshness(), CatalogFreshness::Current);
    assert_ne!(current.recency_cursor(), original.recency_cursor());
    assert_eq!(
        state
            .catalog()
            .recency_page(&store, None, limits(2))
            .unwrap()
            .rows()
            .len(),
        1
    );
}

#[test]
fn exact_rebuild_accepts_recreated_claim_scalar_and_current_optional_absence() {
    let directory = tempfile::tempdir().unwrap();
    let (store, state) = support::open(directory.path());
    let original = seed_high(&store, &state, true);
    let lower = sources(5, 5, 5, Some(1));
    let active = facts(
        &original,
        CatalogClaimSummary::claimed(WindowId::from_bytes([2; 16]), CatalogClaimKind::Active),
        100,
    );
    committed(rebuild(
        &store,
        &state,
        Some(original.clone()),
        lower,
        active.clone(),
    ));
    let current = paired(&store, &state);
    assert_eq!(current.sources(), lower);
    assert_eq!(current.facts(), &active);
    let absent = sources(5, 5, 5, None);
    committed(rebuild(
        &store,
        &state,
        Some(current.clone()),
        absent,
        facts(&current, CatalogClaimSummary::Unclaimed, 100),
    ));
    let unclaimed = paired(&store, &state);
    assert_eq!(unclaimed.sources().claim(), None);
    assert_eq!(unclaimed.facts().claim(), CatalogClaimSummary::Unclaimed);
    assert_eq!(unclaimed.revision().get(), original.revision().get() + 2);
}

#[test]
fn rebuild_full_expected_row_rejects_changed_facts_even_with_fresh_domain_fence() {
    let directory = tempfile::tempdir().unwrap();
    let (store, state) = support::open(directory.path());
    let old = seed_high(&store, &state, false);
    committed(rebuild(
        &store,
        &state,
        Some(old.clone()),
        old.sources(),
        facts(&old, CatalogClaimSummary::Unclaimed, 700),
    ));
    let current = paired(&store, &state);
    let before = store.home_revision().unwrap();
    assert!(matches!(
        rebuild(
            &store,
            &state,
            Some(old.clone()),
            sources(1, 5, 5, None),
            facts(&old, CatalogClaimSummary::Unclaimed, 800)
        ),
        CommandOutcome::NotCommitted { .. }
    ));
    assert_eq!(store.home_revision().unwrap(), before);
    assert_eq!(paired(&store, &state), current);
}

#[test]
fn rebuild_refuses_missing_or_disagreeing_original_copy_and_existing_absence_destination() {
    for damage in 0..3 {
        let directory = tempfile::tempdir().unwrap();
        let (store, state) = support::open(directory.path());
        let old = seed_high(&store, &state, false);
        let catalog = state.catalog();
        if damage == 2 {
            committed(rebuild(
                &store,
                &state,
                Some(old.clone()),
                old.sources(),
                facts(&old, CatalogClaimSummary::Unclaimed, 100),
            ));
            committed(support::execute(
                &store,
                catalog.corrupt_recency_copy_for_test(
                    catalog.revision(&store).unwrap(),
                    old.recency_cursor(),
                    old.clone(),
                ),
            ));
        } else {
            committed(support::execute(
                &store,
                catalog.remove_copy_for_test(
                    catalog.revision(&store).unwrap(),
                    old.clone(),
                    damage == 0,
                ),
            ));
        }
        let before = store.home_revision().unwrap();
        assert!(matches!(
            rebuild(
                &store,
                &state,
                Some(old.clone()),
                sources(1, 5, 5, None),
                facts(&old, CatalogClaimSummary::Unclaimed, 100)
            ),
            CommandOutcome::NotCommitted { .. }
        ));
        if damage == 0 {
            assert!(matches!(
                rebuild(&store, &state, None, old.sources(), old.facts().clone()),
                CommandOutcome::NotCommitted { .. }
            ));
        }
        assert_eq!(store.home_revision().unwrap(), before);
    }
}

#[test]
fn rebuild_rejects_runtime_root_regressions_and_execution_identity_reassignment() {
    for (runtime_revision, root_revision, new_runtime, new_root) in [
        (4, 5, 10, 11),
        (5, 4, 10, 11),
        (5, 5, 12, 11),
        (5, 5, 10, 13),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let (store, state) = support::open(directory.path());
        let old = seed_high(&store, &state, false);
        let execution = old.facts().execution();
        let changed_execution = CatalogExecutionSummary::new(
            RuntimeId::from_bytes([new_runtime; 16]),
            RootId::from_bytes([new_root; 16]),
            execution.environment_label(),
            execution.configured_executable_path().clone(),
            execution.full_root_path().clone(),
            execution.availability(),
        )
        .unwrap();
        let changed = CatalogFacts::new(
            old.title().clone(),
            changed_execution,
            old.facts().archive(),
            old.facts().last_activity_at(),
            old.facts().complete(),
            old.facts().claim(),
            old.facts().lineage(),
        )
        .unwrap();
        let before = store.home_revision().unwrap();
        assert!(matches!(
            rebuild(
                &store,
                &state,
                Some(old.clone()),
                sources(1, runtime_revision, root_revision, None),
                changed
            ),
            CommandOutcome::NotCommitted { .. }
        ));
        assert_eq!(store.home_revision().unwrap(), before);
        assert_eq!(paired(&store, &state), old);
    }
}

#[test]
fn rebuild_proven_missing_inserts_both_copies_and_refuses_second_absence() {
    let directory = tempfile::tempdir().unwrap();
    let (store, state) = support::open(directory.path());
    let source_directory = tempfile::tempdir().unwrap();
    let (source_store, source_state) = support::open(source_directory.path());
    let source = seed_rows(&source_store, &source_state, 1).pop().unwrap();
    committed(rebuild(
        &store,
        &state,
        None,
        source.sources(),
        source.facts().clone(),
    ));
    let current = paired(&store, &state);
    assert_eq!(current.revision(), CatalogRevision::INITIAL);
    assert_eq!(current.sources(), source.sources());
    assert_eq!(current.facts(), source.facts());
    let before = store.home_revision().unwrap();
    assert!(matches!(
        rebuild(
            &store,
            &state,
            None,
            source.sources(),
            source.facts().clone()
        ),
        CommandOutcome::NotCommitted { .. }
    ));
    assert_eq!(store.home_revision().unwrap(), before);
    assert_eq!(paired(&store, &state), current);
}

#[test]
fn rebuild_exhausted_catalog_revision_preserves_the_full_pair() {
    let directory = tempfile::tempdir().unwrap();
    let (store, state) = support::open(directory.path());
    seed_high(&store, &state, false);
    committed(support::execute(
        &store,
        state.catalog().set_row_revision_for_test(
            state.catalog().revision(&store).unwrap(),
            thread(1),
            CatalogRevision::new(u64::MAX).unwrap(),
        ),
    ));
    let exhausted = paired(&store, &state);
    let before = store.home_revision().unwrap();
    assert!(matches!(
        rebuild(
            &store,
            &state,
            Some(exhausted.clone()),
            sources(1, 5, 5, None),
            exhausted.facts().clone()
        ),
        CommandOutcome::NotCommitted { .. }
    ));
    assert_eq!(store.home_revision().unwrap(), before);
    assert_eq!(paired(&store, &state), exhausted);
}
