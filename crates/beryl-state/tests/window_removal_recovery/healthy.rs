use super::*;

fn contribution(
    home: &HomeStore,
    session: &SessionState,
    evidence: &SessionWindowRemovalEvidence,
) -> MutationContribution {
    session
        .recover_removed_window(home, session.revision(home).unwrap(), evidence)
        .unwrap()
}

#[test]
fn healthy_restoration_preserves_claimed_threadless_final_nonfinal_and_capacity_members() {
    for (count, claimed) in [
        (1, false),
        (1, true),
        (3, true),
        (MAX_RESTORABLE_WINDOWS, true),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let (home, state) = support::open(directory.path());
        let session = state.session();
        seed(&home, &session, count);
        if count == 1 && claimed {
            let source = snapshot(&home, &session);
            committed(support::execute(
                &home,
                session.replace_claim(
                    session.revision(&home).unwrap(),
                    ReplaceWindowClaim::new(
                        source.header().revision(),
                        source.windows()[0].window_id(),
                        source.windows()[0].revision(),
                        None,
                        RememberedTarget::new(
                            beryl_model::RuntimeId::from_bytes([0; 16]),
                            beryl_model::RootId::from_bytes([0; 16]),
                        ),
                        SyndicThreadId::from_bytes([0; 16]),
                    ),
                ),
            ));
        }
        let before = snapshot(&home, &session);
        let id = before.windows()[count / 2].window_id();
        let evidence = session.capture_window_removal(&home, id).unwrap();
        let claims = before
            .windows()
            .iter()
            .map(|window| {
                session
                    .window_claim_catalog_source(&home, window.window_id())
                    .unwrap()
                    .claim()
            })
            .collect::<Vec<_>>();
        assert_eq!(
            session.classify_window_removal(&home, &evidence).unwrap(),
            SessionWindowRemovalState::Original
        );
        assert!(
            session
                .recover_removed_window(&home, session.revision(&home).unwrap(), &evidence)
                .is_err()
        );
        let generation = home.health().generation();
        committed(remove(&home, &session, &evidence));
        assert_eq!(
            session.classify_window_removal(&home, &evidence).unwrap(),
            SessionWindowRemovalState::Removed
        );
        committed(support::execute(
            &home,
            contribution(&home, &session, &evidence),
        ));
        assert_eq!(home.health().generation(), generation);
        assert_eq!(
            session.classify_window_removal(&home, &evidence).unwrap(),
            SessionWindowRemovalState::Recovered
        );
        assert!(
            session
                .recover_removed_window(&home, session.revision(&home).unwrap(), &evidence)
                .is_err()
        );
        let after = snapshot(&home, &session);
        assert_recovered(&before, &after, &evidence);
        for (window, old) in after.windows().iter().zip(&claims) {
            let new = session
                .window_claim_catalog_source(&home, window.window_id())
                .unwrap()
                .claim();
            if window.window_id() != id {
                assert_eq!(new, *old);
            } else {
                match (*old, new) {
                    (Some(old), Some(new)) => {
                        assert_eq!(new.window_id(), old.window_id());
                        assert_eq!(new.thread_id(), old.thread_id());
                        assert_eq!(new.generation(), old.generation());
                        assert_eq!(new.state(), old.state());
                        assert_eq!(new.revision(), old.revision().checked_next().unwrap());
                    }
                    (None, None) => {}
                    pair => panic!("changed claim: {pair:?}"),
                }
            }
        }
        home.close().unwrap();
        let (home, state) = support::open(directory.path());
        assert_same(&after, &snapshot(&home, &state.session()));
        home.close().unwrap();
    }
}

#[test]
fn healthy_access_rejects_foreign_home_stale_handle_and_original_generation_replacement() {
    let directory = tempfile::tempdir().unwrap();
    let foreign_directory = tempfile::tempdir().unwrap();
    let (home, state) = support::open(directory.path());
    let session = state.session();
    seed(&home, &session, 3);
    let evidence = session
        .capture_window_removal(&home, WindowId::from_bytes([0; 16]))
        .unwrap();
    committed(remove(&home, &session, &evidence));
    let removed = snapshot(&home, &session);
    let (foreign, foreign_state) = support::open(foreign_directory.path());
    let foreign_session = foreign_state.session();
    seed(&foreign, &foreign_session, 3);
    assert!(
        foreign_session
            .classify_window_removal(&foreign, &evidence)
            .is_err()
    );
    assert!(
        foreign_session
            .recover_removed_window(
                &foreign,
                foreign_session.revision(&foreign).unwrap(),
                &evidence
            )
            .is_err()
    );
    foreign.close().unwrap();
    home.close().unwrap();
    let (home, state) = support::open(directory.path());
    let fresh = state.session();
    assert!(session.classify_window_removal(&home, &evidence).is_err());
    assert!(fresh.classify_window_removal(&home, &evidence).is_err());
    assert!(
        fresh
            .recover_removed_window(&home, fresh.revision(&home).unwrap(), &evidence)
            .is_err()
    );
    assert!(
        fresh
            .remove_captured_window(&home, fresh.revision(&home).unwrap(), &evidence)
            .is_err()
    );
    assert_same(&removed, &snapshot(&home, &fresh));
    let mut candidate = recover(home);
    let fresh = BerylState::reacquire_candidate(&candidate)
        .unwrap()
        .session();
    let access = candidate.recovery_access().unwrap();
    assert_eq!(
        fresh
            .classify_window_removal_candidate(&access, &evidence)
            .unwrap(),
        SessionWindowRemovalState::Removed
    );
    committed(execute(
        &access,
        recovery_contribution(&access, &fresh, &evidence),
    ));
    drop(access);
    candidate.publish().unwrap().close().unwrap();
}

