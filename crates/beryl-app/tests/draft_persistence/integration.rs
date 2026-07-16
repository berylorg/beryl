use super::*;
use beryl_app::draft_persistence::{DraftKnownUnchanged, DraftSaveOutcome};

#[test]
fn preload_is_clean_and_interval_validation_is_closed() {
    let fixture = Fixture::new();
    fixture.set_durable("restored", 1);
    let service = DraftPersistenceService::from_seed(
        fixture.seed(1),
        DraftAutosavePublication::absent_default(),
    );
    assert!(!service.is_dirty());
    assert_eq!(service.editor_payload(), &payload("restored"));
    let invalid = fixture.publish_interval(4);
    assert!(DraftAutosavePublication::from_record(&invalid).is_err());
    let minimum = fixture.publish_interval(5);
    assert!(DraftAutosavePublication::from_record(&minimum).is_ok());
    let maximum = fixture.publish_interval(300);
    assert!(DraftAutosavePublication::from_record(&maximum).is_ok());
    let invalid = fixture.publish_interval(301);
    assert!(DraftAutosavePublication::from_record(&invalid).is_err());
}

#[test]
fn exact_setting_revision_rejects_stale_timer_publication() {
    let fixture = Fixture::new();
    let mut service = new_service(&fixture);
    service
        .edit(payload("dirty"), SyndicTimestamp::from_unix_millis(1))
        .expect("edit");
    let record = fixture.publish_interval(5);
    let publication = DraftAutosavePublication::from_record(&record).expect("publication");
    assert_eq!(
        service
            .apply_autosave_publication(publication, time(10))
            .expect("apply"),
        DraftAutosavePublicationAction::Applied
    );
    let generation = service.timer_generation();
    assert_eq!(
        service
            .apply_autosave_publication(publication, time(12))
            .expect("reject stale"),
        DraftAutosavePublicationAction::Stale
    );
    assert_eq!(service.timer_generation(), generation);
    assert!(matches!(
        service.poll_autosave(time(14)).expect("poll"),
        DraftAutosaveAction::NotDue
    ));
    assert!(matches!(
        service.poll_autosave(time(15)).expect("poll"),
        DraftAutosaveAction::Started(_)
    ));
}

#[test]
fn regressed_edit_timestamp_preserves_the_editor_payload() {
    let fixture = Fixture::new();
    fixture.set_durable("durable", 100);
    let mut service = DraftPersistenceService::from_seed(
        fixture.seed(1),
        DraftAutosavePublication::absent_default(),
    );
    assert!(
        service
            .edit(payload("invalid"), SyndicTimestamp::from_unix_millis(99))
            .is_err()
    );
    assert_eq!(service.editor_payload(), &payload("durable"));
    assert!(!service.is_dirty());
}

#[test]
fn direct_executor_persists_and_publishes_the_exact_request() {
    let fixture = Fixture::new();
    let mut service = new_service(&fixture);
    service
        .edit(payload("saved"), SyndicTimestamp::from_unix_millis(1))
        .expect("edit");
    let request = match service.flush().expect("flush") {
        DraftFlushAction::Started(request) => request,
        other => panic!("unexpected action: {other:?}"),
    };
    let execution = execute_draft_save(&fixture.store, &fixture.storage, &request, point_limit());
    assert!(execution.failure().is_none());
    assert!(matches!(
        service
            .complete(execution, time(1))
            .expect("publish completion"),
        DraftCompletionAction::Published {
            flush_complete: true
        }
    ));
    let current = fixture
        .storage
        .current_draft(&fixture.store, fixture.thread_id, point_limit())
        .expect("read current")
        .expect("current draft");
    assert_eq!(fixture.seed(1).payload(), &payload("saved"));
    assert_eq!(current.draft().revision().get(), 2);
    assert!(!service.is_dirty());
}

#[test]
fn large_editor_payload_roundtrips_through_staging_and_preload() {
    let fixture = Fixture::new();
    let mut service = new_service(&fixture);
    let large = payload(&"large ".repeat(500_000));
    assert!(large.utf8_bytes() > 2_000_000);
    service
        .edit(large.clone(), SyndicTimestamp::from_unix_millis(1))
        .expect("large edit");
    let request = match service.flush().expect("large flush") {
        DraftFlushAction::Started(request) => request,
        other => panic!("unexpected action: {other:?}"),
    };
    let execution = execute_draft_save(&fixture.store, &fixture.storage, &request, point_limit());
    assert!(execution.failure().is_none());
    assert!(matches!(
        service
            .complete(execution, time(1))
            .expect("complete large save"),
        DraftCompletionAction::Published {
            flush_complete: true
        }
    ));
    assert_eq!(fixture.seed(1).payload(), &large);
}

