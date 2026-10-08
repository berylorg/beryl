mod support;

use beryl_home_store::CommandOutcome;
use beryl_model::{
    RootId, RuntimeId, SyndicThreadId, WindowBounds, WindowDisplayState, WindowId, WindowPlacement,
};
use beryl_state::{
    CreateClaimedWindow, InitializeThreadlessWindow, RememberedTarget, UpdateWindowPlacement,
    WindowClaimReplacementPreparation, WindowClaimReplacementState,
};
use support::execute;

#[cfg(feature = "test-faults")]
#[test]
fn candidate_catalog_current_rows_share_exact_primary_and_recency_validation() {
    use beryl_home_store::test_faults::{FaultController, FaultPoint};
    use beryl_state::{
        BerylState, CatalogPointReadLimit, CatalogRowExpectation, PublishCatalogRow,
    };
    use support::state_fixture::{catalog_facts, catalog_sources, open_with_faults};

    for fault in 0..5 {
        let directory = tempfile::tempdir().unwrap();
        let faults = FaultController::new();
        let (store, state) = open_with_faults(directory.path(), faults.clone());
        let catalog = state.catalog();
        for seed in [1, 2] {
            assert!(matches!(
                execute(
                    &store,
                    catalog.publish(
                        catalog.revision(&store).unwrap(),
                        PublishCatalogRow::new(
                            SyndicThreadId::from_bytes([seed; 16]),
                            CatalogRowExpectation::Missing,
                            catalog_sources(1),
                            catalog_facts(seed, 1, u64::from(seed))
                        )
                        .unwrap(),
                    )
                ),
                CommandOutcome::Committed { .. }
            ));
        }
        let thread = SyndicThreadId::from_bytes([1; 16]);
        let row = catalog
            .current_row_source(&store, thread, CatalogPointReadLimit::schema_maximum())
            .unwrap()
            .unwrap()
            .row()
            .clone();
        match fault {
            1 | 2 => {
                assert!(matches!(
                    execute(
                        &store,
                        catalog.remove_copy_for_test(
                            catalog.revision(&store).unwrap(),
                            row.clone(),
                            fault == 1
                        )
                    ),
                    CommandOutcome::Committed { .. }
                ));
            }
            3 => {
                let other = catalog
                    .current_row_source(
                        &store,
                        SyndicThreadId::from_bytes([2; 16]),
                        CatalogPointReadLimit::schema_maximum(),
                    )
                    .unwrap()
                    .unwrap()
                    .row()
                    .clone();
                assert!(matches!(
                    execute(
                        &store,
                        catalog.corrupt_recency_copy_for_test(
                            catalog.revision(&store).unwrap(),
                            row.recency_cursor(),
                            other
                        )
                    ),
                    CommandOutcome::Committed { .. }
                ));
            }
            4 => {
                assert!(matches!(
                    execute(
                        &store,
                        catalog.mark_stale(
                            catalog.revision(&store).unwrap(),
                            beryl_state::MarkCatalogRowStale::new(thread, row.revision())
                        )
                    ),
                    CommandOutcome::Committed { .. }
                ));
            }
            _ => {}
        }
        faults.fail_next(FaultPoint::BeforeReadConfirmation);
        assert!(store.home_revision().is_err());
        let mut recovery = store.recover_same_home().unwrap();
        let fresh = BerylState::reacquire_candidate(&recovery).unwrap();
        let access = recovery.recovery_access().unwrap();
        let revision = access.home_revision().unwrap();
        assert!(
            catalog
                .current_row_source_candidate(
                    &access,
                    thread,
                    CatalogPointReadLimit::schema_maximum()
                )
                .is_err()
        );
        let foreign_directory = tempfile::tempdir().unwrap();
        let (_foreign_store, foreign) = support::open(foreign_directory.path());
        assert!(
            foreign
                .catalog()
                .current_row_source_candidate(
                    &access,
                    thread,
                    CatalogPointReadLimit::schema_maximum()
                )
                .is_err()
        );
        let result = fresh.catalog().current_row_source_candidate(
            &access,
            thread,
            CatalogPointReadLimit::schema_maximum(),
        );
        match fault {
            0 => assert_eq!(result.unwrap().unwrap().row(), &row),
            1 => assert!(result.unwrap().is_none()),
            _ => assert!(result.is_err()),
        }
        assert_eq!(access.home_revision().unwrap(), revision);
        let store = recovery.publish().unwrap();
        assert_eq!(store.home_revision().unwrap(), revision);
    }
}

