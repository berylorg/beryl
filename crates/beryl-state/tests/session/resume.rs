use super::*;
use beryl_home_store::{HomeCommand, MutationContribution};
use beryl_model::SessionRevision;
use beryl_state::{RecordRevision, ResumeSessionAfterExit};

fn references(snapshot: &MinimalSessionBootstrap) -> Vec<(WindowId, RecordRevision)> {
    snapshot
        .windows()
        .iter()
        .rev()
        .map(|window| (window.window_id(), window.revision()))
        .collect()
}

fn contribution(
    home: &HomeStore,
    session: &SessionState,
    source: &MinimalSessionBootstrap,
) -> MutationContribution {
    session.resume_after_exit(
        session.revision(home).unwrap(),
        ResumeSessionAfterExit::new(source.header().revision(), references(source)).unwrap(),
    )
}

fn publish(home: &HomeStore, session: &SessionState) -> MinimalSessionBootstrap {
    let source = snapshot(home, session);
    committed(support::execute(
        home,
        session.publish_exit(
            session.revision(home).unwrap(),
            PublishExitSession::new(source.header().revision(), entries(&source)).unwrap(),
        ),
    ));
    snapshot(home, session)
}

fn assert_resumed(before: &MinimalSessionBootstrap, after: &MinimalSessionBootstrap) {
    assert_eq!(after.header().exit_intent(), SessionExitIntent::Running);
    assert_eq!(
        after.header().revision(),
        before.header().revision().checked_next().unwrap()
    );
    assert_eq!(after.header().windows(), before.header().windows());
    assert_eq!(after.header().fallback(), before.header().fallback());
    assert_eq!(after.windows(), before.windows());
}

#[test]
fn resume_preserves_complete_windows_and_claims_through_capacity_and_reopening() {
    for count in [1, 3, MAX_RESTORABLE_WINDOWS] {
        let directory = tempfile::tempdir().unwrap();
        let (home, state) = support::open(directory.path());
        let session = state.session();
        seed(&home, &session, count);
        let source = publish(&home, &session);
        let claims = source
            .windows()
            .iter()
            .map(|window| {
                session
                    .window_claim_catalog_source(&home, window.window_id())
                    .unwrap()
                    .claim()
            })
            .collect::<Vec<_>>();
        home.close().unwrap();

        let (home, state) = support::open(directory.path());
        let fresh = state.session();
        let foreign = session.resume_after_exit(
            fresh.revision(&home).unwrap(),
            ResumeSessionAfterExit::new(source.header().revision(), references(&source)).unwrap(),
        );
        assert!(matches!(
            support::execute(&home, foreign),
            CommandOutcome::NotCommitted { .. }
        ));
        assert_same(&source, &snapshot(&home, &fresh));
        committed(support::execute(
            &home,
            contribution(&home, &fresh, &source),
        ));
        let resumed = snapshot(&home, &fresh);
        assert_resumed(&source, &resumed);
        for (window, claim) in resumed.windows().iter().zip(&claims) {
            assert_eq!(
                &fresh
                    .window_claim_catalog_source(&home, window.window_id())
                    .unwrap()
                    .claim(),
                claim
            );
        }
        home.close().unwrap();
        let (home, state) = support::open(directory.path());
        assert_same(&resumed, &snapshot(&home, &state.session()));
        for (window, claim) in resumed.windows().iter().zip(claims) {
            assert_eq!(
                state
                    .session()
                    .window_claim_catalog_source(&home, window.window_id())
                    .unwrap()
                    .claim(),
                claim
            );
        }
        home.close().unwrap();
    }
}

#[test]
fn resume_rejects_malformed_missing_running_and_changed_source() {
    let revision = SessionRevision::new(1).unwrap();
    let pair = (
        WindowId::from_bytes([0; 16]),
        RecordRevision::new(1).unwrap(),
    );
    for supplied in [
        vec![],
        vec![pair; 2],
        vec![pair; MAX_RESTORABLE_WINDOWS + 1],
    ] {
        assert!(matches!(
            ResumeSessionAfterExit::new(revision, supplied),
            Err(SessionMutationError::InvalidResumeWindowSet)
        ));
    }
    let directory = tempfile::tempdir().unwrap();
    let (home, state) = support::open(directory.path());
    let session = state.session();
    let absent = support::execute(
        &home,
        session.resume_after_exit(
            session.revision(&home).unwrap(),
            ResumeSessionAfterExit::new(revision, vec![pair]).unwrap(),
        ),
    );
    let CommandOutcome::NotCommitted { evidence } = absent else {
        panic!("{absent:?}")
    };
    assert!(matches!(
        support::contributor_source::<SessionMutationError>(&evidence),
        Some(SessionMutationError::NotInitialized)
    ));
    seed(&home, &session, 3);
    let running = snapshot(&home, &session);
    let outcome = support::execute(&home, contribution(&home, &session, &running));
    let CommandOutcome::NotCommitted { evidence } = outcome else {
        panic!("{outcome:?}")
    };
    assert!(matches!(
        support::contributor_source::<SessionMutationError>(&evidence),
        Some(SessionMutationError::NotOrderlyExit)
    ));
    assert_same(&running, &snapshot(&home, &session));
    let source = publish(&home, &session);
    let exact = references(&source);
    let mut stale = exact.clone();
    stale[0].1 = RecordRevision::new(stale[0].1.get() + 1).unwrap();
    let mut foreign = exact.clone();
    foreign[0].0 = WindowId::from_bytes([255; 16]);
    let mut extra = exact.clone();
    extra.push((WindowId::from_bytes([255; 16]), pair.1));
    for supplied in [exact[1..].to_vec(), stale, foreign, extra] {
        let outcome = support::execute(
            &home,
            session.resume_after_exit(
                session.revision(&home).unwrap(),
                ResumeSessionAfterExit::new(source.header().revision(), supplied).unwrap(),
            ),
        );
        let CommandOutcome::NotCommitted { evidence } = outcome else {
            panic!("{outcome:?}")
        };
        assert!(matches!(
            support::contributor_source::<SessionMutationError>(&evidence),
            Some(SessionMutationError::InvalidResumeWindowSet)
        ));
        assert_same(&source, &snapshot(&home, &session));
    }
    let stale_session = support::execute(
        &home,
        session.resume_after_exit(
            session.revision(&home).unwrap(),
            ResumeSessionAfterExit::new(running.header().revision(), exact).unwrap(),
        ),
    );
    let CommandOutcome::NotCommitted { evidence } = stale_session else {
        panic!("{stale_session:?}")
    };
    assert!(matches!(
        support::contributor_source::<SessionMutationError>(&evidence),
        Some(SessionMutationError::SessionRevisionConflict { .. })
    ));
    assert_same(&source, &snapshot(&home, &session));
    home.close().unwrap();
}

