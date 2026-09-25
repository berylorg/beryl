use super::*;
use beryl_home_store::{ReconciliationResolution, test_faults::FaultPoint};

fn pending(work: MainWindowRestoreSet) -> MainWindowRestoreSet {
    match work.advance() {
        MainWindowRestoreSetOutcome::Pending(work) => work,
        MainWindowRestoreSetOutcome::Prepared(prepared) => {
            dispose(prepared.dispose());
            panic!("unsettled session command exposed prepared members")
        }
        MainWindowRestoreSetOutcome::Failed { error } => {
            panic!("session command unexpectedly failed: {error}")
        }
        MainWindowRestoreSetOutcome::Retained { .. } => {
            panic!("session command unexpectedly retained local-finalization custody")
        }
    }
}

fn arm_uncertain(work: &mut MainWindowRestoreSet, fixture: &Fixture) {
    let faults = fixture.faults.clone();
    work.test_arm_before_session_command(move || {
        faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    });
}

#[test]
fn uncertain_begin_restore_retries_original_reconciliation_without_repeating_command() {
    let fixture = Fixture::new(61);
    drop(fixture.acquire(62));
    let before = snapshot(&fixture);
    let (mut work, _lifetime) = work(&fixture);
    arm_uncertain(&mut work, &fixture);
    let work = pending(work);
    let handles = fixture.store.pending_reconciliations();
    assert_eq!(handles.len(), 1);
    let original = handles.into_iter().next().unwrap();
    let uncertain = snapshot(&fixture);
    assert_eq!(
        uncertain.header().revision(),
        before.header().revision().checked_next().unwrap()
    );
    fixture
        .faults
        .fail_next(FaultPoint::BeforeReconciliationSnapshot);
    let work = pending(work);
    assert_eq!(fixture.store.pending_reconciliations().len(), 1);
    assert!(fixture.store.reconcile(&original).is_err());
    assert_eq!(snapshot(&fixture), uncertain);
    assert_eq!(fixture.process.main_window_occupancy(), 0);
    let prepared = prepare(work);
    assert!(fixture.store.pending_reconciliations().is_empty());
    assert!(matches!(
        fixture.store.reconcile(&original).unwrap(),
        ReconciliationResolution::ExactNew { .. }
    ));
    assert_eq!(
        snapshot(&fixture).header().revision(),
        uncertain.header().revision().checked_next().unwrap()
    );
    assert_eq!(prepared.members().len(), 1);
    let active = snapshot(&fixture);
    dispose(prepared.dispose());
    assert_eq!(snapshot(&fixture), active);
    cleanup(fixture);
}

#[test]
fn empty_header_initialization_keeps_its_original_uncertain_command_after_begin_restore() {
    let fixture = zero_runtime(Some(false));
    let before = snapshot(&fixture);
    let (work, _lifetime) = work(&fixture);
    let mut work = pending(work);
    let after_begin = snapshot(&fixture);
    assert!(after_begin.windows().is_empty());
    assert_eq!(
        after_begin.header().revision(),
        before.header().revision().checked_next().unwrap()
    );
    arm_uncertain(&mut work, &fixture);
    let work = pending(work);
    let handles = fixture.store.pending_reconciliations();
    assert_eq!(handles.len(), 1);
    let original = handles.into_iter().next().unwrap();
    let initialized = snapshot(&fixture);
    assert_eq!(initialized.windows().len(), 1);
    assert_eq!(
        initialized.windows()[0].window_id(),
        WindowId::from_bytes([240; 16])
    );
    fixture
        .faults
        .fail_next(FaultPoint::BeforeReconciliationSnapshot);
    let work = pending(work);
    assert!(fixture.store.reconcile(&original).is_err());
    assert_eq!(snapshot(&fixture), initialized);
    let prepared = prepare(work);
    assert!(matches!(
        prepared.members()[0],
        PreparedRestoreSetMember::Threadless(_)
    ));
    assert_eq!(snapshot(&fixture), initialized);
    assert_eq!(
        initialized.header().revision(),
        after_begin.header().revision().checked_next().unwrap()
    );
    assert!(fixture.store.pending_reconciliations().is_empty());
    assert!(matches!(
        fixture.store.reconcile(&original).unwrap(),
        ReconciliationResolution::ExactNew { .. }
    ));
    dispose(prepared.dispose());
    assert_eq!(snapshot(&fixture), initialized);
    cleanup(fixture);
}

