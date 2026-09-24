#![cfg(feature = "test-faults")]
#[path = "activity_enrollment/corruption.rs"]
mod corruption;
#[path = "activity_enrollment/producer.rs"]
mod producer;
#[path = "activity_enrollment/recovery.rs"]
mod recovery;
#[path = "activity_enrollment/retention.rs"]
mod retention;
mod support;

use beryl_home_store::{
    CommandOutcome, HomeOpenCandidate, HomeOpenOptions, HomeSchemaVersion, HomeStore,
    test_faults::{FaultController, FaultPoint},
};
use beryl_model::{SyndicItemId, SyndicTurnId};
use support::{TestHome, batch, commit, draft_id, id, timestamp};
use syndic_storage::{test_faults::FixtureRecord, *};

struct Fixture {
    store: HomeStore,
    storage: SyndicStorage,
    faults: FaultController,
    _directory: TestHome,
    turn: SyndicTurnId,
}

impl Fixture {
    fn new() -> Self {
        let directory = TestHome::new("activity-enrollment");
        let faults = FaultController::new();
        let mut candidate = HomeOpenCandidate::open_with_faults(
            HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
            faults.clone(),
        )
        .unwrap();
        let storage = SyndicStorage::register(&mut candidate).unwrap();
        let store = candidate
            .prepare_publication(SyndicStorage::required_domains().unwrap())
            .unwrap()
            .publish()
            .unwrap();
        support::seed_populated(&store, storage.clone());
        support::converge_and_release_terminal_history(
            &store,
            storage.clone(),
            id(30),
            support::populated::source_turn(),
        );
        let prior = storage
            .activity_query_head(&store, id(30), limit())
            .unwrap()
            .unwrap();
        let turn = support::exact_cas::submit_current_draft(
            &store,
            storage.clone(),
            id(30),
            draft_id(220),
            SyndicItemId::from_bytes([221; 16]),
            "next",
            timestamp(100),
        );
        assert_eq!(
            storage
                .activity_query_head(&store, id(30), limit())
                .unwrap(),
            Some(prior)
        );
        Self {
            store,
            storage,
            faults,
            _directory: directory,
            turn,
        }
    }
    fn head(&self) -> ActivityQueryHeadRecord {
        self.storage
            .activity_query_head(&self.store, id(30), limit())
            .unwrap()
            .unwrap()
    }
    fn request(&self, token: Option<&ActivityPeriodToken>) -> ActivityEnrollmentRequest {
        let execution = self
            .storage
            .thread_execution(&self.store, id(30), limit())
            .unwrap()
            .unwrap()
            .execution()
            .clone();
        let source = ActivityQuerySource::new(id(30), self.turn);
        match token {
            Some(token) => {
                ActivityEnrollmentRequest::reuse(source, execution, self.head().revision(), token)
            }
            None => ActivityEnrollmentRequest::first(source, execution, self.head().revision()),
        }
    }
    fn prepare(&self, token: Option<&ActivityPeriodToken>) -> PreparedActivityEnrollment {
        match self
            .storage
            .prepare_activity_enrollment(&self.store, self.request(token))
            .unwrap()
        {
            ActivityEnrollmentPreparation::Prepared(prepared) => prepared,
            _ => panic!("expected preparation"),
        }
    }
    fn enroll(&self) -> ActivityPeriodToken {
        let (command, witness) = self.prepare(None).into_command();
        assert!(matches!(
            self.store.execute(command),
            CommandOutcome::Committed {
                later_failure: None,
                ..
            }
        ));
        match self
            .storage
            .activity_enrollment_status(&self.store, &witness)
            .unwrap()
        {
            ActivityEnrollmentStatus::Committed { token: Some(token) } => token,
            _ => panic!("exact enrollment"),
        }
    }
}
fn limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(65_536).unwrap()
}

#[test]
fn first_enrollment_uses_its_commit_revision_and_exact_reuse_writes_nothing() {
    let f = Fixture::new();
    let before = f.store.home_revision().unwrap();
    let token = f.enroll();
    assert_eq!(
        token.work_period().get(),
        before.checked_next().unwrap().get()
    );
    assert_eq!(token.work_period(), f.head().work_period());
    assert_eq!(
        f.head().source(),
        Some(ActivityQuerySource::new(id(30), f.turn))
    );
    let committed = f.store.home_revision().unwrap();
    assert!(
        matches!(f.storage.prepare_activity_enrollment(&f.store, f.request(Some(&token))).unwrap(), ActivityEnrollmentPreparation::AlreadyEnrolled(same) if same == token)
    );
    assert_eq!(f.store.home_revision().unwrap(), committed);
}

#[test]
fn competing_preparations_cannot_publish_a_second_period() {
    let f = Fixture::new();
    let (first, first_witness) = f.prepare(None).into_command();
    let (second, second_witness) = f.prepare(None).into_command();
    assert!(matches!(
        f.store.execute(first),
        CommandOutcome::Committed { .. }
    ));
    assert!(matches!(
        f.store.execute(second),
        CommandOutcome::NotCommitted { .. }
    ));
    assert!(matches!(
        f.storage
            .activity_enrollment_status(&f.store, &first_witness)
            .unwrap(),
        ActivityEnrollmentStatus::Committed { .. }
    ));
    assert!(matches!(
        f.storage
            .activity_enrollment_status(&f.store, &second_witness)
            .unwrap(),
        ActivityEnrollmentStatus::Committed { .. }
    ));
}

#[test]
fn foreign_token_and_changed_source_are_rejected() {
    let f = Fixture::new();
    let other = Fixture::new();
    let token = f.enroll();
    assert!(
        other
            .storage
            .prepare_activity_enrollment(&other.store, other.request(Some(&token)))
            .is_err()
    );
    let (command, witness) = other.prepare(None).into_command();
    commit(
        &other.store,
        other.storage.clone(),
        batch([FixtureRecord::ActivityQueryHead(other.head())]),
    );
    assert!(matches!(
        other.store.execute(command),
        CommandOutcome::NotCommitted { .. }
    ));
    assert!(matches!(
        other
            .storage
            .activity_enrollment_status(&other.store, &witness)
            .unwrap(),
        ActivityEnrollmentStatus::NotCommitted
    ));
}

#[test]
fn original_witness_classifies_uncertain_commit_without_allocating_again() {
    let f = Fixture::new();
    let (command, witness) = f.prepare(None).into_command();
    f.faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let reconciliation = match f.store.execute(command) {
        CommandOutcome::Indeterminate { reconciliation, .. } => reconciliation,
        _ => panic!("uncertain command"),
    };
    reconciliation.install();
    assert!(matches!(
        f.storage
            .activity_enrollment_status(&f.store, &witness)
            .unwrap(),
        ActivityEnrollmentStatus::Committed { token: Some(_) }
    ));
}
