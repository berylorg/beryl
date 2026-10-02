use super::*;

#[test]
fn changed_original_window_header_and_restoring_sources_refuse_capture_or_removal() {
    let directory = tempfile::tempdir().unwrap();
    let (home, state) = support::open(directory.path());
    let session = state.session();
    assert!(
        session
            .capture_window_removal(&home, WindowId::from_bytes([0; 16]))
            .is_err()
    );
    seed(&home, &session, 3);
    assert!(
        session
            .capture_window_removal(&home, WindowId::from_bytes([255; 16]))
            .is_err()
    );
    let before = snapshot(&home, &session);
    let evidence = session
        .capture_window_removal(&home, before.windows()[0].window_id())
        .unwrap();
    let unrelated = &before.windows()[1];
    committed(support::execute(
        &home,
        session.update_placement(
            session.revision(&home).unwrap(),
            UpdateWindowPlacement::new(
                before.header().revision(),
                unrelated.window_id(),
                unrelated.revision(),
                placement(800),
            ),
        ),
    ));
    let changed = snapshot(&home, &session);
    assert!(matches!(
        remove(&home, &session, &evidence),
        CommandOutcome::NotCommitted { .. }
    ));
    assert_same(&changed, &snapshot(&home, &session));
    let mut candidate = recover(home);
    let fresh = BerylState::reacquire_candidate(&candidate)
        .unwrap()
        .session();
    let access = candidate.recovery_access().unwrap();
    assert_eq!(
        fresh
            .classify_window_removal_candidate(&access, &evidence)
            .unwrap(),
        SessionWindowRemovalState::Collision
    );
    drop(access);
    let home = candidate.publish().unwrap();
    let current = snapshot(&home, &fresh);
    committed(support::execute(
        &home,
        fresh.begin_restore(
            fresh.revision(&home).unwrap(),
            BeginSessionRestore::new(current.header().revision()),
        ),
    ));
    assert!(
        fresh
            .capture_window_removal(&home, evidence.window().window_id())
            .is_err()
    );
    let current = snapshot(&home, &fresh);
    committed(support::execute(
        &home,
        fresh.mark_orderly_exit(
            fresh.revision(&home).unwrap(),
            MarkOrderlyExit::new(current.header().revision()),
        ),
    ));
    assert!(
        fresh
            .capture_window_removal(&home, evidence.window().window_id())
            .is_err()
    );
    home.close().unwrap();
}

#[test]
fn conflicting_removed_membership_and_thread_claim_never_classify_as_absence() {
    let directory = tempfile::tempdir().unwrap();
    let (home, state) = support::open(directory.path());
    let session = state.session();
    seed(&home, &session, 3);
    let before = snapshot(&home, &session);
    let evidence = session
        .capture_window_removal(&home, before.windows()[0].window_id())
        .unwrap();
    committed(remove(&home, &session, &evidence));
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
    let mut candidate = recover(home);
    let fresh = BerylState::reacquire_candidate(&candidate)
        .unwrap()
        .session();
    let access = candidate.recovery_access().unwrap();
    assert_eq!(
        fresh
            .classify_window_removal_candidate(&access, &evidence)
            .unwrap(),
        SessionWindowRemovalState::Collision
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

#[test]
fn recovery_rechecks_writer_revisions_and_duplicate_execution() {
    let directory = tempfile::tempdir().unwrap();
    let (home, state) = support::open(directory.path());
    let session = state.session();
    seed(&home, &session, 3);
    let evidence = session
        .capture_window_removal(&home, WindowId::from_bytes([0; 16]))
        .unwrap();
    committed(remove(&home, &session, &evidence));
    let mut candidate = recover(home);
    let fresh = BerylState::reacquire_candidate(&candidate)
        .unwrap()
        .session();
    let access = candidate.recovery_access().unwrap();
    let stale_domain = recovery_contribution(&access, &fresh, &evidence);
    let mut stale_home = HomeCommand::new(access.home_revision().unwrap());
    stale_home
        .add(recovery_contribution(&access, &fresh, &evidence))
        .unwrap();
    committed(execute(
        &access,
        recovery_contribution(&access, &fresh, &evidence),
    ));
    assert!(matches!(
        access.execute(stale_home),
        CommandOutcome::NotCommitted { .. }
    ));
    assert!(matches!(
        execute(&access, stale_domain),
        CommandOutcome::NotCommitted { .. }
    ));
    assert_eq!(
        fresh
            .classify_window_removal_candidate(&access, &evidence)
            .unwrap(),
        SessionWindowRemovalState::Recovered
    );
    drop(access);
    candidate.publish().unwrap().close().unwrap();
}