#[cfg(feature = "test-faults")]
#[test]
fn candidate_replacement_classifies_exact_old_new_and_rejects_partial_or_changed_sources() {
    use beryl_home_store::{
        HomeOpenCandidate, HomeOpenOptions, HomeSchemaVersion,
        test_faults::{FaultController, FaultPoint},
    };
    use beryl_state::BerylState;

    for cut in 0..5 {
        let directory = tempfile::tempdir().unwrap();
        let faults = FaultController::new();
        let mut candidate = HomeOpenCandidate::open_with_faults(
            HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
            faults.clone(),
        )
        .unwrap();
        let old_state = BerylState::register(&mut candidate).unwrap();
        let store = candidate
            .prepare_publication(BerylState::required_domains().unwrap())
            .unwrap()
            .publish()
            .unwrap();
        let session = old_state.session();
        let window = WindowId::from_bytes([1; 16]);
        let first = SyndicThreadId::from_bytes([1; 16]);
        let second = SyndicThreadId::from_bytes([2; 16]);
        assert!(matches!(
            execute(
                &store,
                session.initialize_threadless(
                    session.revision(&store).unwrap(),
                    InitializeThreadlessWindow::new(window, placement()),
                )
            ),
            CommandOutcome::Committed { .. }
        ));
        let WindowClaimReplacementPreparation::Prepared(initial) = session
            .prepare_window_claim_replacement(&store, window, None, target(), first)
            .unwrap()
        else {
            panic!("initial replacement")
        };
        assert!(matches!(
            execute(&store, initial.contribution(&session, &store).unwrap()),
            CommandOutcome::Committed { .. }
        ));
        let WindowClaimReplacementPreparation::Prepared(prepared) = session
            .prepare_window_claim_replacement(
                &store,
                window,
                Some(initial.future_selection()),
                target(),
                second,
            )
            .unwrap()
        else {
            panic!("replacement")
        };
        if cut == 1 || cut == 3 {
            assert!(matches!(
                execute(&store, prepared.contribution(&session, &store).unwrap()),
                CommandOutcome::Committed { .. }
            ));
        }
        if cut == 2 || cut == 3 {
            let selected = if cut == 2 { first } else { second };
            assert!(matches!(
                execute(
                    &store,
                    session.delete_thread_claim_copy_for_test(
                        session.revision(&store).unwrap(),
                        window,
                        selected,
                    )
                ),
                CommandOutcome::Committed { .. }
            ));
        }
        if cut == 4 {
            assert!(matches!(
                execute(
                    &store,
                    session.update_placement(
                        session.revision(&store).unwrap(),
                        UpdateWindowPlacement::new(
                            session
                                .minimal_bootstrap(&store)
                                .unwrap()
                                .unwrap()
                                .header()
                                .revision(),
                            window,
                            initial.future_window().revision(),
                            WindowPlacement::new(
                                WindowBounds::new(1, 0, 900, 700).unwrap(),
                                WindowDisplayState::Normal,
                                None,
                                None,
                            ),
                        ),
                    )
                ),
                CommandOutcome::Committed { .. }
            ));
        }
        faults.fail_next(FaultPoint::BeforeReadConfirmation);
        assert!(store.home_revision().is_err());
        let mut recovery = store.recover_same_home().unwrap();
        let fresh = BerylState::reacquire_candidate(&recovery).unwrap();
        let access = recovery.recovery_access().unwrap();
        let revision = access.home_revision().unwrap();
        assert!(
            session
                .classify_window_claim_replacement_candidate(&access, &prepared)
                .is_err()
        );
        let foreign_directory = tempfile::tempdir().unwrap();
        let (_foreign_store, foreign) = support::open(foreign_directory.path());
        assert!(
            foreign
                .session()
                .classify_window_claim_replacement_candidate(&access, &prepared)
                .is_err()
        );
        let expected = match cut {
            0 => WindowClaimReplacementState::Original,
            1 => WindowClaimReplacementState::Committed,
            _ => WindowClaimReplacementState::Collision,
        };
        for _ in 0..2 {
            assert_eq!(
                fresh
                    .session()
                    .classify_window_claim_replacement_candidate(&access, &prepared)
                    .unwrap(),
                expected
            );
        }
        assert_eq!(access.home_revision().unwrap(), revision);
        let store = recovery.publish().unwrap();
        assert_eq!(store.home_revision().unwrap(), revision);
    }
}

