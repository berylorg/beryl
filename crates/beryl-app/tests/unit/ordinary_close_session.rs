use super::*;
use beryl_home_store::test_faults::{FaultController, FaultPoint};
use beryl_home_store::{
    HomeOpenCandidate, HomeOpenOptions, HomeSchemaVersion, MutationContribution,
};
use beryl_model::{
    RootId, RuntimeId, SyndicThreadId, WindowBounds, WindowDisplayState, WindowPlacement,
};
use beryl_state::{
    BerylState, CreateClaimedWindow, InitializeThreadlessWindow, MinimalSessionBootstrap,
    RememberedTarget, ReplaceWindowClaim, UpdateWindowPlacement,
};

fn placement(seed: i32) -> WindowPlacement {
    WindowPlacement::new(
        WindowBounds::new(seed, -seed, 900, 700).unwrap(),
        WindowDisplayState::Normal,
        None,
        None,
    )
}

fn execute(home: &HomeStore, contribution: MutationContribution) {
    let mut command = HomeCommand::new(home.home_revision().unwrap());
    command.add(contribution).unwrap();
    assert!(matches!(
        home.execute(command),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
}

fn open(count: usize, faults: FaultController) -> (tempfile::TempDir, HomeStore, SessionState) {
    let directory = tempfile::tempdir().unwrap();
    let mut candidate = HomeOpenCandidate::open_with_faults(
        HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
        faults,
    )
    .unwrap();
    let state = BerylState::register(&mut candidate).unwrap();
    let home = candidate
        .prepare_publication(BerylState::required_domains().unwrap())
        .unwrap()
        .publish()
        .unwrap();
    let session = state.session();
    let first = WindowId::from_bytes(0u128.to_be_bytes());
    execute(
        &home,
        session.initialize_threadless(
            session.revision(&home).unwrap(),
            InitializeThreadlessWindow::new(first, placement(0)),
        ),
    );
    let target = RememberedTarget::new(RuntimeId::from_bytes([1; 16]), RootId::from_bytes([2; 16]));
    if count > 1 {
        let before = session.minimal_bootstrap(&home).unwrap().unwrap();
        execute(
            &home,
            session.replace_claim(
                session.revision(&home).unwrap(),
                ReplaceWindowClaim::new(
                    before.header().revision(),
                    first,
                    before.windows()[0].revision(),
                    None,
                    target,
                    SyndicThreadId::from_bytes(0u128.to_be_bytes()),
                ),
            ),
        );
    }
    for ordinal in 1..count {
        let before = session.minimal_bootstrap(&home).unwrap().unwrap();
        execute(
            &home,
            session.create_claimed_window(
                session.revision(&home).unwrap(),
                CreateClaimedWindow::new(
                    before.header().revision(),
                    WindowId::from_bytes((ordinal as u128).to_be_bytes()),
                    target,
                    SyndicThreadId::from_bytes((ordinal as u128).to_be_bytes()),
                    placement(ordinal as i32),
                ),
            ),
        );
    }
    (directory, home, session)
}

fn recover(home: HomeStore) -> (HomeRecoveryCandidate, SessionState) {
    if home.health().state() == beryl_home_store::HomeHealthState::Healthy {
        home.inject_retained_maintenance_terminal();
        assert!(BerylState::reacquire(&home).is_err());
    }
    let candidate = home.recover_same_home().unwrap();
    let state = BerylState::reacquire_candidate(&candidate).unwrap();
    (candidate, state.session())
}

fn snapshot(
    candidate: &mut HomeRecoveryCandidate,
    session: &SessionState,
) -> MinimalSessionBootstrap {
    session
        .minimal_bootstrap_candidate(&candidate.recovery_access().unwrap())
        .unwrap()
        .unwrap()
}

fn revision(candidate: &mut HomeRecoveryCandidate) -> u64 {
    candidate
        .recovery_access()
        .unwrap()
        .home_revision()
        .unwrap()
        .get()
}

fn assert_restored(
    before: &MinimalSessionBootstrap,
    after: &MinimalSessionBootstrap,
    evidence: &SessionWindowRemovalEvidence,
) {
    assert_eq!(
        after.header().revision().get(),
        before.header().revision().get() + 2
    );
    assert_eq!(after.header().fallback(), before.header().fallback());
    assert_eq!(after.header().exit_intent(), before.header().exit_intent());
    assert_eq!(after.windows().len(), before.windows().len());
    for (old, new) in before.windows().iter().zip(after.windows()) {
        assert_eq!(new.window_id(), old.window_id());
        if old.window_id() != evidence.window().window_id() {
            assert_eq!(new, old);
            continue;
        }
        assert_eq!(new.revision().get(), old.revision().get() + 1);
        assert_eq!(new.placement(), old.placement());
        assert_eq!(new.remembered_target(), old.remembered_target());
        match (old.selected_thread(), new.selected_thread()) {
            (Some(old), Some(new)) => {
                assert_eq!(new.thread_id(), old.thread_id());
                assert_eq!(new.generation(), old.generation());
                assert_eq!(new.revision().get(), old.revision().get() + 1);
            }
            (None, None) => {}
            pair => panic!("changed selection: {pair:?}"),
        }
    }
}

#[test]
fn healthy_native_failure_restores_exact_claim_once_without_replacing_generation() {
    let (_directory, home, session) = open(3, FaultController::new());
    let before = session.minimal_bootstrap(&home).unwrap().unwrap();
    let generation = home.generation_identity().unwrap();
    let mut close =
        OrdinaryCloseSession::execute(&home, &session, before.windows()[0].window_id()).unwrap();
    let original = format!("{:?}", close.original);
    let restored = close.restore_healthy(&home, &session).unwrap();
    assert_eq!(home.generation_identity().unwrap(), generation);
    assert_eq!(format!("{:?}", close.original), original);
    assert_restored(
        &before,
        &session.minimal_bootstrap(&home).unwrap().unwrap(),
        close.removal_evidence(),
    );
    let revision = home.home_revision().unwrap();
    assert_eq!(close.restore_healthy(&home, &session).unwrap(), restored);
    assert_eq!(home.home_revision().unwrap(), revision);
    drop(session);
    home.close().unwrap();
}

#[test]
fn healthy_native_failure_noncommit_validates_original_without_inverse_write() {
    let (_directory, home, session) = open(3, FaultController::new());
    let before = session.minimal_bootstrap(&home).unwrap().unwrap();
    let settings = beryl_state::BerylState::reacquire(&home)
        .unwrap()
        .settings();
    let mut close = OrdinaryCloseSession::execute_with_admission_hook(
        &home,
        &session,
        before.windows()[0].window_id(),
        |home| {
            execute(
                home,
                settings.apply(
                    settings.revision(home).unwrap(),
                    beryl_state::ApplySettings::new(vec![beryl_state::SettingUpdate::new(
                        beryl_state::SettingKey::DeveloperInstructions,
                        beryl_state::ExpectedSettingRevision::Absent,
                        beryl_state::SettingValue::developer_instructions(
                            "native original noncommit",
                        )
                        .unwrap(),
                    )])
                    .unwrap(),
                ),
            );
        },
    )
    .unwrap();
    assert_eq!(close.original.known_commit(), Some(false));
    let revision = home.home_revision().unwrap();
    assert_eq!(
        close.restore_healthy(&home, &session).unwrap(),
        before.windows()[0]
    );
    assert_eq!(home.home_revision().unwrap(), revision);
    assert!(close.restoration.is_none());
    drop((session, settings));
    home.close().unwrap();
}

#[test]
fn healthy_native_restoration_failure_transfers_both_outcomes_to_candidate_without_replay() {
    for fault in [
        FaultPoint::BeforeCommit,
        FaultPoint::AfterPersist,
        FaultPoint::AfterCommitBeforePersist,
    ] {
        let faults = FaultController::new();
        let (_directory, home, session) = open(3, faults.clone());
        let before = session.minimal_bootstrap(&home).unwrap().unwrap();
        let mut close =
            OrdinaryCloseSession::execute(&home, &session, before.windows()[0].window_id())
                .unwrap();
        let original = format!("{:?}", close.original);
        faults.fail_next(fault);
        let result = close.restore_healthy(&home, &session);
        if result.is_ok() {
            assert_eq!(
                close
                    .restoration
                    .as_ref()
                    .and_then(ResumeSessionOutcome::known_commit),
                Some(true),
                "{fault:?}"
            );
            assert_eq!(
                home.health().state(),
                beryl_home_store::HomeHealthState::Healthy,
                "{fault:?}"
            );
        }
        assert_eq!(format!("{:?}", close.original), original);
        assert!(close.restoration.is_some());
        let (mut candidate, fresh) = recover(home);
        close.converge_candidate(&mut candidate, &fresh).unwrap();
        let restored = snapshot(&mut candidate, &fresh);
        assert_restored(&before, &restored, close.restored_evidence().unwrap());
        let before_repeat = revision(&mut candidate);
        close.converge_candidate(&mut candidate, &fresh).unwrap();
        assert_eq!(revision(&mut candidate), before_repeat);
        candidate.abort().close().unwrap();
    }
}

#[test]
fn original_noncommit_validates_unchanged_membership_without_inverse_write() {
    for count in [1, 3] {
        let faults = FaultController::new();
        let (_directory, home, old) = open(count, faults.clone());
        let before = old.minimal_bootstrap(&home).unwrap().unwrap();
        faults.fail_next(FaultPoint::BeforeCommit);
        let mut close =
            OrdinaryCloseSession::execute(&home, &old, before.windows()[0].window_id()).unwrap();
        assert!(matches!(
            close.original,
            ResumeSessionOutcome::NotCommitted { .. }
        ));
        assert!(close.require_ready(&home, &old).is_err());
        let (mut candidate, fresh) = recover(home);
        let start = revision(&mut candidate);
        for _ in 0..2 {
            close.converge_candidate(&mut candidate, &fresh).unwrap();
            close.revalidate_candidate(&mut candidate, &fresh).unwrap();
            assert_eq!(snapshot(&mut candidate, &fresh), before);
            assert_eq!(revision(&mut candidate), start);
            assert!(close.restoration.is_none());
            assert!(close.restored_evidence().is_none());
        }
        candidate.abort().close().unwrap();
    }
}

#[test]
fn committed_and_exact_new_removals_restore_once_with_renewed_claims() {
    for count in [1, 3] {
        for fault in [
            FaultPoint::AfterPersist,
            FaultPoint::AfterCommitBeforePersist,
        ] {
            let faults = FaultController::new();
            let (_directory, home, old) = open(count, faults.clone());
            let before = old.minimal_bootstrap(&home).unwrap().unwrap();
            faults.fail_next(fault);
            let mut close =
                OrdinaryCloseSession::execute(&home, &old, before.windows()[0].window_id())
                    .unwrap();
            assert!(close.require_ready(&home, &old).is_err());
            let (mut candidate, fresh) = recover(home);
            assert!(close.revalidate_candidate(&mut candidate, &fresh).is_err());
            close.converge_candidate(&mut candidate, &fresh).unwrap();
            assert_eq!(close.original.known_commit(), Some(true));
            if fault == FaultPoint::AfterCommitBeforePersist {
                assert!(matches!(
                    close.original,
                    ResumeSessionOutcome::Indeterminate {
                        reconciliation: Some(Ok(ReconciliationResolution::ExactNew { .. })),
                        ..
                    }
                ));
            } else {
                assert!(matches!(
                    close.original,
                    ResumeSessionOutcome::Committed {
                        later_failure: Some(_),
                        local_finalization: Some(_),
                        ..
                    }
                ));
            }
            assert_restored(
                &before,
                &snapshot(&mut candidate, &fresh),
                close.restored_evidence().unwrap(),
            );
            let claim = fresh
                .window_claim_catalog_source_candidate(
                    &candidate.recovery_access().unwrap(),
                    before.windows()[0].window_id(),
                )
                .unwrap()
                .claim();
            if let Some(claim) = claim {
                let original = close.evidence.claim().unwrap();
                assert_eq!(claim.revision().get(), original.revision().get() + 1);
                assert_eq!(claim.generation(), original.generation());
                assert_eq!(claim.state(), original.state());
                assert_eq!(claim.thread_id(), original.thread_id());
            } else {
                assert!(close.evidence.claim().is_none());
            }
            let settled = format!("{close:?}");
            let start = revision(&mut candidate);
            close.converge_candidate(&mut candidate, &fresh).unwrap();
            assert_eq!(revision(&mut candidate), start);
            assert_eq!(format!("{close:?}"), settled);
            candidate.abort().close().unwrap();
        }
    }
}

#[test]
fn restoration_noncommit_retains_original_and_retries_only_on_fresh_candidate() {
    let faults = FaultController::new();
    let (_directory, home, old) = open(3, faults.clone());
    let before = old.minimal_bootstrap(&home).unwrap().unwrap();
    faults.fail_next(FaultPoint::AfterPersist);
    let mut close =
        OrdinaryCloseSession::execute(&home, &old, before.windows()[0].window_id()).unwrap();
    let original = format!("{:?}", close.original);
    let (mut candidate, fresh) = recover(home);
    faults.fail_next(FaultPoint::BeforeCommit);
    assert!(close.converge_candidate(&mut candidate, &fresh).is_err());
    assert!(matches!(
        close.restoration,
        Some(ResumeSessionOutcome::NotCommitted { .. })
    ));
    assert!(close.restored_evidence().is_none());
    assert!(close.converge_candidate(&mut candidate, &fresh).is_err());
    let (mut candidate, newer) = recover(candidate.abort());
    let start = revision(&mut candidate);
    assert!(close.converge_candidate(&mut candidate, &fresh).is_err());
    assert_eq!(revision(&mut candidate), start);
    close.converge_candidate(&mut candidate, &newer).unwrap();
    assert!(matches!(
        close.previous_restoration,
        Some(ResumeSessionOutcome::NotCommitted { .. })
    ));
    assert_eq!(format!("{:?}", close.original), original);
    assert_restored(
        &before,
        &snapshot(&mut candidate, &newer),
        close.restored_evidence().unwrap(),
    );
    candidate.abort().close().unwrap();
}

#[test]
fn restoration_commit_with_later_failure_and_indeterminate_have_independent_custody() {
    for fault in [
        FaultPoint::AfterPersist,
        FaultPoint::AfterCommitBeforePersist,
    ] {
        let faults = FaultController::new();
        let (_directory, home, old) = open(3, faults.clone());
        let before = old.minimal_bootstrap(&home).unwrap().unwrap();
        faults.fail_next(FaultPoint::AfterPersist);
        let mut close =
            OrdinaryCloseSession::execute(&home, &old, before.windows()[0].window_id()).unwrap();
        let original = format!("{:?}", close.original);
        let (mut candidate, fresh) = recover(home);
        faults.fail_next(fault);
        let _ = close.converge_candidate(&mut candidate, &fresh);
        assert_eq!(format!("{:?}", close.original), original);
        if fault == FaultPoint::AfterPersist {
            assert!(matches!(
                close.restoration,
                Some(ResumeSessionOutcome::Committed {
                    later_failure: Some(_),
                    local_finalization: Some(_),
                    ..
                })
            ));
        } else {
            assert!(matches!(
                close.restoration,
                Some(ResumeSessionOutcome::Indeterminate { .. })
            ));
        }
        let (mut candidate, newer) = recover(candidate.abort());
        let start = revision(&mut candidate);
        close.converge_candidate(&mut candidate, &newer).unwrap();
        assert_eq!(revision(&mut candidate), start);
        assert!(close.previous_restoration.is_none());
        assert_restored(
            &before,
            &snapshot(&mut candidate, &newer),
            close.restored_evidence().unwrap(),
        );
        let settled = format!("{close:?}");
        close.converge_candidate(&mut candidate, &newer).unwrap();
        assert_eq!(format!("{close:?}"), settled);
        candidate.abort().close().unwrap();
    }
}

#[test]
fn failed_original_reconciliation_is_retained_and_retry_does_not_repeat_removal() {
    let faults = FaultController::new();
    let (_directory, home, old) = open(3, faults.clone());
    let before = old.minimal_bootstrap(&home).unwrap().unwrap();
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let mut close =
        OrdinaryCloseSession::execute(&home, &old, before.windows()[0].window_id()).unwrap();
    let (mut candidate, fresh) = recover(home);
    faults.fail_next(FaultPoint::BeforeReconciliationSnapshot);
    assert!(close.converge_candidate(&mut candidate, &fresh).is_err());
    assert!(matches!(
        close.original,
        ResumeSessionOutcome::Indeterminate {
            reconciliation: Some(Err(_)),
            ..
        }
    ));
    assert!(close.restoration.is_none());
    let (mut candidate, newer) = recover(candidate.abort());
    close.converge_candidate(&mut candidate, &newer).unwrap();
    assert_restored(
        &before,
        &snapshot(&mut candidate, &newer),
        close.restored_evidence().unwrap(),
    );
    candidate.abort().close().unwrap();
}

#[test]
fn failed_restoration_reconciliation_retains_both_outcomes_until_fresh_exact_new() {
    let faults = FaultController::new();
    let (_directory, home, old) = open(3, faults.clone());
    let before = old.minimal_bootstrap(&home).unwrap().unwrap();
    faults.fail_next(FaultPoint::AfterPersist);
    let mut close =
        OrdinaryCloseSession::execute(&home, &old, before.windows()[0].window_id()).unwrap();
    let original = format!("{:?}", close.original);
    let (mut candidate, fresh) = recover(home);
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    faults.fail_next(FaultPoint::BeforeReconciliationSnapshot);
    assert!(close.converge_candidate(&mut candidate, &fresh).is_err());
    assert!(matches!(
        close.restoration,
        Some(ResumeSessionOutcome::Indeterminate {
            reconciliation: Some(Err(_)),
            ..
        })
    ));
    assert_eq!(format!("{:?}", close.original), original);
    assert!(close.restored_evidence().is_none());
    let (mut candidate, newer) = recover(candidate.abort());
    let start = revision(&mut candidate);
    close.converge_candidate(&mut candidate, &newer).unwrap();
    assert!(matches!(
        close.restoration,
        Some(ResumeSessionOutcome::Indeterminate {
            reconciliation: Some(Ok(ReconciliationResolution::ExactNew { .. })),
            ..
        })
    ));
    assert_eq!(revision(&mut candidate), start);
    assert_eq!(format!("{:?}", close.original), original);
    assert!(close.previous_restoration.is_none());
    assert_restored(
        &before,
        &snapshot(&mut candidate, &newer),
        close.restored_evidence().unwrap(),
    );
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(close.revalidate_candidate(&mut candidate, &newer).is_err());
    let (mut candidate, newest) = recover(candidate.abort());
    let start = revision(&mut candidate);
    close.converge_candidate(&mut candidate, &newest).unwrap();
    assert_eq!(revision(&mut candidate), start);
    candidate.abort().close().unwrap();
}

#[test]
fn stale_foreign_and_changed_membership_candidates_refuse_without_writes() {
    let faults = FaultController::new();
    let (_directory, home, old) = open(3, faults.clone());
    let before = old.minimal_bootstrap(&home).unwrap().unwrap();
    faults.fail_next(FaultPoint::AfterPersist);
    let mut close =
        OrdinaryCloseSession::execute(&home, &old, before.windows()[0].window_id()).unwrap();
    let (mut candidate, fresh) = recover(home);
    let start = revision(&mut candidate);
    assert!(close.converge_candidate(&mut candidate, &old).is_err());
    assert_eq!(revision(&mut candidate), start);
    let (_foreign_directory, foreign, _) = open(3, FaultController::new());
    let (mut foreign, foreign_session) = recover(foreign);
    let foreign_revision = revision(&mut foreign);
    assert!(
        close
            .converge_candidate(&mut foreign, &foreign_session)
            .is_err()
    );
    assert_eq!(revision(&mut foreign), foreign_revision);
    assert!(close.restoration.is_none());
    foreign.abort().close().unwrap();
    let after = snapshot(&mut candidate, &fresh);
    let survivor = &after.windows()[0];
    let access = candidate.recovery_access().unwrap();
    let mut command = HomeCommand::new(access.home_revision().unwrap());
    command
        .add(fresh.update_placement(
            fresh.revision_candidate(&access).unwrap(),
            UpdateWindowPlacement::new(
                after.header().revision(),
                survivor.window_id(),
                survivor.revision(),
                placement(555),
            ),
        ))
        .unwrap();
    assert!(matches!(
        access.execute(command),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    let start = revision(&mut candidate);
    assert!(close.converge_candidate(&mut candidate, &fresh).is_err());
    assert_eq!(revision(&mut candidate), start);
    assert!(close.restoration.is_none());
    candidate.abort().close().unwrap();
}

#[test]
fn collision_and_successor_qualification_never_authorize_restoration() {
    for collision in [true, false] {
        let faults = FaultController::new();
        let (_directory, home, old) = open(3, faults.clone());
        let before = old.minimal_bootstrap(&home).unwrap().unwrap();
        faults.fail_next(FaultPoint::AfterCommitBeforePersist);
        let mut close =
            OrdinaryCloseSession::execute(&home, &old, before.windows()[0].window_id()).unwrap();
        let (mut candidate, fresh) = recover(home);
        OrdinaryCloseSession::reconcile(&mut close.original, &mut candidate).unwrap();
        let ResumeSessionOutcome::Indeterminate { reconciliation, .. } = &mut close.original else {
            panic!()
        };
        let Some(Ok(ReconciliationResolution::ExactNew { receipt })) = reconciliation.take() else {
            panic!()
        };
        *reconciliation = Some(Ok(if collision {
            ReconciliationResolution::Collision
        } else {
            ReconciliationResolution::ExactSuccessor { receipt }
        }));
        let start = revision(&mut candidate);
        assert!(close.converge_candidate(&mut candidate, &fresh).is_err());
        assert_eq!(close.original.known_commit(), None);
        assert_eq!(revision(&mut candidate), start);
        assert!(close.restoration.is_none());
        assert!(close.restored_evidence().is_none());
        candidate.abort().close().unwrap();
    }
}
