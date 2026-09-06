use beryl_home_store::{CommandOutcome, HomeCommand, HomeStore, MutationContribution};

fn execute_contribution(store: &HomeStore, contribution: MutationContribution, prefix: &str) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    match store.execute(command) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        CommandOutcome::NotCommitted { evidence } => {
            panic!("{prefix} unexpectedly not committed: {evidence:?}")
        }
        outcome @ CommandOutcome::Committed {
            later_failure: Some(_),
            ..
        } => panic!("{prefix} committed with later failure: {outcome:?}"),
        outcome @ CommandOutcome::Indeterminate { .. } => {
            panic!("{prefix} indeterminate: {outcome:?}")
        }
    }
}

pub(super) fn execute_exact_contribution(store: &HomeStore, contribution: MutationContribution) {
    execute_contribution(store, contribution, "exact Syndic contribution")
}

pub(super) fn execute_fixture_contribution(store: &HomeStore, contribution: MutationContribution) {
    execute_contribution(store, contribution, "Syndic fixture contribution")
}
