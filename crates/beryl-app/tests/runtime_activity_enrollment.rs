#![cfg(feature = "test-faults")]
#[path = "../../syndic-storage/tests/support/mod.rs"]
mod support;
#[path = "runtime_activity_enrollment/runtime.rs"]
mod runtime;

use beryl_app::runtime_activity_enrollment::*;
use beryl_home_store::{
    CommandCancellation, CommandOutcome, HomeOpenCandidate, HomeOpenOptions, HomeSchemaVersion,
    HomeStore,
    test_faults::{FaultController, FaultPoint},
};
use beryl_model::{ExecutionBinding, RuntimeId, SyndicItemId, SyndicTurnId};
use std::num::NonZeroUsize;
use support::{TestHome, batch, commit, draft_id, id, timestamp};
use syndic_storage::{test_faults::FixtureRecord, *};

struct Fixture {
    home: HomeStore,
    syndic: SyndicStorage,
    faults: FaultController,
    _directory: TestHome,
    turn: SyndicTurnId,
    operations: RuntimeActivityEnrollmentOperations,
}

impl Fixture {
    fn new() -> Self {
        let directory = TestHome::new("runtime-activity-enrollment");
        let faults = FaultController::new();
        let mut candidate = HomeOpenCandidate::open_with_faults(
            HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
            faults.clone(),
        )
        .unwrap();
        let syndic = SyndicStorage::register(&mut candidate).unwrap();
        let home = candidate
            .prepare_publication(SyndicStorage::required_domains().unwrap())
            .unwrap()
            .publish()
            .unwrap();
        support::seed_populated(&home, syndic.clone());
        support::converge_and_release_terminal_history(
            &home,
            syndic.clone(),
            id(30),
            support::populated::source_turn(),
        );
        let head = syndic
            .activity_query_head(&home, id(30), limit())
            .unwrap()
            .unwrap();
        let turn = support::exact_cas::submit_current_draft(
            &home,
            syndic.clone(),
            id(30),
            draft_id(220),
            SyndicItemId::from_bytes([221; 16]),
            "next",
            timestamp(100),
        );
        commit(
            &home,
            syndic.clone(),
            batch([FixtureRecord::ActivityQueryHead(head)]),
        );
        let operations =
            RuntimeActivityEnrollmentOperations::new(home.home_id(), NonZeroUsize::new(1).unwrap());
        Self {
            home,
            syndic,
            faults,
            _directory: directory,
            turn,
            operations,
        }
    }

    fn prepare(&self) -> PreparedActivityEnrollment {
        let head = self
            .syndic
            .activity_query_head(&self.home, id(30), limit())
            .unwrap()
            .unwrap();
        let execution = self
            .syndic
            .thread_execution(&self.home, id(30), limit())
            .unwrap()
            .unwrap();
        match self
            .syndic
            .prepare_activity_enrollment(
                &self.home,
                ActivityEnrollmentRequest::first(
                    ActivityQuerySource::new(id(30), self.turn),
                    execution.execution().clone(),
                    head.revision(),
                ),
            )
            .unwrap()
        {
            ActivityEnrollmentPreparation::Prepared(prepared) => prepared,
            _ => panic!("new enrollment"),
        }
    }

    fn uncertain(&self) {
        let reservation = self.operations.reserve(&self.home, self.prepare()).unwrap();
        self.faults.fail_next(FaultPoint::AfterCommitBeforePersist);
        assert!(matches!(
            reservation.execute(),
            ActivityEnrollmentCommandOutcome::Pending { .. }
        ));
        assert_eq!(self.operations.pending_count(), 1);
        assert_eq!(self.home.pending_reconciliations().len(), 1);
        self.faults.fail_next(FaultPoint::BeforeReadConfirmation);
        assert!(self.home.home_revision().is_err());
    }
}

fn limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(65_536).unwrap()
}

#[test]
fn reservation_excludes_duplicates_and_abandonment_releases_capacity() {
    let f = Fixture::new();
    let reservation = f.operations.reserve(&f.home, f.prepare()).unwrap();
    assert!(matches!(
        f.operations.reserve(&f.home, f.prepare()),
        Err(ActivityEnrollmentCustodyError::Occupied)
    ));
    let before = f.home.home_revision().unwrap();
    drop(reservation);
    assert_eq!(f.operations.pending_count(), 0);
    assert_eq!(f.home.home_revision().unwrap(), before);
    drop(f.operations.reserve(&f.home, f.prepare()).unwrap());
    assert_eq!(f.operations.pending_count(), 0);
}

