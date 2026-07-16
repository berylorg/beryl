use std::time::Duration;

use beryl_app::draft_persistence::{
    DraftAutosaveAction, DraftAutosavePublication, DraftAutosavePublicationAction,
    DraftCompletionAction, DraftFlushAction, DraftPersistenceService, DraftPersistenceTime,
    execute_draft_save, read_draft_persistence_seed,
};
use beryl_home_store::{HomeCommand, HomeOpenOptions, HomeSchemaVersion, HomeStore};
use beryl_model::{SyndicDraftId, SyndicThreadId};
use beryl_state::{
    ApplySettings, BerylState, ExpectedSettingRevision, SettingKey, SettingRecord, SettingUpdate,
    SettingValue,
};
use syndic_storage::{
    ComposerAtom, ComposerPayload, CreateThread, SyndicPointReadLimit, SyndicStorage,
    SyndicTimestamp,
};

struct Fixture {
    _directory: tempfile::TempDir,
    store: HomeStore,
    storage: SyndicStorage,
    state: BerylState,
    thread_id: SyndicThreadId,
}

impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().expect("temp home");
        let mut store = HomeStore::open(HomeOpenOptions::new(
            directory.path(),
            HomeSchemaVersion::CURRENT,
        ))
        .expect("open home");
        let state = BerylState::register(&mut store).expect("register Beryl state");
        let storage = SyndicStorage::register(&mut store).expect("register Syndic");
        let thread_id = SyndicThreadId::from_bytes([1; 16]);
        let creation = CreateThread::ordinary(
            thread_id,
            SyndicDraftId::from_bytes([2; 16]),
            SyndicTimestamp::from_unix_millis(0),
        );
        let mut command = HomeCommand::new(store.home_revision().expect("home revision"));
        command
            .add(
                storage.create_thread(storage.revision(&store).expect("domain revision"), creation),
            )
            .expect("add creation");
        store.execute(command).expect("create thread");
        Self {
            _directory: directory,
            store,
            storage,
            state,
            thread_id,
        }
    }

    fn seed(&self, published_at: u64) -> beryl_app::draft_persistence::DraftPersistenceSeed {
        read_draft_persistence_seed(
            &self.store,
            &self.storage,
            self.thread_id,
            point_limit(),
            time(published_at),
        )
        .expect("read seed")
        .expect("current draft")
    }

    fn set_durable(&self, text: &str, updated_at: u64) {
        let mut service = DraftPersistenceService::from_seed(
            self.seed(0),
            DraftAutosavePublication::absent_default(),
        );
        service
            .edit(payload(text), SyndicTimestamp::from_unix_millis(updated_at))
            .expect("edit durable seed");
        let request = match service.flush().expect("flush durable seed") {
            DraftFlushAction::Started(request) => request,
            other => panic!("unexpected durable seed action: {other:?}"),
        };
        let execution = execute_draft_save(&self.store, &self.storage, &request, point_limit());
        assert!(execution.failure().is_none());
        assert!(matches!(
            service
                .complete(execution, time(0))
                .expect("complete durable seed"),
            DraftCompletionAction::Published {
                flush_complete: true
            }
        ));
    }

    fn publish_interval(&self, seconds: u64) -> SettingRecord {
        let settings = self.state.settings();
        let current = settings
            .setting(&self.store, SettingKey::DraftAutosaveInterval)
            .expect("read setting");
        let expected = current
            .as_ref()
            .map_or(ExpectedSettingRevision::Absent, |record| {
                ExpectedSettingRevision::Exact(record.revision())
            });
        let update = SettingUpdate::new(
            SettingKey::DraftAutosaveInterval,
            expected,
            SettingValue::draft_autosave_interval_seconds(seconds),
        );
        let apply = ApplySettings::new(vec![update]).expect("setting update");
        let mut command = HomeCommand::new(self.store.home_revision().expect("home revision"));
        command
            .add(settings.apply(
                settings.revision(&self.store).expect("settings revision"),
                apply,
            ))
            .expect("add setting update");
        self.store.execute(command).expect("publish setting");
        settings
            .setting(&self.store, SettingKey::DraftAutosaveInterval)
            .expect("read setting")
            .expect("published setting")
    }
}

fn payload(text: &str) -> ComposerPayload {
    ComposerPayload::new(vec![ComposerAtom::text(text).expect("bounded text")])
        .expect("bounded payload")
}

fn time(seconds: u64) -> DraftPersistenceTime {
    DraftPersistenceTime::from_duration(Duration::from_secs(seconds))
}

fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(1024 * 1024).expect("point limit")
}

fn new_service(fixture: &Fixture) -> DraftPersistenceService {
    DraftPersistenceService::from_seed(fixture.seed(0), DraftAutosavePublication::absent_default())
}

#[test]
fn autosave_is_dirty_only_and_due_from_last_publication() {
    let fixture = Fixture::new();
    let mut service = new_service(&fixture);
    assert!(matches!(
        service.poll_autosave(time(30)).expect("poll"),
        DraftAutosaveAction::Clean
    ));
    assert!(
        service
            .edit(payload("new"), SyndicTimestamp::from_unix_millis(1))
            .expect("edit")
    );
    assert!(
        !service
            .edit(payload("new"), SyndicTimestamp::from_unix_millis(2))
            .expect("no-op edit")
    );
    assert!(matches!(
        service.poll_autosave(time(29)).expect("poll"),
        DraftAutosaveAction::NotDue
    ));
    assert!(matches!(
        service.poll_autosave(time(30)).expect("poll"),
        DraftAutosaveAction::Started(_)
    ));
}

#[test]
fn later_edit_is_not_cleaned_by_an_older_save() {
    let fixture = Fixture::new();
    let mut service = new_service(&fixture);
    service
        .edit(payload("first"), SyndicTimestamp::from_unix_millis(1))
        .expect("edit");
    let request = match service.poll_autosave(time(30)).expect("start") {
        DraftAutosaveAction::Started(request) => request,
        other => panic!("unexpected action: {other:?}"),
    };
    service
        .edit(payload("later"), SyndicTimestamp::from_unix_millis(2))
        .expect("later edit");
    let execution = execute_draft_save(&fixture.store, &fixture.storage, &request, point_limit());
    let action = service.complete(execution, time(31)).expect("complete");
    assert!(matches!(
        action,
        DraftCompletionAction::Published {
            flush_complete: false
        }
    ));
    assert!(service.is_dirty());
    assert_eq!(service.editor_payload(), &payload("later"));
}

#[test]
fn lifecycle_flush_drains_and_chains_the_latest_edit() {
    let fixture = Fixture::new();
    let mut service = new_service(&fixture);
    service
        .edit(payload("one"), SyndicTimestamp::from_unix_millis(1))
        .expect("edit");
    let first = match service.poll_autosave(time(30)).expect("start") {
        DraftAutosaveAction::Started(request) => request,
        other => panic!("unexpected action: {other:?}"),
    };
    service
        .edit(payload("two"), SyndicTimestamp::from_unix_millis(2))
        .expect("edit");
    assert!(matches!(
        service.flush().expect("flush"),
        DraftFlushAction::Waiting(token) if token == first.token()
    ));
    let execution = execute_draft_save(&fixture.store, &fixture.storage, &first, point_limit());
    let second = match service.complete(execution, time(31)).expect("complete") {
        DraftCompletionAction::Chained(request) => request,
        other => panic!("unexpected action: {other:?}"),
    };
    assert_eq!(second.payload(), &payload("two"));
    let execution = execute_draft_save(&fixture.store, &fixture.storage, &second, point_limit());
    assert!(matches!(
        service.complete(execution, time(32)).expect("complete"),
        DraftCompletionAction::Published {
            flush_complete: true
        }
    ));
    assert!(!service.is_dirty());
}

#[test]
fn interval_changes_rearm_from_setting_publication_time() {
    let fixture = Fixture::new();
    let mut service = DraftPersistenceService::from_seed(
        fixture.seed(10),
        DraftAutosavePublication::absent_default(),
    );
    service
        .edit(payload("new"), SyndicTimestamp::from_unix_millis(11_000))
        .expect("edit");
    assert!(matches!(
        service.poll_autosave(time(20)).expect("poll"),
        DraftAutosaveAction::NotDue
    ));
    let generation = service.timer_generation();
    let record = fixture.publish_interval(5);
    let publication = DraftAutosavePublication::from_record(&record).expect("publication");
    assert_eq!(
        service
            .apply_autosave_publication(publication, time(20))
            .expect("set interval"),
        DraftAutosavePublicationAction::Applied
    );
    assert!(service.timer_generation() > generation);
    assert!(matches!(
        service.poll_autosave(time(24)).expect("poll"),
        DraftAutosaveAction::NotDue
    ));
    assert!(matches!(
        service.poll_autosave(time(25)).expect("poll"),
        DraftAutosaveAction::Started(_)
    ));
}

#[path = "draft_persistence/integration.rs"]
mod integration;
