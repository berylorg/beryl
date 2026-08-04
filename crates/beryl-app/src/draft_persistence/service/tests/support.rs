use std::time::Duration;

use beryl_home_store::{HomeCommand, HomeOpenOptions, HomeSchemaVersion, HomeStore};
use beryl_model::{
    ExecutionBinding, PathFlavor, RootId, RuntimeId, RuntimeMode, RuntimeNativePath, SyndicDraftId,
    SyndicThreadId,
};
use beryl_state::{
    ApplySettings, BerylState, ExpectedSettingRevision, SettingKey, SettingRecord, SettingUpdate,
    SettingValue,
};
use syndic_storage::{
    ComposerAtom, ComposerPayload, CreateThread, SyndicPointReadLimit, SyndicStorage,
    SyndicTimestamp,
};

use crate::draft_persistence::{
    DraftAutosavePublication, DraftCompletionAction, DraftFlushAction, DraftPersistenceSeed,
    DraftPersistenceService, DraftPersistenceTime, execute_draft_save, read_draft_persistence_seed,
};

pub(super) struct Fixture {
    _directory: tempfile::TempDir,
    pub(super) store: HomeStore,
    pub(super) storage: SyndicStorage,
    state: BerylState,
    thread_id: SyndicThreadId,
}

impl Fixture {
    pub(super) fn new(identity_byte: u8) -> Self {
        let directory = tempfile::tempdir().expect("temp home");
        let mut store = HomeStore::open(HomeOpenOptions::new(
            directory.path(),
            HomeSchemaVersion::CURRENT,
        ))
        .expect("open home");
        let state = BerylState::register(&mut store).expect("register Beryl state");
        let storage = SyndicStorage::register(&mut store).expect("register Syndic");
        let thread_id = SyndicThreadId::from_bytes([identity_byte; 16]);
        let creation = CreateThread::ordinary(
            thread_id,
            SyndicDraftId::from_bytes([identity_byte.saturating_add(1); 16]),
            execution_binding(identity_byte),
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

    pub(super) fn seed(&self, published_at: u64) -> DraftPersistenceSeed {
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

    pub(super) fn set_durable(&self, text: &str, updated_at: u64) {
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

    pub(super) fn publish_interval(&self, seconds: u64) -> SettingRecord {
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

fn execution_binding(seed: u8) -> ExecutionBinding {
    ExecutionBinding::new(
        RuntimeId::from_bytes([seed; 16]),
        RootId::from_bytes([seed.saturating_add(2); 16]),
        RuntimeNativePath::from_admitted(
            RuntimeMode::host(),
            PathFlavor::Windows,
            r"C:\work\beryl-draft-persistence",
        )
        .unwrap(),
    )
}

pub(super) fn payload(text: &str) -> ComposerPayload {
    ComposerPayload::new(vec![ComposerAtom::text(text).expect("bounded text")])
        .expect("bounded payload")
}

pub(super) fn time(seconds: u64) -> DraftPersistenceTime {
    DraftPersistenceTime::from_duration(Duration::from_secs(seconds))
}

pub(super) fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(1024 * 1024).expect("point limit")
}

pub(super) fn new_service(fixture: &Fixture) -> DraftPersistenceService {
    DraftPersistenceService::from_seed(fixture.seed(0), DraftAutosavePublication::absent_default())
}