fn placement() -> WindowPlacement {
    WindowPlacement::new(
        WindowBounds::new(0, 0, 900, 700).unwrap(),
        WindowDisplayState::Normal,
        None,
        None,
    )
}

fn target() -> RememberedTarget {
    RememberedTarget::new(RuntimeId::from_bytes([1; 16]), RootId::from_bytes([2; 16]))
}

#[test]
fn prepared_replacement_derives_writer_selection_and_audits_both_claim_copies() {
    let directory = tempfile::tempdir().unwrap();
    let (store, state) = support::open(directory.path());
    let session = state.session();
    let window = WindowId::from_bytes([1; 16]);
    let first = SyndicThreadId::from_bytes([1; 16]);
    let second = SyndicThreadId::from_bytes([2; 16]);
    assert!(matches!(
        execute(
            &store,
            session.initialize_threadless(
                session.revision(&store).unwrap(),
                InitializeThreadlessWindow::new(window, placement())
            )
        ),
        CommandOutcome::Committed { .. }
    ));
    let WindowClaimReplacementPreparation::Prepared(initial) = session
        .prepare_window_claim_replacement(&store, window, None, target(), first)
        .unwrap()
    else {
        panic!("initial preparation")
    };
    assert_eq!(
        session
            .classify_window_claim_replacement(&store, &initial)
            .unwrap(),
        WindowClaimReplacementState::Original
    );
    assert!(matches!(
        execute(&store, initial.contribution(&session, &store).unwrap()),
        CommandOutcome::Committed { .. }
    ));
    assert_eq!(
        session
            .classify_window_claim_replacement(&store, &initial)
            .unwrap(),
        WindowClaimReplacementState::Committed
    );
    let WindowClaimReplacementPreparation::Prepared(replacement) = session
        .prepare_window_claim_replacement(
            &store,
            window,
            Some(initial.future_selection()),
            target(),
            second,
        )
        .unwrap()
    else {
        panic!("replacement preparation")
    };
    assert_eq!(replacement.prior_claim(), Some(initial.future_claim()));
    assert_eq!(
        replacement.future_selection().revision(),
        initial
            .future_selection()
            .revision()
            .checked_next()
            .unwrap()
    );
    assert_eq!(
        replacement.future_selection().generation(),
        initial
            .future_selection()
            .generation()
            .checked_next()
            .unwrap()
    );
    assert!(matches!(
        execute(&store, replacement.contribution(&session, &store).unwrap()),
        CommandOutcome::Committed { .. }
    ));
    assert_eq!(
        session
            .classify_window_claim_replacement(&store, &replacement)
            .unwrap(),
        WindowClaimReplacementState::Committed
    );
    assert_eq!(
        session
            .window_claim_catalog_source(&store, window)
            .unwrap()
            .claim(),
        Some(replacement.future_claim())
    );
    assert_eq!(
        session
            .thread_claim_catalog_source(&store, first)
            .unwrap()
            .claim(),
        None
    );
    assert!(
        matches!(session.prepare_window_claim_replacement(&store, window, Some(replacement.future_selection()), target(), second).unwrap(), WindowClaimReplacementPreparation::Current { claim, window: current } if claim == replacement.future_claim() && &current == replacement.future_window())
    );
    assert!(
        session
            .prepare_window_claim_replacement(
                &store,
                window,
                Some(initial.future_selection()),
                target(),
                first
            )
            .is_err()
    );
}

