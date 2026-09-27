mod support;

#[path = "session/resume.rs"]
mod resume;

use beryl_home_store::{
    CommandOutcome, HomeOpenCandidate, HomeOpenOptions, HomeSchemaVersion, HomeStore,
    ReconciliationResolution,
    test_faults::{FaultController, FaultPoint},
};
use beryl_model::WindowId;
use beryl_state::{
    BerylState, ExitWindowPlacement, MAX_RESTORABLE_WINDOWS, MinimalSessionBootstrap,
    PublishExitSession, SessionExitIntent, SessionMutationError, SessionState,
    UpdateWindowPlacement,
};

use support::session_exit::*;

#[test]
fn complete_exit_sets_preserve_claims_and_survive_reopening_at_capacity() {
    for count in [1, 3, MAX_RESTORABLE_WINDOWS] {
        let directory = tempfile::tempdir().unwrap();
        let (home, state) = support::open(directory.path());
        let session = state.session();
        seed(&home, &session, count);
        let before = snapshot(&home, &session);
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
        committed(support::execute(
            &home,
            session.publish_exit(
                session.revision(&home).unwrap(),
                PublishExitSession::new(before.header().revision(), entries(&before)).unwrap(),
            ),
        ));
        let after = snapshot(&home, &session);
        assert_eq!(after.header().exit_intent(), SessionExitIntent::OrderlyExit);
        assert_eq!(
            after.header().revision(),
            before.header().revision().checked_next().unwrap()
        );
        assert_eq!(after.header().fallback(), before.header().fallback());
        assert_eq!(after.windows().len(), count);
        for (index, (old, new)) in before.windows().iter().zip(after.windows()).enumerate() {
            assert_eq!(new.window_id(), old.window_id());
            assert_eq!(new.revision().get(), old.revision().get() + 1);
            assert_eq!(new.selected_thread(), old.selected_thread());
            assert_eq!(new.remembered_target(), old.remembered_target());
            assert_eq!(
                new.placement(),
                &if index == 0 {
                    old.placement().clone()
                } else {
                    placement(700 + index as i32)
                }
            );
            assert_eq!(
                session
                    .window_claim_catalog_source(&home, new.window_id())
                    .unwrap()
                    .claim(),
                claims[index]
            );
        }
        let outcome = support::execute(
            &home,
            session.publish_exit(
                session.revision(&home).unwrap(),
                PublishExitSession::new(after.header().revision(), entries(&after)).unwrap(),
            ),
        );
        let CommandOutcome::NotCommitted { evidence } = outcome else {
            panic!("{outcome:?}")
        };
        assert!(matches!(
            support::contributor_source::<SessionMutationError>(&evidence),
            Some(SessionMutationError::OrderlyExitInProgress)
        ));
        assert_same(&after, &snapshot(&home, &session));
        home.close().unwrap();
        let (reopened, state) = support::open(directory.path());
        assert_same(&after, &snapshot(&reopened, &state.session()));
        for (window, claim) in after.windows().iter().zip(claims) {
            assert_eq!(
                state
                    .session()
                    .window_claim_catalog_source(&reopened, window.window_id())
                    .unwrap()
                    .claim(),
                claim
            );
        }
        reopened.close().unwrap();
    }
}

#[test]
fn malformed_and_stale_exit_sets_publish_nothing() {
    let directory = tempfile::tempdir().unwrap();
    let (home, state) = support::open(directory.path());
    let session = state.session();
    seed(&home, &session, 3);
    let before = snapshot(&home, &session);
    let revision = before.header().revision();
    let entry = || {
        ExitWindowPlacement::new(
            before.windows()[0].window_id(),
            before.windows()[0].revision(),
            placement(800),
        )
    };
    assert!(matches!(
        PublishExitSession::new(revision, vec![]),
        Err(SessionMutationError::InvalidExitWindowSet)
    ));
    assert!(matches!(
        PublishExitSession::new(revision, vec![entry(), entry()]),
        Err(SessionMutationError::InvalidExitWindowSet)
    ));
    assert!(matches!(
        PublishExitSession::new(revision, (0..257).map(|_| entry()).collect()),
        Err(SessionMutationError::InvalidExitWindowSet)
    ));
    for variant in 0..4 {
        let mut values = entries(&before);
        match variant {
            0 => {
                values.pop();
            }
            1 => values.push(ExitWindowPlacement::new(
                WindowId::from_bytes([99; 16]),
                before.windows()[0].revision(),
                placement(800),
            )),
            2 => {
                values[0] = ExitWindowPlacement::new(
                    WindowId::from_bytes([99; 16]),
                    before.windows()[0].revision(),
                    placement(800),
                )
            }
            _ => {
                values[0] = ExitWindowPlacement::new(
                    before.windows()[2].window_id(),
                    beryl_state::RecordRevision::new(before.windows()[2].revision().get() + 1)
                        .unwrap(),
                    placement(800),
                )
            }
        }
        let outcome = support::execute(
            &home,
            session.publish_exit(
                session.revision(&home).unwrap(),
                PublishExitSession::new(revision, values).unwrap(),
            ),
        );
        let CommandOutcome::NotCommitted { evidence } = outcome else {
            panic!("{outcome:?}")
        };
        assert!(matches!(
            support::contributor_source::<SessionMutationError>(&evidence),
            Some(
                SessionMutationError::InvalidExitWindowSet
                    | SessionMutationError::WindowRevisionConflict { .. }
            )
        ));
        assert_same(&before, &snapshot(&home, &session));
    }
    let stale_domain = session.revision(&home).unwrap();
    committed(support::execute(
        &home,
        session.update_placement(
            stale_domain,
            UpdateWindowPlacement::new(
                revision,
                before.windows()[0].window_id(),
                before.windows()[0].revision(),
                placement(900),
            ),
        ),
    ));
    let current = snapshot(&home, &session);
    for domain in [stale_domain, session.revision(&home).unwrap()] {
        assert!(matches!(
            support::execute(
                &home,
                session.publish_exit(
                    domain,
                    PublishExitSession::new(revision, entries(&before)).unwrap()
                )
            ),
            CommandOutcome::NotCommitted { .. }
        ));
        assert_same(&current, &snapshot(&home, &session));
    }
    home.close().unwrap();
}

#[test]
fn ambiguous_exit_publication_reconciles_the_complete_original_set() {
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
    let before = snapshot(&home, &session);
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let outcome = support::execute(
        &home,
        session.publish_exit(
            session.revision(&home).unwrap(),
            PublishExitSession::new(before.header().revision(), entries(&before)).unwrap(),
        ),
    );
    let CommandOutcome::Indeterminate { reconciliation, .. } = outcome else {
        panic!("{outcome:?}")
    };
    let reconciliation = reconciliation.install_and_handle();
    assert_eq!(home.pending_reconciliations().len(), 1);
    assert!(matches!(
        home.retry_reconciliation(&reconciliation).unwrap(),
        ReconciliationResolution::ExactNew { .. }
    ));
    assert!(home.pending_reconciliations().is_empty());
    let after = snapshot(&home, &session);
    assert_eq!(after.header().exit_intent(), SessionExitIntent::OrderlyExit);
    for (index, window) in after.windows().iter().enumerate() {
        assert_eq!(
            window.placement(),
            &if index == 0 {
                placement(0)
            } else {
                placement(700 + index as i32)
            }
        );
    }
    home.close().unwrap();
    let (home, state) = support::open(directory.path());
    assert_same(&after, &snapshot(&home, &state.session()));
    home.close().unwrap();
}