#[test]
fn runtime_slots_share_one_capacity_and_reject_foreign_home() {
    let f = Fixture::new();
    let other = Fixture::new();
    assert!(matches!(
        f.operations.reserve(&other.home, other.prepare()),
        Err(ActivityEnrollmentCustodyError::Identity)
    ));
    assert!(matches!(
        f.operations.reserve(&f.home, other.prepare()),
        Err(ActivityEnrollmentCustodyError::Identity)
    ));
    let reservation = f.operations.reserve(&f.home, f.prepare()).unwrap();
    let prior = support::exact_cas::execution_binding();
    let execution = ExecutionBinding::new(
        RuntimeId::from_bytes([211; 16]),
        prior.root_id(),
        prior.root_path().clone(),
    );
    commit(
        &f.home,
        f.syndic.clone(),
        batch([FixtureRecord::ThreadExecution(ThreadExecutionRecord::new(
            id(30),
            execution,
        ))]),
    );
    assert!(matches!(
        f.operations.reserve(&f.home, f.prepare()),
        Err(ActivityEnrollmentCustodyError::Capacity)
    ));
    drop(reservation);
    assert_eq!(f.operations.pending_count(), 0);
}

#[test]
fn definitive_commit_and_noncommit_release_the_slot() {
    for fail in [false, true] {
        let f = Fixture::new();
        let reservation = f.operations.reserve(&f.home, f.prepare()).unwrap();
        if fail {
            f.faults.fail_next(FaultPoint::BeforeCommit);
        }
        let ActivityEnrollmentCommandOutcome::Definitive { outcome, .. } = reservation.execute()
        else {
            panic!("definitive outcome")
        };
        assert!(matches!(
            (fail, outcome),
            (false, CommandOutcome::Committed { .. }) | (true, CommandOutcome::NotCommitted { .. })
        ));
        assert_eq!(f.operations.pending_count(), 0);
    }
}

#[test]
fn dropped_service_attachment_keeps_uncertainty_until_fresh_candidate_settlement() {
    let f = Fixture::new();
    let attachment = f.operations.clone();
    f.uncertain();
    drop(attachment);
    assert_eq!(f.operations.pending_count(), 1);
    let mut candidate = f.home.recover_same_home().unwrap();
    let fresh = SyndicStorage::reacquire_candidate(&candidate).unwrap();
    let access = candidate.recovery_access().unwrap();
    assert_eq!(
        f.operations
            .settle_retired_candidate(&access, &fresh, &CommandCancellation::new())
            .unwrap(),
        1
    );
    assert_eq!(f.operations.pending_count(), 0);
    assert!(access.pending_reconciliations().is_empty());
    candidate.publish().unwrap().close().unwrap();
}

#[test]
fn cancellation_and_failed_candidate_keep_the_original_custody() {
    let f = Fixture::new();
    f.uncertain();
    let mut candidate = f.home.recover_same_home().unwrap();
    let fresh = SyndicStorage::reacquire_candidate(&candidate).unwrap();
    {
        let access = candidate.recovery_access().unwrap();
        let cancelled = CommandCancellation::new();
        cancelled.cancel();
        assert!(matches!(
            f.operations
                .settle_retired_candidate(&access, &fresh, &cancelled),
            Err(ActivityEnrollmentCustodyError::Cancelled)
        ));
        assert_eq!(f.operations.pending_count(), 1);
        f.faults.fail_next(FaultPoint::BeforeReconciliationSnapshot);
        assert!(
            f.operations
                .settle_retired_candidate(&access, &fresh, &CommandCancellation::new())
                .is_err()
        );
        assert_eq!(f.operations.pending_count(), 1);
    }
    let home = candidate.abort();
    let mut retry = home.recover_same_home().unwrap();
    let fresh = SyndicStorage::reacquire_candidate(&retry).unwrap();
    let access = retry.recovery_access().unwrap();
    assert_eq!(
        f.operations
            .settle_retired_candidate(&access, &fresh, &CommandCancellation::new())
            .unwrap(),
        1
    );
    assert_eq!(f.operations.pending_count(), 0);
    retry.publish().unwrap().close().unwrap();
}

#[test]
fn natural_read_failure_does_not_release_registry_settled_custody() {
    let f = Fixture::new();
    f.uncertain();
    let mut candidate = f.home.recover_same_home().unwrap();
    let fresh = SyndicStorage::reacquire_candidate(&candidate).unwrap();
    let access = candidate.recovery_access().unwrap();
    assert!(
        f.operations
            .settle_retired_candidate(&access, &f.syndic, &CommandCancellation::new())
            .is_err()
    );
    assert_eq!(f.operations.pending_count(), 1);
    assert_eq!(
        f.operations
            .settle_retired_candidate(&access, &fresh, &CommandCancellation::new())
            .unwrap(),
        1
    );
    assert_eq!(f.operations.pending_count(), 0);
    candidate.publish().unwrap().close().unwrap();
}

