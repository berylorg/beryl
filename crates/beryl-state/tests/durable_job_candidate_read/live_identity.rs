use super::*;
use beryl_state::{
    DurableJobReadError, HandoffFailureEvidence, HandoffFailureKind, HandoffJobIndexFault,
    HandoffJobTransition,
};

fn execute(
    access: &HomeCandidateRecoveryAccess<'_>,
    contribution: beryl_home_store::MutationContribution,
) {
    let mut command = HomeCommand::new(access.home_revision().unwrap());
    command.add(contribution).unwrap();
    assert!(matches!(
        access.execute(command),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
}

#[test]
fn live_pages_reject_wrong_key_and_terminal_copies_in_both_access_modes() {
    let directory = tempfile::tempdir().unwrap();
    let (mut candidate, state) = candidate(&directory, FaultController::new());
    let jobs = state.durable_jobs();
    let access = candidate.recovery_access().unwrap();
    let first_id = admit(&access, &jobs, 1);
    let second_id = admit(&access, &jobs, 2);
    let first = jobs.job_candidate(&access, first_id).unwrap().unwrap();
    let second = jobs.job_candidate(&access, second_id).unwrap().unwrap();
    let limits = CursorReadLimits::new(2, 800_000).unwrap();
    execute(
        &access,
        jobs.corrupt_handoff_job_index_for_test(
            jobs.revision_candidate(&access).unwrap(),
            first.clone(),
            HandoffJobIndexFault::LiveCopy(second.clone()),
        ),
    );
    assert!(matches!(
        jobs.list_live_candidate(&access, None, limits),
        Err(DurableJobReadError::InvalidLiveEntry { .. })
    ));
    execute(
        &access,
        jobs.corrupt_handoff_job_index_for_test(
            jobs.revision_candidate(&access).unwrap(),
            first.clone(),
            HandoffJobIndexFault::LiveCopy(first.clone()),
        ),
    );
    let transition = jobs
        .prepare_handoff_job_transition_candidate(
            &access,
            second_id,
            second.revision(),
            HandoffJobTransition::TerminalFailure(
                HandoffFailureEvidence::new(HandoffFailureKind::InvariantViolation, None).unwrap(),
            ),
        )
        .unwrap();
    execute(&access, transition.contribution());
    let terminal = jobs.job_candidate(&access, second_id).unwrap().unwrap();
    execute(
        &access,
        jobs.corrupt_handoff_job_index_for_test(
            jobs.revision_candidate(&access).unwrap(),
            terminal.clone(),
            HandoffJobIndexFault::LiveCopy(terminal.clone()),
        ),
    );
    assert!(matches!(
        jobs.list_live_candidate(&access, None, limits),
        Err(DurableJobReadError::InvalidLiveEntry { .. })
    ));
    execute(
        &access,
        jobs.corrupt_handoff_job_index_for_test(
            jobs.revision_candidate(&access).unwrap(),
            terminal.clone(),
            HandoffJobIndexFault::MissingLive,
        ),
    );
    let store = candidate.publish().unwrap();
    for (key, copy, restore) in [
        (first.clone(), second, HandoffJobIndexFault::LiveCopy(first)),
        (
            terminal.clone(),
            terminal,
            HandoffJobIndexFault::MissingLive,
        ),
    ] {
        let mut command = HomeCommand::new(store.home_revision().unwrap());
        command
            .add(jobs.corrupt_handoff_job_index_for_test(
                jobs.revision(&store).unwrap(),
                key.clone(),
                HandoffJobIndexFault::LiveCopy(copy),
            ))
            .unwrap();
        assert!(matches!(
            store.execute(command),
            CommandOutcome::Committed {
                later_failure: None,
                ..
            }
        ));
        assert!(matches!(
            jobs.list_live(&store, None, limits),
            Err(DurableJobReadError::InvalidLiveEntry { .. })
        ));
        let mut command = HomeCommand::new(store.home_revision().unwrap());
        command
            .add(jobs.corrupt_handoff_job_index_for_test(
                jobs.revision(&store).unwrap(),
                key,
                restore,
            ))
            .unwrap();
        assert!(matches!(
            store.execute(command),
            CommandOutcome::Committed {
                later_failure: None,
                ..
            }
        ));
    }
    assert_eq!(
        jobs.list_live(&store, None, limits)
            .unwrap()
            .records()
            .len(),
        1
    );
    store.close().unwrap();
}
