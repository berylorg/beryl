#![cfg(feature = "test-faults")]

use std::time::Duration;

use beryl_app::draft_persistence::{
    DraftAutosavePublication, DraftCompletionAction, DraftFlushAction, DraftPersistenceService,
    DraftPersistenceTime, DraftReconciliationAction, DraftSuspensionCause, execute_draft_save,
    read_draft_persistence_seed,
};
use beryl_home_store::{
    HomeCommand, HomeHealthState, HomeOpenOptions, HomeSchemaVersion, HomeStore,
    test_faults::{FaultController, FaultPoint},
};
use beryl_model::{SyndicDraftId, SyndicThreadId};
use syndic_storage::{
    CONTENT_APPEND_MAX_CHUNKS, ComposerAtom, ComposerPayload, ContentAppend, ContentBuild,
    CreateThread, PreparedContent, SyndicPointReadLimit, SyndicStorage, SyndicTimestamp,
};

struct Fixture {
    _directory: tempfile::TempDir,
    faults: FaultController,
    store: HomeStore,
    storage: SyndicStorage,
    thread_id: SyndicThreadId,
}

impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let faults = FaultController::new();
        let mut store = HomeStore::open_with_faults(
            HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
            faults.clone(),
        )
        .unwrap();
        let storage = SyndicStorage::register(&mut store).unwrap();
        let thread_id = SyndicThreadId::from_bytes([1; 16]);
        let creation = CreateThread::ordinary(
            thread_id,
            SyndicDraftId::from_bytes([2; 16]),
            SyndicTimestamp::from_unix_millis(0),
        );
        let mut command = HomeCommand::new(store.home_revision().unwrap());
        command
            .add(storage.create_thread(storage.revision(&store).unwrap(), creation))
            .unwrap();
        store.execute(command).unwrap();
        Self {
            _directory: directory,
            faults,
            store,
            storage,
            thread_id,
        }
    }

    fn seed(
        &self,
        storage: &SyndicStorage,
        published_at: u64,
    ) -> beryl_app::draft_persistence::DraftPersistenceSeed {
        read_draft_persistence_seed(
            &self.store,
            storage,
            self.thread_id,
            point_limit(),
            time(published_at),
        )
        .unwrap()
        .unwrap()
    }

    fn service(&self) -> DraftPersistenceService {
        DraftPersistenceService::from_seed(
            self.seed(&self.storage, 0),
            DraftAutosavePublication::absent_default(),
        )
    }
}

fn payload(text: &str) -> ComposerPayload {
    ComposerPayload::new(vec![ComposerAtom::text(text).unwrap()]).unwrap()
}

fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(1024 * 1024).unwrap()
}

fn time(seconds: u64) -> DraftPersistenceTime {
    DraftPersistenceTime::from_duration(Duration::from_secs(seconds))
}

fn stage_payload_content(fixture: &Fixture, payload: &ComposerPayload) {
    stage_payload_prefix(fixture, payload, staging_command_count(payload));
}

fn staging_command_count(payload: &ComposerPayload) -> usize {
    let content = PreparedContent::composer(payload).unwrap();
    1 + content.chunks().len().div_ceil(CONTENT_APPEND_MAX_CHUNKS)
}

fn stage_payload_prefix(fixture: &Fixture, payload: &ComposerPayload, command_count: usize) {
    if command_count == 0 {
        return;
    }
    let content = PreparedContent::composer(payload).unwrap();
    let mut command = HomeCommand::new(fixture.store.home_revision().unwrap());
    command
        .add(fixture.storage.begin_content(
            fixture.storage.revision(&fixture.store).unwrap(),
            ContentBuild::from_prepared(&content),
        ))
        .unwrap();
    fixture.store.execute(command).unwrap();

    let mut manifest = content.building_manifest();
    for _ in 1..command_count {
        let append = ContentAppend::prepare(&manifest, &content)
            .unwrap()
            .expect("staging prefix exceeds complete content");
        let next = append.next_manifest().clone();
        let mut command = HomeCommand::new(fixture.store.home_revision().unwrap());
        command
            .add(
                fixture
                    .storage
                    .append_content(fixture.storage.revision(&fixture.store).unwrap(), append),
            )
            .unwrap();
        fixture.store.execute(command).unwrap();
        manifest = next;
    }
}

#[test]
fn actual_pre_commit_ambiguity_reconciles_old_then_flushes_the_retained_editor() {
    let fixture = Fixture::new();
    let mut service = fixture.service();
    service
        .edit(payload("retained"), SyndicTimestamp::from_unix_millis(1))
        .unwrap();
    let request = match service.flush().unwrap() {
        DraftFlushAction::Started(request) => request,
        other => panic!("unexpected flush action: {other:?}"),
    };

    fixture.faults.fail_next(FaultPoint::BeforeCommit);
    let execution = execute_draft_save(&fixture.store, &fixture.storage, &request, point_limit());
    assert!(execution.failure().is_some());
    assert!(matches!(
        service.complete(execution, time(1)).unwrap(),
        DraftCompletionAction::Suspended(DraftSuspensionCause::AmbiguousStorageFailure)
    ));
    assert_eq!(fixture.store.health().state(), HomeHealthState::Verifying);
    fixture.store.verify_health().unwrap();

    let retry = match service
        .reconcile(fixture.seed(&fixture.storage, 2))
        .unwrap()
    {
        DraftReconciliationAction::Chained(request) => request,
        other => panic!("unexpected reconciliation action: {other:?}"),
    };
    assert_eq!(service.editor_payload(), &payload("retained"));
    let execution = execute_draft_save(&fixture.store, &fixture.storage, &retry, point_limit());
    assert!(matches!(
        service.complete(execution, time(3)).unwrap(),
        DraftCompletionAction::Published {
            flush_complete: true
        }
    ));
    assert!(!service.is_dirty());
}