#[test]
fn direct_executor_honors_pre_admission_cancellation() {
    let fixture = Fixture::new();
    let mut service = new_service(&fixture);
    service
        .edit(payload("cancelled"), SyndicTimestamp::from_unix_millis(1))
        .expect("edit");
    let request = match service.flush().expect("flush") {
        DraftFlushAction::Started(request) => request,
        other => panic!("unexpected action: {other:?}"),
    };
    request.cancellation().cancel();
    let execution = execute_draft_save(&fixture.store, &fixture.storage, &request, point_limit());
    assert!(matches!(
        execution.outcome(),
        DraftSaveOutcome::KnownUnchanged(DraftKnownUnchanged::CancelledBeforeAdmission)
    ));
    assert!(matches!(
        service.complete(execution, time(1)).expect("complete"),
        DraftCompletionAction::KnownUnchanged {
            flush_failed: true,
            ..
        }
    ));
    assert!(service.is_dirty());
    let current = fixture
        .storage
        .current_draft(&fixture.store, fixture.thread_id, point_limit())
        .expect("read current")
        .expect("current draft");
    assert_eq!(current.draft().revision().get(), 1);
    assert_eq!(fixture.seed(1).payload(), &ComposerPayload::default());
}

#[test]
fn newer_setting_publication_survives_an_older_commit_completion() {
    let fixture = Fixture::new();
    let mut service = new_service(&fixture);
    service
        .edit(payload("saved"), SyndicTimestamp::from_unix_millis(1))
        .expect("edit");
    let request = match service.poll_autosave(time(30)).expect("start") {
        DraftAutosaveAction::Started(request) => request,
        other => panic!("unexpected action: {other:?}"),
    };
    let execution = execute_draft_save(&fixture.store, &fixture.storage, &request, point_limit());
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
        service.complete(execution, time(200)).expect("complete"),
        DraftCompletionAction::Published {
            flush_complete: false
        }
    ));
    assert_eq!(service.timer_generation(), timer_generation);
    assert_eq!(service.interval(), publication.interval());
    service
        .edit(payload("later"), SyndicTimestamp::from_unix_millis(2))
        .expect("later edit");
    assert!(matches!(
        service.poll_autosave(time(104)).expect("poll"),
        DraftAutosaveAction::NotDue
    ));
    assert!(matches!(
        service.poll_autosave(time(105)).expect("poll"),
        DraftAutosaveAction::Started(_)
    ));
}

#[test]
fn execution_from_another_exact_binding_is_stale() {
    let first = Fixture::new();
    let second = Fixture::new();
    let mut first_service = new_service(&first);
    let mut second_service = new_service(&second);
    first_service
        .edit(payload("same"), SyndicTimestamp::from_unix_millis(1))
        .expect("first edit");
    second_service
        .edit(payload("same"), SyndicTimestamp::from_unix_millis(1))
        .expect("second edit");
    let first_request = match first_service.flush().expect("first flush") {
        DraftFlushAction::Started(request) => request,
        other => panic!("unexpected action: {other:?}"),
    };
    let second_request = match second_service.flush().expect("second flush") {
        DraftFlushAction::Started(request) => request,
        other => panic!("unexpected action: {other:?}"),
    };
    assert_eq!(
        first_request.token().binding_generation(),
        second_request.token().binding_generation()
    );
    assert_eq!(
        first_request.token().edit_generation(),
        second_request.token().edit_generation()
    );
    assert_eq!(
        first_request.token().request_generation(),
        second_request.token().request_generation()
    );
    assert_ne!(first_request.token(), second_request.token());
    let foreign_execution =
        execute_draft_save(&first.store, &first.storage, &first_request, point_limit());
    assert!(matches!(
        second_service
            .complete(foreign_execution, time(1))
            .expect("reject foreign completion"),
        DraftCompletionAction::Stale
    ));
    assert_eq!(second_service.in_flight(), Some(second_request.token()));
    assert!(second_service.is_dirty());
}
