use crate::syndic::Fixture;
use beryl_home_store::{CommandOutcome, HomeCommand};
use beryl_state::{
    ApplySettings, ExpectedSettingRevision, SettingKey, SettingUpdate, SettingValue,
};

#[path = "../support/ordinary_compaction.rs"]
mod ordinary;
pub use ordinary::{execute, obtain};

pub fn apply_timeout(fixture: &Fixture, millis: u64) {
    let home = fixture.home();
    let settings = fixture.state.settings();
    let prior = settings
        .setting(&home, SettingKey::ContextCompactionTimeout)
        .unwrap();
    let expected = prior.map_or(ExpectedSettingRevision::Absent, |record| {
        ExpectedSettingRevision::Exact(record.revision())
    });
    let update = SettingUpdate::new(
        SettingKey::ContextCompactionTimeout,
        expected,
        SettingValue::context_compaction_timeout_millis(millis),
    );
    let mut command = HomeCommand::new(home.home_revision().unwrap());
    command
        .add(settings.apply(
            settings.revision(&home).unwrap(),
            ApplySettings::new(vec![update]).unwrap(),
        ))
        .unwrap();
    assert!(matches!(
        home.execute(command),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
}