#[test]
fn cancellation_keeps_uncertain_session_custody_until_original_scope_settles() {
    for initialize in [false, true] {
        let fixture = if initialize {
            zero_runtime(None)
        } else {
            let fixture = Fixture::new(71);
            drop(fixture.acquire(72));
            fixture
        };
        let (mut work, _lifetime) = work(&fixture);
        arm_uncertain(&mut work, &fixture);
        let work = pending(work);
        let handles = fixture.store.pending_reconciliations();
        assert_eq!(handles.len(), 1);
        let original = handles.into_iter().next().unwrap();
        let durable = snapshot(&fixture);
        work.cancellation().cancel();
        fixture
            .faults
            .fail_next(FaultPoint::BeforeReconciliationSnapshot);
        let work = pending(work);
        assert_eq!(fixture.store.pending_reconciliations().len(), 1);
        assert!(fixture.store.reconcile(&original).is_err());
        assert_eq!(snapshot(&fixture), durable);
        assert_eq!(fixture.process.main_window_occupancy(), 0);
        assert!(dispose(work).contains("cancelled"));
        assert!(fixture.store.pending_reconciliations().is_empty());
        assert!(matches!(
            fixture.store.reconcile(&original).unwrap(),
            ReconciliationResolution::ExactNew { .. }
        ));
        assert_eq!(snapshot(&fixture), durable);
        cleanup(fixture);
    }
}

#[test]
fn concurrent_home_write_refuses_threadless_initialization_without_replacing_the_session() {
    let fixture = zero_runtime(Some(false));
    let (work, _lifetime) = work(&fixture);
    let mut work = pending(work);
    let before = snapshot(&fixture);
    let store = fixture.store.clone();
    let session = fixture.state.session();
    let expected = before.header().revision();
    work.test_arm_before_session_command(move || {
        execute(
            &store,
            session.begin_restore(
                session.revision(&store).unwrap(),
                beryl_state::BeginSessionRestore::new(expected),
            ),
        );
    });
    let work = pending(work);
    let concurrent = snapshot(&fixture);
    assert_eq!(
        concurrent.header().revision(),
        expected.checked_next().unwrap()
    );
    assert!(concurrent.windows().is_empty());
    let revision = fixture.store.home_revision().unwrap();
    assert!(dispose(work).contains("did not commit"));
    assert_eq!(snapshot(&fixture), concurrent);
    assert_eq!(fixture.store.home_revision().unwrap(), revision);
    assert!(fixture.store.pending_reconciliations().is_empty());
    assert_eq!(fixture.process.main_window_occupancy(), 0);
    cleanup(fixture);
}

#[test]
fn retired_service_before_discovery_releases_empty_custody_without_writing() {
    let fixture = zero_runtime(Some(true));
    let before = snapshot(&fixture);
    let revision = fixture.store.home_revision().unwrap();
    let (work, lifetime) = work(&fixture);
    drop(lifetime);
    assert!(dispose(work).contains("retired"));
    assert_eq!(snapshot(&fixture), before);
    assert_eq!(fixture.store.home_revision().unwrap(), revision);
    assert!(fixture.store.pending_reconciliations().is_empty());
    assert_eq!(fixture.process.main_window_occupancy(), 0);
    cleanup(fixture);
}

#[test]
fn foreign_restore_attempt_is_rejected_before_any_session_command() {
    let fixture = zero_runtime(Some(true));
    let foreign = zero_runtime(Some(true));
    let before = snapshot(&fixture);
    let foreign_before = snapshot(&foreign);
    let (services, appearance) = creation_support::services(&fixture);
    let (attempt, lifetime) = attempt(&foreign);
    assert!(
        MainWindowRestoreSet::new(
            services,
            attempt,
            activation_source(),
            appearance,
            WindowId::from_bytes([240; 16]),
            placement(),
        )
        .is_err()
    );
    assert_eq!(snapshot(&fixture), before);
    assert_eq!(snapshot(&foreign), foreign_before);
    assert!(fixture.store.pending_reconciliations().is_empty());
    assert!(foreign.store.pending_reconciliations().is_empty());
    drop(lifetime);
    cleanup(fixture);
    cleanup(foreign);
}

