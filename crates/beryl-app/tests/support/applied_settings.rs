#![allow(dead_code)]

use beryl_app::settings_service::{
    AppliedSettingsReader, SettingsApplyAction, SettingsApplyObservation, SettingsDraft,
    SettingsDraftValue, SettingsProcessOwner, SettingsService,
};
use beryl_home_store::{CommandCancellation, HomeStore};
use beryl_state::SettingsState;

pub struct AppliedSettingsFixture {
    service: SettingsService,
    owner: SettingsProcessOwner,
}

impl AppliedSettingsFixture {
    pub fn new(home: &HomeStore, settings: SettingsState) -> Self {
        let owner = SettingsProcessOwner::new(home.home_id());
        let service =
            SettingsService::prepare(home.service_reference(), settings, owner.clone(), 1).unwrap();
        Self { service, owner }
    }

    pub fn reader(&self) -> AppliedSettingsReader {
        self.service.applied_reader()
    }

    pub fn apply(&self, value: SettingsDraftValue) {
        let operation = self.owner.next_operation_id().unwrap();
        let mut draft = SettingsDraft::new(self.service.snapshot().unwrap(), 1, operation);
        draft.set(value).unwrap();
        if draft.modified().is_empty() {
            return;
        }
        let prepared = self
            .service
            .prepare_apply(&draft, SettingsApplyAction::Apply, operation)
            .unwrap();
        assert!(matches!(
            self.service
                .execute(prepared, &CommandCancellation::new())
                .unwrap(),
            SettingsApplyObservation::Committed { .. }
        ));
    }

    pub fn retire(&self) {
        self.service.retire().unwrap();
    }
}

impl Drop for AppliedSettingsFixture {
    fn drop(&mut self) {
        let _ = self.service.retire();
    }
}