#[test]
fn resume_rejects_writer_drift_and_duplicate_execution() {
    let directory = tempfile::tempdir().unwrap();
    let (home, state) = support::open(directory.path());
    let session = state.session();
    seed(&home, &session, 1);
    let source = publish(&home, &session);
    let mut stale_home = HomeCommand::new(home.home_revision().unwrap());
    stale_home
        .add(contribution(&home, &session, &source))
        .unwrap();
    let stale_domain = contribution(&home, &session, &source);
    committed(support::execute(
        &home,
        contribution(&home, &session, &source),
    ));
    let resumed = snapshot(&home, &session);
    assert!(matches!(
        home.execute(stale_home),
        CommandOutcome::NotCommitted { .. }
    ));
    assert!(matches!(
        support::execute(&home, stale_domain),
        CommandOutcome::NotCommitted { .. }
    ));
    assert!(matches!(
        support::execute(&home, contribution(&home, &session, &source)),
        CommandOutcome::NotCommitted { .. }
    ));
    let already_running = support::execute(&home, contribution(&home, &session, &resumed));
    let CommandOutcome::NotCommitted { evidence } = already_running else {
        panic!("{already_running:?}")
    };
    assert!(matches!(
        support::contributor_source::<SessionMutationError>(&evidence),
        Some(SessionMutationError::NotOrderlyExit)
    ));
    assert_same(&resumed, &snapshot(&home, &session));
    home.close().unwrap();
}

#[test]
fn resume_retains_noncommit_postcommit_and_ambiguous_outcomes() {
    for fault in [
        FaultPoint::BeforeCommit,
        FaultPoint::AfterPersist,
        FaultPoint::AfterCommitBeforePersist,
    ] {
        let directory = tempfile::tempdir().unwrap();
        let faults = FaultController::new();
        let mut candidate = HomeOpenCandidate::open_with_faults(
            HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
            faults.clone(),
        )
        .unwrap();
        let state = BerylState::register(&mut candidate).unwrap();
        let home = candidate
            .prepare_publication(BerylState::required_domains().unwrap())
            .unwrap()
            .publish()
            .unwrap();
        let session = state.session();
        seed(&home, &session, 3);
        let source = publish(&home, &session);
        let claims = source
            .windows()
            .iter()
            .map(|window| {
                session
                    .window_claim_catalog_source(&home, window.window_id())
                    .unwrap()
                    .claim()
            })
            .collect::<Vec<_>>();
        let prepared = contribution(&home, &session, &source);
        faults.fail_next(fault);
        let outcome = support::execute(&home, prepared);
        match (fault, outcome) {
            (FaultPoint::BeforeCommit, CommandOutcome::NotCommitted { .. }) => {}
            (
                FaultPoint::AfterPersist,
                CommandOutcome::Committed {
                    later_failure: Some(_),
                    local_finalization: Some(_),
                    ..
                },
            ) => {}
            (
                FaultPoint::AfterCommitBeforePersist,
                CommandOutcome::Indeterminate { reconciliation, .. },
            ) => {
                let handle = reconciliation.install_and_handle();
                assert_eq!(home.pending_reconciliations().len(), 1);
                assert!(matches!(
                    home.retry_reconciliation(&handle).unwrap(),
                    ReconciliationResolution::ExactNew { .. }
                ));
                assert!(home.pending_reconciliations().is_empty());
            }
            (_, outcome) => panic!("unexpected resume outcome: {outcome:?}"),
        }
        home.close().unwrap();
        let (home, state) = support::open(directory.path());
        let after = snapshot(&home, &state.session());
        if matches!(fault, FaultPoint::BeforeCommit) {
            assert_same(&source, &after);
        } else {
            assert_resumed(&source, &after);
        }
        for (window, claim) in after.windows().iter().zip(claims) {
            assert_eq!(
                state
                    .session()
                    .window_claim_catalog_source(&home, window.window_id())
                    .unwrap()
                    .claim(),
                claim
            );
        }
        home.close().unwrap();
    }
}
