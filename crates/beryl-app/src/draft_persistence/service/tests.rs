mod support;

use beryl_model::DraftRevision;
use syndic_storage::SyndicTimestamp;

use self::support::{Fixture, new_service, payload, time};
use crate::draft_persistence::{
    DraftAutosaveAction, DraftAutosavePublication, DraftAutosavePublicationAction,
    DraftCompletionAction, DraftFlushAction, DraftReconciliationAction, DraftSaveExecution,
    DraftSaveOutcome, DraftSaveRequest, DraftSuspensionCause,
};

fn completion(request: &DraftSaveRequest, outcome: DraftSaveOutcome) -> DraftSaveExecution {
    super::super::executor::DraftSaveExecution::test_completion(request.token(), outcome)
}

#[test]
fn stale_completion_cannot_touch_the_current_request() {
    let fixture = Fixture::new(1);
    let mut service = new_service(&fixture);
    service
        .edit(payload("one"), SyndicTimestamp::from_unix_millis(1))
        .expect("edit");
    let first = match service.flush().expect("flush") {
        DraftFlushAction::Started(request) => request,
        other => panic!("unexpected action: {other:?}"),
    };
    service
        .complete(
            completion(
                &first,
                DraftSaveOutcome::Committed {
                    revision: DraftRevision::new(2).expect("revision"),
                },
            ),
            time(1),
        )
        .expect("complete");
    service
        .edit(payload("two"), SyndicTimestamp::from_unix_millis(2))
        .expect("edit");
    let second = match service.flush().expect("flush") {
        DraftFlushAction::Started(request) => request,
        other => panic!("unexpected action: {other:?}"),
    };
    assert!(matches!(
        service
            .complete(
                completion(
                    &first,
                    DraftSaveOutcome::Committed {
                        revision: DraftRevision::new(3).expect("revision"),
                    },
                ),
                time(2),
            )
            .expect("stale completion"),
        DraftCompletionAction::Stale
    ));
    assert_eq!(service.in_flight(), Some(second.token()));
    assert!(service.is_dirty());
}

#[test]
fn conflict_suspends_and_exact_seed_chains_pending_flush() {
    let fixture = Fixture::new(3);
    let mut service = new_service(&fixture);
    service
        .edit(payload("local"), SyndicTimestamp::from_unix_millis(1))
        .expect("edit");
    let request = match service.flush().expect("flush") {
        DraftFlushAction::Started(request) => request,
        other => panic!("unexpected action: {other:?}"),
    };
    assert!(matches!(
        service
            .complete(
                completion(
                    &request,
                    DraftSaveOutcome::RequiresReconciliation(
                        DraftSuspensionCause::RevisionConflict,
                    ),
                ),
                time(1),
            )
            .expect("complete"),
        DraftCompletionAction::Suspended(DraftSuspensionCause::RevisionConflict)
    ));
    let binding_generation = service.binding().generation();
    assert!(matches!(
        service.reconcile(fixture.seed(2)).expect("reconcile"),
        DraftReconciliationAction::Chained(_)
    ));
    assert!(service.binding().generation() > binding_generation);
    assert_eq!(service.editor_payload(), &payload("local"));
}

#[test]
fn ambiguous_failure_accepts_only_a_whole_old_or_new_state() {
    let fixture = Fixture::new(5);
    let mut service = new_service(&fixture);
    service
        .edit(payload("local"), SyndicTimestamp::from_unix_millis(1))
        .expect("edit");
    let request = match service.poll_autosave(time(30)).expect("start") {
        DraftAutosaveAction::Started(request) => request,
        other => panic!("unexpected action: {other:?}"),
    };
    service
        .complete(
            completion(
                &request,
                DraftSaveOutcome::RequiresReconciliation(
                    DraftSuspensionCause::AmbiguousStorageFailure,
                ),
            ),
            time(30),
        )
        .expect("complete");
    assert!(matches!(
        service
            .reconcile(fixture.seed(31))
            .expect("old state reconciliation"),
        DraftReconciliationAction::Ready
    ));
    assert!(service.is_dirty());

    let other = Fixture::new(7);
    let mut other_service = new_service(&other);
    other_service
        .edit(payload("local"), SyndicTimestamp::from_unix_millis(1))
        .expect("edit");
    let request = match other_service.flush().expect("flush") {
        DraftFlushAction::Started(request) => request,
        other => panic!("unexpected action: {other:?}"),
    };
    other_service
        .complete(
            completion(
                &request,
                DraftSaveOutcome::RequiresReconciliation(
                    DraftSuspensionCause::AmbiguousStorageFailure,
                ),
            ),
            time(1),
        )
        .expect("suspend");
    other.set_durable("unexplained", 2);
    assert!(other_service.reconcile(other.seed(2)).is_err());
}

#[test]
fn newer_setting_publication_survives_reconciliation_of_an_older_request() {
    let fixture = Fixture::new(9);
    let mut service = new_service(&fixture);
    service
        .edit(payload("local"), SyndicTimestamp::from_unix_millis(1))
        .expect("edit");
    let request = match service.poll_autosave(time(30)).expect("start") {
        DraftAutosaveAction::Started(request) => request,
        other => panic!("unexpected action: {other:?}"),
    };
    service
        .complete(
            completion(
                &request,
                DraftSaveOutcome::RequiresReconciliation(
                    DraftSuspensionCause::AmbiguousStorageFailure,
                ),
            ),
            time(30),
        )
        .expect("suspend");
    let record = fixture.publish_interval(5);
    let publication = DraftAutosavePublication::from_record(&record).expect("publication");
    assert_eq!(
        service
            .apply_autosave_publication(publication, time(100))
            .expect("apply newer publication"),
        DraftAutosavePublicationAction::Applied
    );
    let timer_generation = service.timer_generation();
    assert!(matches!(
        service.reconcile(fixture.seed(200)).expect("reconcile"),
        DraftReconciliationAction::Ready
    ));
    assert_eq!(service.timer_generation(), timer_generation);
    assert_eq!(service.interval(), publication.interval());
    assert_eq!(
        service
            .apply_autosave_publication(publication, time(300))
            .expect("reject stale publication"),
        DraftAutosavePublicationAction::Stale
    );
    assert!(matches!(
        service.poll_autosave(time(104)).expect("poll"),
        DraftAutosaveAction::NotDue
    ));
    assert!(matches!(
        service.poll_autosave(time(105)).expect("poll"),
        DraftAutosaveAction::Started(_)
    ));
}