#[test]
fn physical_write_uncertainty_reconciles_exact_old_without_an_enrollment() {
    let f = Fixture::new();
    let reservation = f.operations.reserve(&f.home, f.prepare()).unwrap();
    let fault = beryl_home_store::test_faults::fail_next_journal_write();
    let outcome = reservation.execute();
    drop(fault);
    assert!(matches!(
        outcome,
        ActivityEnrollmentCommandOutcome::Pending { .. }
    ));
    assert_eq!(f.operations.pending_count(), 1);
    f.faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(f.home.home_revision().is_err());
    let mut candidate = f.home.recover_same_home().unwrap();
    let fresh = SyndicStorage::reacquire_candidate(&candidate).unwrap();
    let access = candidate.recovery_access().unwrap();
    let pending = access.pending_reconciliations();
    assert_eq!(pending.len(), 1);
    assert!(matches!(
        access.reconcile(&pending[0]).unwrap(),
        beryl_home_store::ReconciliationResolution::ExactOld
    ));
    assert_eq!(
        f.operations
            .settle_retired_candidate(&access, &fresh, &CommandCancellation::new())
            .unwrap(),
        1
    );
    assert_eq!(f.operations.pending_count(), 0);
    candidate.publish().unwrap().close().unwrap();
}

#[test]
fn foreign_candidate_cannot_settle_another_homes_pending_slot() {
    let f = Fixture::new();
    let other = Fixture::new();
    f.uncertain();
    other.uncertain();
    let mut candidate = other.home.recover_same_home().unwrap();
    let fresh = SyndicStorage::reacquire_candidate(&candidate).unwrap();
    let access = candidate.recovery_access().unwrap();
    assert!(matches!(
        f.operations
            .settle_retired_candidate(&access, &fresh, &CommandCancellation::new()),
        Err(ActivityEnrollmentCustodyError::Identity)
    ));
    assert_eq!(f.operations.pending_count(), 1);
    assert_eq!(
        other
            .operations
            .settle_retired_candidate(&access, &fresh, &CommandCancellation::new())
            .unwrap(),
        1
    );
    candidate.publish().unwrap().close().unwrap();
    let mut candidate = f.home.recover_same_home().unwrap();
    let fresh = SyndicStorage::reacquire_candidate(&candidate).unwrap();
    let access = candidate.recovery_access().unwrap();
    assert_eq!(
        f.operations
            .settle_retired_candidate(&access, &fresh, &CommandCancellation::new())
            .unwrap(),
        1
    );
    candidate.publish().unwrap().close().unwrap();
}

#[test]
fn settled_registry_cannot_release_a_conflicting_natural_witness() {
    let f = Fixture::new();
    let old = f
        .syndic
        .activity_query_head(&f.home, id(30), limit())
        .unwrap()
        .unwrap();
    let reservation = f.operations.reserve(&f.home, f.prepare()).unwrap();
    f.faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    assert!(matches!(
        reservation.execute(),
        ActivityEnrollmentCommandOutcome::Pending { .. }
    ));
    let new = f
        .syndic
        .activity_query_head(&f.home, id(30), limit())
        .unwrap()
        .unwrap();
    f.faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(f.home.home_revision().is_err());
    let mut candidate = f.home.recover_same_home().unwrap();
    let fresh = SyndicStorage::reacquire_candidate(&candidate).unwrap();
    let access = candidate.recovery_access().unwrap();
    assert!(matches!(
        access
            .reconcile(&access.pending_reconciliations()[0])
            .unwrap(),
        beryl_home_store::ReconciliationResolution::ExactNew { .. }
    ));
    for (head, conflicting) in [(old, true), (new, false)] {
        let mut command = beryl_home_store::HomeCommand::new(access.home_revision().unwrap());
        command
            .add(fresh.clone().fixture_contribution(
                fresh.revision_candidate(&access).unwrap(),
                batch([FixtureRecord::ActivityQueryHead(head)]),
            ))
            .unwrap();
        assert!(matches!(
            access.execute(command),
            CommandOutcome::Committed { .. }
        ));
        let result =
            f.operations
                .settle_retired_candidate(&access, &fresh, &CommandCancellation::new());
        if conflicting {
            assert!(matches!(
                result,
                Err(ActivityEnrollmentCustodyError::Conflict)
            ));
            assert_eq!(f.operations.pending_count(), 1);
        } else {
            assert_eq!(result.unwrap(), 1);
            assert_eq!(f.operations.pending_count(), 0);
        }
    }
    candidate.publish().unwrap().close().unwrap();
}
