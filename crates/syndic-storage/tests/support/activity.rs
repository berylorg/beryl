use beryl_home_store::{CommandOutcome, HomeStore};
use beryl_model::{SyndicThreadId, SyndicTurnId};
use syndic_storage::{
    ActivityEnrollmentPreparation, ActivityEnrollmentRequest, ActivityEnrollmentStatus,
    ActivityPeriodToken, ActivityQuerySource, ActivitySourceQualification, ProjectionLifecycle,
    SyndicPointReadLimit, SyndicStorage,
};

pub fn fixture_activity(
    store: &HomeStore,
    storage: &SyndicStorage,
    thread: SyndicThreadId,
    turn: SyndicTurnId,
) -> ActivitySourceQualification {
    let limit = SyndicPointReadLimit::new(65_536).unwrap();
    let head = storage
        .activity_query_head(store, thread, limit)
        .unwrap()
        .unwrap();
    let execution = storage
        .thread_execution(store, thread, limit)
        .unwrap()
        .unwrap();
    ActivitySourceQualification::Current {
        token: ActivityPeriodToken::for_fixture(
            store.home_id(),
            execution.execution().runtime_id(),
            head.work_period(),
        ),
        source: ActivityQuerySource::new(thread, turn),
    }
}

pub fn enroll_fixture_activity(
    store: &HomeStore,
    storage: &SyndicStorage,
    thread: SyndicThreadId,
    turn: SyndicTurnId,
) -> ActivitySourceQualification {
    let limit = SyndicPointReadLimit::new(65_536).unwrap();
    let head = storage
        .activity_query_head(store, thread, limit)
        .unwrap()
        .unwrap();
    let execution = storage
        .thread_execution(store, thread, limit)
        .unwrap()
        .unwrap();
    let source = ActivityQuerySource::new(thread, turn);
    let token = if head.source() == Some(source)
        && head.source_active()
        && head.lifecycle() == ProjectionLifecycle::Current
    {
        ActivityPeriodToken::for_fixture(
            store.home_id(),
            execution.execution().runtime_id(),
            head.work_period(),
        )
    } else {
        let prepared = storage
            .prepare_activity_enrollment(
                store,
                ActivityEnrollmentRequest::first(
                    source,
                    execution.execution().clone(),
                    head.revision(),
                ),
            )
            .unwrap();
        let ActivityEnrollmentPreparation::Prepared(prepared) = prepared else {
            panic!("first fixture enrollment")
        };
        let (command, witness) = prepared.into_command();
        assert!(matches!(
            store.execute(command),
            CommandOutcome::Committed {
                later_failure: None,
                ..
            }
        ));
        match storage.activity_enrollment_status(store, &witness).unwrap() {
            ActivityEnrollmentStatus::Committed { token: Some(token) } => token,
            _ => panic!("committed fixture enrollment"),
        }
    };
    ActivitySourceQualification::Current { token, source }
}

pub fn retired_activity(
    store: &HomeStore,
    storage: &SyndicStorage,
    thread: SyndicThreadId,
    turn: SyndicTurnId,
) -> ActivitySourceQualification {
    match storage
        .activity_retirement_fingerprint(store, ActivityQuerySource::new(thread, turn))
        .unwrap()
    {
        Some(fingerprint) => ActivitySourceQualification::Retired(fingerprint),
        None => ActivitySourceQualification::Unenrolled,
    }
}
