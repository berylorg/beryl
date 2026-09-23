use super::*;

#[test]
fn pending_child_input_retains_attempt_identity_and_allows_a_later_resolution() {
    let directory = tempdir().unwrap();
    let (store, state) = open(directory.path());
    let first = admission(71, 1, 72, "child-input");
    let job_id = first.job_id();
    let request = first.request().clone();
    let discussion = first.discussion_thread_id();
    admit(&store, &state, first);
    let before = job(&store, &state, job_id);
    let evidence = HandoffFailureEvidence::new(
        HandoffFailureKind::ChildInputPending,
        Some("Accepted child input remains after settlement".into()),
    )
    .unwrap();
    assert!(matches!(
        execute(
            &store,
            state.durable_jobs().record_terminal_failure(
                state.durable_jobs().revision(&store).unwrap(),
                RecordTerminalHandoffFailure::new(job_id, before.revision(), evidence.clone()),
            )
        ),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    let failed = job(&store, &state, job_id);
    assert_eq!(failed.request(), before.request());
    assert_eq!(failed.resolution(), before.resolution());
    assert!(
        matches!(failed.state(), BranchHandoffJobState::TerminalFailed {
        stopped_at: BranchHandoffCheckpoint::WaitingResolvingTurn, evidence: actual,
    } if actual == &evidence)
    );
    assert!(
        state
            .durable_jobs()
            .list_live(&store, None, CursorReadLimits::new(8, 1024 * 1024).unwrap(),)
            .unwrap()
            .records()
            .is_empty()
    );
    assert_eq!(
        state
            .durable_jobs()
            .request_admission(&store, &request)
            .unwrap()
            .unwrap()
            .job_id(),
        job_id
    );
    assert_eq!(
        state
            .durable_jobs()
            .latest_attempt(&store, discussion)
            .unwrap()
            .unwrap()
            .job_id(),
        job_id
    );
    assert!(matches!(
        execute(
            &store,
            state.durable_jobs().retry_branch_handoff(
                state.durable_jobs().revision(&store).unwrap(),
                RetryBranchHandoff::new(job_id, failed.revision()),
            )
        ),
        CommandOutcome::NotCommitted { .. }
    ));
    store.close().unwrap();

    let (store, state) = open(directory.path());
    assert_eq!(job(&store, &state, job_id), failed);
    let next = admission(73, 2, 72, "child-input-later-resolution");
    let next_id = next.job_id();
    admit(&store, &state, next);
    assert_eq!(
        state
            .durable_jobs()
            .latest_attempt(&store, discussion)
            .unwrap()
            .unwrap()
            .job_id(),
        next_id
    );
    assert_eq!(job(&store, &state, job_id), failed);
    assert_eq!(
        state
            .durable_jobs()
            .request_admission(&store, &request)
            .unwrap()
            .unwrap()
            .job_id(),
        job_id
    );
    store.close().unwrap();
}
