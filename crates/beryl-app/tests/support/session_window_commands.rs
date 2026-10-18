use beryl_home_store::{CommandOutcome, HomeCommand, HomeStore};
use beryl_state::PreparedWindowMutation;

pub fn execute(home: &HomeStore, prepared: PreparedWindowMutation) -> CommandOutcome {
    let mut command = HomeCommand::new(home.home_revision().unwrap());
    prepared.add_to(&mut command).unwrap();
    home.execute(command)
}

pub fn commit(home: &HomeStore, prepared: PreparedWindowMutation) {
    assert!(matches!(
        execute(home, prepared),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
}
