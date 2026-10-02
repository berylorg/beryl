mod support;

use beryl_home_store::test_faults::{FaultController, FaultPoint};
use beryl_home_store::{
    CommandOutcome, HomeCandidateRecoveryAccess, HomeCommand, HomeOpenCandidate, HomeOpenOptions,
    HomeRecoveryCandidate, HomeSchemaVersion, HomeStore, MutationContribution,
    ReconciliationResolution,
};
use beryl_model::{SyndicThreadId, WindowId};
use beryl_state::{
    BeginSessionRestore, BerylState, CreateClaimedWindow, MAX_RESTORABLE_WINDOWS, MarkOrderlyExit,
    MinimalSessionBootstrap, RememberedTarget, ReplaceWindowClaim, SessionState,
    SessionWindowRemovalEvidence, SessionWindowRemovalState, UpdateWindowPlacement,
};
use support::session_exit::{assert_same, committed, placement, seed, snapshot};

fn recover(home: HomeStore) -> HomeRecoveryCandidate {
    if home.health().state() == beryl_home_store::HomeHealthState::Healthy {
        home.inject_retained_maintenance_terminal();
        assert!(BerylState::reacquire(&home).is_err());
    }
    home.recover_same_home().unwrap()
}

fn execute(
    access: &HomeCandidateRecoveryAccess<'_>,
    contribution: MutationContribution,
) -> CommandOutcome {
    let mut command = HomeCommand::new(access.home_revision().unwrap());
    command.add(contribution).unwrap();
    access.execute(command)
}

fn remove(
    home: &HomeStore,
    session: &SessionState,
    evidence: &SessionWindowRemovalEvidence,
) -> CommandOutcome {
    support::execute(
        home,
        session
            .remove_captured_window(home, session.revision(home).unwrap(), evidence)
            .unwrap(),
    )
}

fn recovery_contribution(
    access: &HomeCandidateRecoveryAccess<'_>,
    session: &SessionState,
    evidence: &SessionWindowRemovalEvidence,
) -> MutationContribution {
    session
        .recover_removed_window_candidate(
            access,
            session.revision_candidate(access).unwrap(),
            evidence,
        )
        .unwrap()
}

fn assert_recovered(
    before: &MinimalSessionBootstrap,
    after: &MinimalSessionBootstrap,
    evidence: &SessionWindowRemovalEvidence,
) {
    assert_eq!(
        after.header().revision(),
        before
            .header()
            .revision()
            .checked_next()
            .unwrap()
            .checked_next()
            .unwrap()
    );
    assert_eq!(after.header().fallback(), before.header().fallback());
    assert_eq!(after.header().exit_intent(), before.header().exit_intent());
    assert_eq!(after.windows().len(), before.windows().len());
    for (original, restored) in before.windows().iter().zip(after.windows()) {
        assert_eq!(restored.window_id(), original.window_id());
        if original.window_id() != evidence.window().window_id() {
            assert_eq!(restored, original);
            continue;
        }
        assert_eq!(restored.revision().get(), original.revision().get() + 1);
        assert_eq!(restored.remembered_target(), original.remembered_target());
        assert_eq!(restored.placement(), original.placement());
        match (original.selected_thread(), restored.selected_thread()) {
            (Some(original), Some(restored)) => {
                assert_eq!(restored.thread_id(), original.thread_id());
                assert_eq!(restored.generation(), original.generation());
                assert_eq!(
                    restored.revision(),
                    original.revision().checked_next().unwrap()
                );
            }
            (None, None) => {}
            pair => panic!("changed selection: {pair:?}"),
        }
    }
}

#[test]
fn final_nonfinal_and_capacity_recover_exact_identity_placement_fallback_and_claims_after_reopening()
 {
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
        assert_eq!(evidence.header(), before.header());
        assert_eq!(evidence.window(), &before.windows()[count / 2]);
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
        committed(remove(&home, &session, &evidence));
        let removed = snapshot(&home, &session);
        assert_eq!(removed.windows().len(), count - 1);
        assert_eq!(removed.header().fallback(), before.header().fallback());
        assert_eq!(
            removed.header().revision(),
            before.header().revision().checked_next().unwrap()
        );
        home.close().unwrap();
        let (home, _) = support::open(directory.path());
        let mut candidate = recover(home);
        let fresh = BerylState::reacquire_candidate(&candidate)
            .unwrap()
            .session();
        {
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
            assert_eq!(
                fresh
                    .classify_window_removal_candidate(&access, &evidence)
                    .unwrap(),
                SessionWindowRemovalState::Recovered
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
            let restored = fresh.minimal_bootstrap_candidate(&access).unwrap().unwrap();
            assert_recovered(&before, &restored, &evidence);
            for (window, old_claim) in restored.windows().iter().zip(&claims) {
                let claim = fresh
                    .window_claim_catalog_source_candidate(&access, window.window_id())
                    .unwrap()
                    .claim();
                if window.window_id() == id {
                    match (*old_claim, claim) {
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
                } else {
                    assert_eq!(claim, *old_claim);
                }
            }
        }
        let home = candidate.publish().unwrap();
        let restored = snapshot(&home, &fresh);
        home.close().unwrap();
        let (home, state) = support::open(directory.path());
        assert_same(&restored, &snapshot(&home, &state.session()));
        home.close().unwrap();
    }
}

#[test]
fn original_state_never_authorizes_recovery_and_foreign_handles_or_homes_are_rejected() {
    let directory = tempfile::tempdir().unwrap();
    let foreign_directory = tempfile::tempdir().unwrap();
    let (home, state) = support::open(directory.path());
    let old_session = state.session();
    seed(&home, &old_session, 1);
    let evidence = old_session
        .capture_window_removal(&home, WindowId::from_bytes([0; 16]))
        .unwrap();
    let before = snapshot(&home, &old_session);
    let (foreign, foreign_state) = support::open(foreign_directory.path());
    seed(&foreign, &foreign_state.session(), 1);
    assert!(
        foreign_state
            .session()
            .remove_captured_window(
                &foreign,
                foreign_state.session().revision(&foreign).unwrap(),
                &evidence
            )
            .is_err()
    );
    let mut foreign = recover(foreign);
    let foreign_session = BerylState::reacquire_candidate(&foreign).unwrap().session();
    let access = foreign.recovery_access().unwrap();
    assert!(
        foreign_session
            .classify_window_removal_candidate(&access, &evidence)
            .is_err()
    );
    assert!(
        foreign_session
            .recover_removed_window_candidate(
                &access,
                foreign_session.revision_candidate(&access).unwrap(),
                &evidence
            )
            .is_err()
    );
    drop(access);
    foreign.publish().unwrap().close().unwrap();
    let mut candidate = recover(home);
    let fresh = BerylState::reacquire_candidate(&candidate)
        .unwrap()
        .session();
    let access = candidate.recovery_access().unwrap();
    assert!(
        old_session
            .classify_window_removal_candidate(&access, &evidence)
            .is_err()
    );
    assert_eq!(
        fresh
            .classify_window_removal_candidate(&access, &evidence)
            .unwrap(),
        SessionWindowRemovalState::Original
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
    assert_same(
        &before,
        &fresh.minimal_bootstrap_candidate(&access).unwrap().unwrap(),
    );
    drop(access);
    candidate.publish().unwrap().close().unwrap();
}

#[path = "window_removal_recovery/guards.rs"]
mod guards;
#[path = "window_removal_recovery/healthy.rs"]
mod healthy;
#[path = "window_removal_recovery/outcomes.rs"]
mod outcomes;
