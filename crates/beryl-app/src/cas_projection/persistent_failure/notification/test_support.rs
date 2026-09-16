use beryl_home_store::HomeServiceReference;
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{Arc, mpsc},
};

use beryl_home_store::{
    HomeCommand, HomeHealthState, HomeOpenCandidate, HomeOpenOptions, HomeSchemaVersion, HomeStore,
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
    pub(in crate::cas_projection::persistent_failure) home: Arc<HomeServiceReference>,
    _owned_home: HomeStore,
    _directory: tempfile::TempDir,
}

fn open_faulted_home() -> (tempfile::TempDir, HomeStore) {
    let directory = tempfile::tempdir().expect("persistent-failure notification home");
    let faults = FaultController::new();
    let mut home = HomeOpenCandidate::open_with_faults(
        HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .expect("open notification test home");
    let state = BerylState::register(&mut home).expect("register Beryl state");
    let home = home
        .prepare_publication(BerylState::required_domains().unwrap())
        .unwrap()
        .publish()
        .unwrap();
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
    faults.panic_next(FaultPoint::BeforeCommit);
    assert!(
        catch_unwind(AssertUnwindSafe(|| {
            let _ = home.execute(command);
        }))
        .is_err()
    );
    assert_eq!(home.health().state(), HomeHealthState::Failed);
    (directory, home)
}

impl FailedNotificationFixture {
    pub(in crate::cas_projection::persistent_failure) fn new(
        service_generation: ProjectionServiceGeneration,
    ) -> Self {
        let (directory, home) = open_faulted_home();
        let health = home.health();
        assert_eq!(health.state(), HomeHealthState::Failed);
        let home_id = home.home_id();
        let home_generation = health
            .generation()
            .expect("failed home retains its exact generation");
        let owned_home = home;
        let home = Arc::new(owned_home.service_reference());
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
            _owned_home: owned_home,
            _directory: directory,
        }
    }
}