#[test]
fn uncertain_first_claim_settles_original_custody_before_preparing_both_saved_members() {
    let fixture = Fixture::new(81);
    let first = fixture.acquire(82);
    let second = fixture.acquire(83);
    let expected = [
        (first.window_id(), first.thread_id()),
        (second.window_id(), second.thread_id()),
    ];
    drop((first, second));
    let (mut work, _lifetime) = work(&fixture);
    let faults = fixture.faults.clone();
    work.test_arm_before_claim_activation(move || {
        faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    });
    let work = pending(work);
    assert!(fixture.store.pending_reconciliations().is_empty());
    let work = pending(work);
    let handles = fixture.store.pending_reconciliations();
    assert_eq!(handles.len(), 1);
    let original = handles.into_iter().next().unwrap();
    let uncertain = snapshot(&fixture);
    for (index, state) in [
        beryl_state::ThreadClaimState::Active,
        beryl_state::ThreadClaimState::Restoring,
    ]
    .into_iter()
    .enumerate()
    {
        let claim = fixture
            .state
            .session()
            .window_claim_catalog_source(&fixture.store, expected[index].0)
            .unwrap()
            .claim()
            .unwrap();
        assert_eq!(claim.state(), state);
        assert_eq!(claim.thread_id(), expected[index].1);
    }
    let prepared = prepare(work);
    assert!(fixture.store.pending_reconciliations().is_empty());
    assert!(matches!(
        fixture.store.reconcile(&original).unwrap(),
        ReconciliationResolution::ExactNew { .. }
    ));
    assert_eq!(prepared.members().len(), 2);
    let active = snapshot(&fixture);
    assert_eq!(
        active.header().revision(),
        uncertain.header().revision().checked_next().unwrap()
    );
    for (member, (window, thread)) in prepared.members().iter().zip(expected) {
        assert!(matches!(member, PreparedRestoreSetMember::Restored(_)));
        assert_eq!(member.window_id(), window);
        let claim = fixture
            .state
            .session()
            .window_claim_catalog_source(&fixture.store, window)
            .unwrap()
            .claim()
            .unwrap();
        assert_eq!(claim.state(), beryl_state::ThreadClaimState::Active);
        assert_eq!(claim.thread_id(), thread);
        assert_eq!(fixture.claim(window).revision(), claim.revision());
    }
    prepared.revalidate().unwrap();
    dispose(prepared.dispose());
    assert_eq!(snapshot(&fixture), active);
    assert_eq!(fixture.process.main_window_occupancy(), 0);
    cleanup(fixture);
}

#[test]
fn committed_local_finalization_remains_typed_original_custody_across_retry_and_cancellation() {
    for command in 0..3 {
        let fixture = if command == 1 {
            zero_runtime(None)
        } else {
            let fixture = Fixture::new(91);
            drop(fixture.acquire(92));
            fixture
        };
        let expected = match command {
            0 => RestoreSetRetainedReason::BeginRestoreLocalFinalization,
            1 => RestoreSetRetainedReason::ThreadlessInitializationLocalFinalization,
            2 => RestoreSetRetainedReason::RestoredClaimLocalFinalization {
                window_id: WindowId::from_bytes([92; 16]),
            },
            _ => unreachable!(),
        };
        let (mut work, lifetime) = work(&fixture);
        let faults = fixture.faults.clone();
        let fail_after_commit = move || {
            faults.fail_next_with_kind(FaultPoint::AfterPersist, std::io::ErrorKind::StorageFull);
        };
        if command == 2 {
            work.test_arm_before_claim_activation(fail_after_commit);
            work = pending(work);
        } else {
            work.test_arm_before_session_command(fail_after_commit);
        }
        let mut custody = retained(work, expected);
        assert_eq!(
            fixture.store.health().state(),
            beryl_home_store::HomeHealthState::Failed
        );
        assert!(fixture.store.pending_reconciliations().is_empty());
        let original = {
            let retained = custody.retained_command().unwrap();
            assert_eq!(retained.reason, expected);
            assert!(matches!(
                retained.failure,
                Some(beryl_home_store::CommandError::Persistence { .. })
            ));
            let _: &beryl_home_store::CommittedLocalFinalization = retained.local_finalization;
            retained.receipt.clone()
        };
        for cancel in [false, false, true, true] {
            if cancel {
                custody.cancellation().cancel();
            }
            custody = retained(custody, expected);
            let retained = custody.retained_command().unwrap();
            assert_eq!(retained.reason, expected);
            assert_eq!(retained.receipt.home_revision(), original.home_revision());
            assert!(*retained.receipt == original);
            assert!(matches!(
                retained.failure,
                Some(beryl_home_store::CommandError::Persistence { .. })
            ));
            let _: &beryl_home_store::CommittedLocalFinalization = retained.local_finalization;
            assert!(fixture.store.pending_reconciliations().is_empty());
        }
        drop(custody);
        drop(lifetime);
        cleanup(fixture);
    }
}

fn retained(
    work: MainWindowRestoreSet,
    expected: RestoreSetRetainedReason,
) -> MainWindowRestoreSet {
    match work.advance() {
        MainWindowRestoreSetOutcome::Retained { custody, reason } => {
            assert_eq!(reason, expected);
            custody
        }
        MainWindowRestoreSetOutcome::Pending(_) => {
            panic!("local finalization lost its typed retained outcome")
        }
        MainWindowRestoreSetOutcome::Prepared(_) => {
            panic!("local finalization exposed a prepared set")
        }
        MainWindowRestoreSetOutcome::Failed { error } => {
            panic!("local finalization was falsely settled: {error}")
        }
    }
}