#[test]
fn actual_post_persist_ambiguity_reconciles_new_after_same_home_recovery() {
    let fixture = Fixture::new();
    let mut service = fixture.service();
    service
        .edit(payload("durable"), SyndicTimestamp::from_unix_millis(1))
        .unwrap();
    let request = match service.flush().unwrap() {
        DraftFlushAction::Started(request) => request,
        other => panic!("unexpected flush action: {other:?}"),
    };
    stage_payload_content(&fixture, request.payload());

    fixture.faults.fail_next(FaultPoint::AfterPersist);
    let execution = execute_draft_save(&fixture.store, &fixture.storage, &request, point_limit());
    assert!(matches!(
        service.complete(execution, time(1)).unwrap(),
        DraftCompletionAction::Suspended(DraftSuspensionCause::AmbiguousStorageFailure)
    ));
    fixture.faults.fail_next(FaultPoint::BeforeVerification);
    assert!(fixture.store.verify_health().is_err());
    assert_eq!(fixture.store.health().state(), HomeHealthState::Failed);

    let recovery = fixture.store.recover_same_home().unwrap();
    let storage = SyndicStorage::reacquire(&fixture.store).unwrap();
    assert!(matches!(
        service.reconcile(fixture.seed(&storage, 2)).unwrap(),
        DraftReconciliationAction::FlushComplete
    ));
    assert_eq!(service.binding().home_generation(), recovery.generation());
    assert_eq!(service.editor_payload(), &payload("durable"));
    assert!(!service.is_dirty());
    assert_eq!(fixture.seed(&storage, 2).payload(), &payload("durable"));
}

#[test]
fn persisted_staging_ambiguity_remains_unreachable_and_resumes_after_recovery() {
    let fixture = Fixture::new();
    let mut service = fixture.service();
    service
        .edit(payload("retained"), SyndicTimestamp::from_unix_millis(1))
        .unwrap();
    let request = match service.flush().unwrap() {
        DraftFlushAction::Started(request) => request,
        other => panic!("unexpected flush action: {other:?}"),
    };

    fixture.faults.fail_next(FaultPoint::AfterPersist);
    let execution = execute_draft_save(&fixture.store, &fixture.storage, &request, point_limit());
    assert!(matches!(
        service.complete(execution, time(1)).unwrap(),
        DraftCompletionAction::Suspended(DraftSuspensionCause::AmbiguousStorageFailure)
    ));
    fixture.store.verify_health().unwrap();
    assert_eq!(
        fixture.seed(&fixture.storage, 2).payload(),
        &ComposerPayload::default()
    );

    let retry = match service
        .reconcile(fixture.seed(&fixture.storage, 2))
        .unwrap()
    {
        DraftReconciliationAction::Chained(request) => request,
        other => panic!("unexpected reconciliation action: {other:?}"),
    };
    let execution = execute_draft_save(&fixture.store, &fixture.storage, &retry, point_limit());
    assert!(matches!(
        service.complete(execution, time(3)).unwrap(),
        DraftCompletionAction::Published {
            flush_complete: true
        }
    ));
    assert_eq!(
        fixture.seed(&fixture.storage, 3).payload(),
        &payload("retained")
    );
}

#[test]
fn every_persisted_content_command_cut_reconciles_to_whole_old_or_new_draft() {
    let exact_payload = payload(&"cut ".repeat(400_000));
    let stage_commands = staging_command_count(&exact_payload);
    assert!(stage_commands >= 3);

    for prefix in 0..=stage_commands {
        let fixture = Fixture::new();
        let mut service = fixture.service();
        service
            .edit(exact_payload.clone(), SyndicTimestamp::from_unix_millis(1))
            .unwrap();
        let request = match service.flush().unwrap() {
            DraftFlushAction::Started(request) => request,
            other => panic!("unexpected flush action: {other:?}"),
        };
        stage_payload_prefix(&fixture, &exact_payload, prefix);

        fixture.faults.fail_next(FaultPoint::AfterPersist);
        let execution =
            execute_draft_save(&fixture.store, &fixture.storage, &request, point_limit());
        assert!(matches!(
            service.complete(execution, time(1)).unwrap(),
            DraftCompletionAction::Suspended(DraftSuspensionCause::AmbiguousStorageFailure)
        ));
        fixture.store.verify_health().unwrap();
        let seed = fixture.seed(&fixture.storage, 2);

        if prefix == stage_commands {
            assert_eq!(seed.payload(), &exact_payload);
            assert!(matches!(
                service.reconcile(seed).unwrap(),
                DraftReconciliationAction::FlushComplete
            ));
        } else {
            assert_eq!(seed.payload(), &ComposerPayload::default());
            let retry = match service.reconcile(seed).unwrap() {
                DraftReconciliationAction::Chained(request) => request,
                other => panic!("unexpected old-state reconciliation: {other:?}"),
            };
            let execution =
                execute_draft_save(&fixture.store, &fixture.storage, &retry, point_limit());
            assert!(matches!(
                service.complete(execution, time(3)).unwrap(),
                DraftCompletionAction::Published {
                    flush_complete: true
                }
            ));
            assert_eq!(fixture.seed(&fixture.storage, 3).payload(), &exact_payload);
        }
        fixture.store.validate_registered_domains().unwrap();
    }
}
