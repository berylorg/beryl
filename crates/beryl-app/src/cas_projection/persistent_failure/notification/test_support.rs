use std::sync::{Arc, mpsc};

use beryl_home_store::{
    HomeCommand, HomeHealthState, HomeOpenOptions, HomeSchemaVersion, HomeStore,
    test_faults::{FaultController, FaultPoint},
};
use beryl_state::{
    ApplySettings, BerylState, ExpectedSettingRevision, SettingKey, SettingUpdate, SettingValue,
};

use super::{
    PersistentFailureNotification, ProjectionServiceGeneration,
    persistent_failure_notification_channel,
};

pub(in crate::cas_projection::persistent_failure) struct FailedNotificationFixture {
    pub(in crate::cas_projection::persistent_failure) notification: PersistentFailureNotification,
    pub(in crate::cas_projection::persistent_failure) receiver: mpsc::Receiver<()>,
    pub(in crate::cas_projection::persistent_failure) home: Arc<HomeStore>,
    _directory: tempfile::TempDir,
}

pub(in crate::cas_projection::persistent_failure) struct VerifyingNotificationFixture {
    pub(in crate::cas_projection::persistent_failure) notification: PersistentFailureNotification,
    pub(in crate::cas_projection::persistent_failure) receiver: mpsc::Receiver<()>,
    pub(in crate::cas_projection::persistent_failure) home: Arc<HomeStore>,
    pub(in crate::cas_projection::persistent_failure) faults: FaultController,
    _directory: tempfile::TempDir,
}

fn enter_verifying_home() -> (tempfile::TempDir, FaultController, HomeStore) {
    let directory = tempfile::tempdir().expect("persistent-failure notification home");
    let faults = FaultController::new();
    let mut home = HomeStore::open_with_faults(
        HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .expect("open notification test home");
    let state = BerylState::register(&mut home).expect("register Beryl state");
    let settings = state.settings();
    let update = SettingUpdate::new(
        SettingKey::DraftAutosaveInterval,
        ExpectedSettingRevision::Absent,
        SettingValue::draft_autosave_interval_seconds(1),
    );
    let mut command = HomeCommand::new(home.home_revision().expect("healthy home revision"));
    command
        .add(settings.apply(
            settings.revision(&home).expect("settings revision"),
            ApplySettings::new(vec![update]).expect("one setting update"),
        ))
        .expect("add settings contribution");
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    home.execute(command)
        .expect_err("faulted persistence enters verification");
    assert_eq!(home.health().state(), HomeHealthState::Verifying);
    (directory, faults, home)
}

impl FailedNotificationFixture {
    pub(in crate::cas_projection::persistent_failure) fn new(
        service_generation: ProjectionServiceGeneration,
    ) -> Self {
        let (directory, faults, home) = enter_verifying_home();
        faults.fail_next(FaultPoint::BeforeVerification);
        home.verify_health()
            .expect_err("faulted verification establishes persistent failure");
        let health = home.health();
        assert_eq!(health.state(), HomeHealthState::Failed);
        let home_id = home.home_id();
        let home_generation = health
            .generation()
            .expect("failed home retains its exact generation");
        let home = Arc::new(home);
        let (notification, receiver) = persistent_failure_notification_channel(
            &home,
            home_id,
            home_generation,
            service_generation,
        );
        Self {
            notification,
            receiver,
            home,
            _directory: directory,
        }
    }
}

impl VerifyingNotificationFixture {
    pub(in crate::cas_projection::persistent_failure) fn new(
        service_generation: ProjectionServiceGeneration,
    ) -> Self {
        let (directory, faults, home) = enter_verifying_home();
        let home_id = home.home_id();
        let home_generation = home
            .health()
            .generation()
            .expect("verifying home retains its exact generation");
        let home = Arc::new(home);
        let (notification, receiver) = persistent_failure_notification_channel(
            &home,
            home_id,
            home_generation,
            service_generation,
        );
        Self {
            notification,
            receiver,
            home,
            faults,
            _directory: directory,
        }
    }

    pub(in crate::cas_projection::persistent_failure) fn enter_next_verifying(&self) {
        let state = BerylState::reacquire(&self.home).expect("reacquire Beryl state");
        let settings = state.settings();
        let update = SettingUpdate::new(
            SettingKey::ContextCompactionTimeout,
            ExpectedSettingRevision::Absent,
            SettingValue::context_compaction_timeout_millis(1),
        );
        let mut command =
            HomeCommand::new(self.home.home_revision().expect("healthy home revision"));
        command
            .add(settings.apply(
                settings.revision(&self.home).expect("settings revision"),
                ApplySettings::new(vec![update]).expect("one setting update"),
            ))
            .expect("add settings contribution");
        self.faults.fail_next(FaultPoint::AfterCommitBeforePersist);
        self.home
            .execute(command)
            .expect_err("second faulted persistence enters verification");
        assert_eq!(self.home.health().state(), HomeHealthState::Verifying);
    }
}