#[test]
fn healthy_restoration_rechecks_home_domain_and_exact_records_after_preparation() {
    let directory = tempfile::tempdir().unwrap();
    let (home, state) = support::open(directory.path());
    let session = state.session();
    seed(&home, &session, 3);
    let evidence = session
        .capture_window_removal(&home, WindowId::from_bytes([0; 16]))
        .unwrap();
    committed(remove(&home, &session, &evidence));
    let mut stale_home = HomeCommand::new(home.home_revision().unwrap());
    stale_home
        .add(contribution(&home, &session, &evidence))
        .unwrap();
    let unaffected_domain = contribution(&home, &session, &evidence);
    committed(support::execute(
        &home,
        state.runtime_roots().create_runtime_with_home_root(
            state.runtime_roots().revision(&home).unwrap(),
            support::host_runtime(9, 10, r"C:\runtime.exe", r"C:\root"),
        ),
    ));
    assert!(matches!(
        home.execute(stale_home),
        CommandOutcome::NotCommitted { .. }
    ));
    let stale_domain = contribution(&home, &session, &evidence);
    committed(support::execute(&home, unaffected_domain));
    assert!(matches!(
        support::execute(&home, stale_domain),
        CommandOutcome::NotCommitted { .. }
    ));
    assert_eq!(
        session.classify_window_removal(&home, &evidence).unwrap(),
        SessionWindowRemovalState::Recovered
    );
    home.close().unwrap();

    let directory = tempfile::tempdir().unwrap();
    let (home, state) = support::open(directory.path());
    let session = state.session();
    seed(&home, &session, 3);
    let evidence = session
        .capture_window_removal(&home, WindowId::from_bytes([0; 16]))
        .unwrap();
    committed(remove(&home, &session, &evidence));
    let stale = contribution(&home, &session, &evidence);
    let removed = snapshot(&home, &session);
    committed(support::execute(
        &home,
        session.create_claimed_window(
            session.revision(&home).unwrap(),
            CreateClaimedWindow::new(
                removed.header().revision(),
                WindowId::from_bytes([9; 16]),
                evidence.window().remembered_target().unwrap(),
                evidence.claim().unwrap().thread_id(),
                placement(99),
            ),
        ),
    ));
    let conflict = snapshot(&home, &session);
    assert_eq!(
        session.classify_window_removal(&home, &evidence).unwrap(),
        SessionWindowRemovalState::Collision
    );
    assert!(
        session
            .recover_removed_window(&home, session.revision(&home).unwrap(), &evidence)
            .is_err()
    );
    assert!(matches!(
        support::execute(&home, stale),
        CommandOutcome::NotCommitted { .. }
    ));
    assert_same(&conflict, &snapshot(&home, &session));
    home.close().unwrap();
}

#[test]
fn healthy_to_failed_handoff_classifies_already_committed_restoration_without_duplicate_write() {
    for restore_first in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let (home, state) = support::open(directory.path());
        let session = state.session();
        seed(&home, &session, 3);
        let before = snapshot(&home, &session);
        let evidence = session
            .capture_window_removal(&home, before.windows()[0].window_id())
            .unwrap();
        committed(remove(&home, &session, &evidence));
        if restore_first {
            committed(support::execute(
                &home,
                contribution(&home, &session, &evidence),
            ));
        }
        let revision = session.revision(&home).unwrap();
        home.inject_retained_maintenance_terminal();
        assert!(BerylState::reacquire(&home).is_err());
        assert!(session.classify_window_removal(&home, &evidence).is_err());
        assert!(
            session
                .recover_removed_window(&home, revision, &evidence)
                .is_err()
        );
        let mut candidate = home.recover_same_home().unwrap();
        let fresh = BerylState::reacquire_candidate(&candidate)
            .unwrap()
            .session();
        let access = candidate.recovery_access().unwrap();
        assert_eq!(
            fresh
                .classify_window_removal_candidate(&access, &evidence)
                .unwrap(),
            if restore_first {
                SessionWindowRemovalState::Recovered
            } else {
                SessionWindowRemovalState::Removed
            }
        );
        if !restore_first {
            committed(execute(
                &access,
                recovery_contribution(&access, &fresh, &evidence),
            ));
        }
        assert_recovered(
            &before,
            &fresh.minimal_bootstrap_candidate(&access).unwrap().unwrap(),
            &evidence,
        );
        assert!(
            fresh
                .recover_removed_window_candidate(
                    &access,
                    fresh.revision_candidate(&access).unwrap(),
                    &evidence
                )
                .is_err()
        );
        drop(access);
        candidate.publish().unwrap().close().unwrap();
    }
}

#[path = "healthy/outcomes.rs"]
mod outcomes;