#[test]
fn preparation_proves_elsewhere_and_fences_foreign_home_and_session_drift() {
    let directory = tempfile::tempdir().unwrap();
    let (store, state) = support::open(directory.path());
    let session = state.session();
    let first_window = WindowId::from_bytes([1; 16]);
    let second_window = WindowId::from_bytes([2; 16]);
    let first = SyndicThreadId::from_bytes([1; 16]);
    let second = SyndicThreadId::from_bytes([2; 16]);
    assert!(matches!(
        execute(
            &store,
            session.initialize_threadless(
                session.revision(&store).unwrap(),
                InitializeThreadlessWindow::new(first_window, placement())
            )
        ),
        CommandOutcome::Committed { .. }
    ));
    let WindowClaimReplacementPreparation::Prepared(initial) = session
        .prepare_window_claim_replacement(&store, first_window, None, target(), first)
        .unwrap()
    else {
        panic!("initial preparation")
    };
    assert!(matches!(
        execute(&store, initial.contribution(&session, &store).unwrap()),
        CommandOutcome::Committed { .. }
    ));
    let bootstrap = session.minimal_bootstrap(&store).unwrap().unwrap();
    assert!(matches!(
        execute(
            &store,
            session.create_claimed_window(
                session.revision(&store).unwrap(),
                CreateClaimedWindow::new(
                    bootstrap.header().revision(),
                    second_window,
                    target(),
                    second,
                    placement()
                )
            )
        ),
        CommandOutcome::Committed { .. }
    ));
    let prior = session
        .capture_window_removal(&store, first_window)
        .unwrap();
    assert!(
        matches!(session.prepare_window_claim_replacement(&store, first_window, prior.window().selected_thread(), target(), second).unwrap(), WindowClaimReplacementPreparation::ClaimedElsewhere { claim } if claim.window_id() == second_window)
    );
    let WindowClaimReplacementPreparation::Prepared(prepared) = session
        .prepare_window_claim_replacement(
            &store,
            first_window,
            prior.window().selected_thread(),
            target(),
            SyndicThreadId::from_bytes([3; 16]),
        )
        .unwrap()
    else {
        panic!("replacement preparation")
    };
    let foreign_directory = tempfile::tempdir().unwrap();
    let (foreign, foreign_state) = support::open(foreign_directory.path());
    assert!(
        prepared
            .contribution(&foreign_state.session(), &foreign)
            .is_err()
    );
    assert!(
        session
            .classify_window_claim_replacement(&foreign, &prepared)
            .is_err()
    );
    assert!(matches!(
        execute(
            &store,
            session.update_placement(
                session.revision(&store).unwrap(),
                UpdateWindowPlacement::new(
                    prior.header().revision(),
                    first_window,
                    prior.window().revision(),
                    WindowPlacement::new(
                        WindowBounds::new(20, 0, 900, 700).unwrap(),
                        WindowDisplayState::Normal,
                        None,
                        None
                    )
                )
            )
        ),
        CommandOutcome::Committed { .. }
    ));
    assert!(prepared.contribution(&session, &store).is_err());
    assert_eq!(
        session
            .classify_window_claim_replacement(&store, &prepared)
            .unwrap(),
        WindowClaimReplacementState::Collision
    );
}
