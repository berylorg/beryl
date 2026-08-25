use super::*;

pub(super) fn execute(home: &HomeStore, contribution: beryl_home_store::MutationContribution) {
    let mut command = HomeCommand::new(home.home_revision().unwrap());
    command.add(contribution).unwrap();
    match home.execute(command) {
        beryl_home_store::CommandOutcome::Committed { later_failure: None, .. } => {}
        outcome @ beryl_home_store::CommandOutcome::Committed { later_failure: Some(_), .. } => panic!("checked-user storage command committed with later failure: {outcome:?}"),
        beryl_home_store::CommandOutcome::NotCommitted { evidence } => panic!("checked-user storage command was not committed: {evidence:?}"),
        outcome @ beryl_home_store::CommandOutcome::Indeterminate { .. } => panic!("checked-user storage command was indeterminate: {outcome:?}"),
    }
}

pub(super) fn selected_path(
    home: &HomeStore,
    storage: SyndicStorage,
    thread_id: SyndicThreadId,
) -> SelectedPathProof {
    let thread = storage
        .thread(home, thread_id, point_limit())
        .unwrap()
        .unwrap();
    SelectedPathProof::new(
        thread.committed_tail(),
        thread.revision(),
        thread.selected_path_digest(),
    )
}

pub(super) fn execution_binding(runtime_id: RuntimeId, seed: u8) -> ExecutionBinding {
    ExecutionBinding::new(
        runtime_id,
        RootId::from_bytes([seed.wrapping_add(6); 16]),
        RuntimeNativePath::from_admitted(RuntimeMode::host(), PathFlavor::Windows, EXECUTION_ROOT)
            .unwrap(),
    )
}

pub(in super::super) fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(POINT_READ_BYTES).unwrap()
}
