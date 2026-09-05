#![allow(dead_code, unused_imports)]

pub use beryl_app::LifecycleYieldOutcome;

#[path = "../src/shell/execution_detail.rs"]
mod execution_detail;
#[path = "../src/shell/lifecycle_continuation.rs"]
mod lifecycle_continuation;
#[path = "../src/shell/lifecycle_yield.rs"]
mod lifecycle_yield;
#[path = "../src/shell/notifications.rs"]
mod notifications;
#[path = "../src/shell/pending_turn_input.rs"]
mod pending_turn_input;

use beryl_backend::{TurnStartOptions, TurnStatus};
use beryl_model::workspace::WorkspaceId;
use execution_detail::UserInputFragment;
use lifecycle_continuation::{
    PHASE_CONTINUE_RESUME_TEXT, context_compaction_queue_failure_message,
    pending_turn_queue_should_wait_for_compaction, phase_continue_new_thread_handoff,
    phase_continue_request, take_phase_continue_new_thread_handoff_for_finished_worker,
};
use lifecycle_yield::LifecycleYieldState;
use pending_turn_input::PendingTurnInputQueue;

#[test]
fn phase_continue_builds_fixed_resume_request() {
    let lifecycle_yield = terminal_lifecycle_yield(LifecycleYieldOutcome::PhaseContinue);

    let request =
        phase_continue_request(&lifecycle_yield).expect("phase_continue should request resume");

    assert_eq!(request.thread_id(), "thread_1");
    assert_eq!(request.resume_fragment().text, PHASE_CONTINUE_RESUME_TEXT);
}

#[test]
fn non_continue_yields_do_not_request_auto_resume() {
    for outcome in [
        LifecycleYieldOutcome::PhaseNeedsReview,
        LifecycleYieldOutcome::BlockedNeedsOperator,
        LifecycleYieldOutcome::PhaseContinueNewThread,
        LifecycleYieldOutcome::PlanComplete,
    ] {
        let lifecycle_yield = terminal_lifecycle_yield(outcome);
        assert_eq!(phase_continue_request(&lifecycle_yield), None);
    }
}

#[test]
fn phase_continue_new_thread_stages_exact_completed_source_handoff() {
    let lifecycle_yield = terminal_lifecycle_yield(LifecycleYieldOutcome::PhaseContinueNewThread);

    let handoff = phase_continue_new_thread_handoff(&lifecycle_yield, TurnStatus::Completed)
        .expect("completed phase continuation should stage a handoff");

    assert_eq!(handoff.source_thread_id(), "thread_1");
    assert_eq!(handoff.source_turn_id(), "turn_1");
    assert_eq!(handoff.resume_fragment().text, PHASE_CONTINUE_RESUME_TEXT);
}

#[test]
fn phase_continue_new_thread_does_not_stage_failed_or_interrupted_source() {
    for status in [TurnStatus::Failed, TurnStatus::Interrupted] {
        let lifecycle_yield =
            terminal_lifecycle_yield(LifecycleYieldOutcome::PhaseContinueNewThread);

        assert_eq!(
            phase_continue_new_thread_handoff(&lifecycle_yield, status),
            None
        );
    }
}

#[test]
fn phase_continue_new_thread_handoff_requires_matching_finished_worker_source() {
    let lifecycle_yield = terminal_lifecycle_yield(LifecycleYieldOutcome::PhaseContinueNewThread);
    let handoff = phase_continue_new_thread_handoff(&lifecycle_yield, TurnStatus::Completed);
    let mut pending = handoff;

    assert_eq!(
        take_phase_continue_new_thread_handoff_for_finished_worker(
            &mut pending,
            Some("other_thread"),
        ),
        None
    );
    assert_eq!(pending, None);

    let lifecycle_yield = terminal_lifecycle_yield(LifecycleYieldOutcome::PhaseContinueNewThread);
    let mut pending = phase_continue_new_thread_handoff(&lifecycle_yield, TurnStatus::Completed);
    let handoff =
        take_phase_continue_new_thread_handoff_for_finished_worker(&mut pending, Some("thread_1"))
            .expect("matching finished worker should consume the handoff");

    assert_eq!(handoff.source_thread_id(), "thread_1");
    assert_eq!(handoff.source_turn_id(), "turn_1");
    assert_eq!(pending, None);
}

#[test]
fn phase_continue_new_thread_handoff_clears_when_worker_fails() {
    let lifecycle_yield = terminal_lifecycle_yield(LifecycleYieldOutcome::PhaseContinueNewThread);
    let mut pending = phase_continue_new_thread_handoff(&lifecycle_yield, TurnStatus::Completed);

    assert_eq!(
        take_phase_continue_new_thread_handoff_for_finished_worker(&mut pending, None),
        None
    );
    assert_eq!(pending, None);
}

#[test]
fn generated_resume_precedes_composer_fragments_accepted_during_compaction() {
    let request = phase_continue_request(&terminal_lifecycle_yield(
        LifecycleYieldOutcome::PhaseContinue,
    ))
    .unwrap();
    let mut queue = PendingTurnInputQueue::new(
        "thread_1".to_string(),
        WorkspaceId::host_windows("C:\\work\\beryl"),
        false,
        TurnStartOptions::default(),
        9,
        request.resume_fragment(),
    );

    queue.append(UserInputFragment::text(
        "Operator follow-up while compacting",
    ));

    assert_eq!(
        fragment_texts(&queue.into_fragments()),
        vec![
            PHASE_CONTINUE_RESUME_TEXT.to_string(),
            "Operator follow-up while compacting".to_string(),
        ]
    );
}

#[test]
fn compaction_success_releases_resume_queue_for_turn_start() {
    let queue = PendingTurnInputQueue::new(
        "thread_1".to_string(),
        WorkspaceId::host_windows("C:\\work\\beryl"),
        false,
        TurnStartOptions::default(),
        9,
        UserInputFragment::text(PHASE_CONTINUE_RESUME_TEXT),
    );

    assert!(pending_turn_queue_should_wait_for_compaction(
        Some("thread_1"),
        "thread_1"
    ));
    assert!(!pending_turn_queue_should_wait_for_compaction(
        None, "thread_1"
    ));
    assert_eq!(
        fragment_texts(&queue.into_fragments()),
        vec![PHASE_CONTINUE_RESUME_TEXT.to_string()]
    );
}

#[test]
fn compaction_failure_reports_queue_failure_without_resume_start() {
    assert_eq!(
        context_compaction_queue_failure_message("backend rejected compact"),
        "Beryl could not send the queued input because context compaction failed: backend rejected compact"
    );
    assert!(pending_turn_queue_should_wait_for_compaction(
        Some("thread_1"),
        "thread_1"
    ));
}

fn terminal_lifecycle_yield(
    outcome: LifecycleYieldOutcome,
) -> lifecycle_yield::TerminalLifecycleYield {
    let mut state = LifecycleYieldState::default();
    assert!(state.record("thread_1", "turn_1", outcome));
    state
        .apply_terminal_turn("thread_1", "turn_1")
        .expect("terminal turn should consume lifecycle yield")
}

#[test]
fn interrupted_queue_removes_only_generated_identity_and_preserves_human_order() {
    let human_same_text = UserInputFragment::text(PHASE_CONTINUE_RESUME_TEXT);
    let generated = UserInputFragment::text(PHASE_CONTINUE_RESUME_TEXT);
    let mut queue = PendingTurnInputQueue::new(
        "thread_1".into(),
        WorkspaceId::host_windows("C:\\work"),
        false,
        TurnStartOptions::default(),
        3,
        human_same_text.clone(),
    );
    queue.append(generated.clone());
    queue.mark_generated_lifecycle_fragment(generated.id);
    queue.append(UserInputFragment::text("human follow-up"));
    assert_eq!(queue.hold_after_compaction(), Some(generated));
    assert!(queue.is_held());
    assert_eq!(queue.fragments()[0], human_same_text);
    assert_eq!(queue.fragments()[1].text, "human follow-up");
    assert!(queue.hold_after_compaction().is_none());
    assert_eq!(queue.fragment_count(), 2);
}

#[test]
fn explicit_submission_joins_held_input_until_idle_authorization() {
    let mut queue = PendingTurnInputQueue::new(
        "thread_1".into(),
        WorkspaceId::host_windows("C:\\work"),
        false,
        TurnStartOptions::default(),
        3,
        UserInputFragment::text("first human input"),
    );
    queue.hold_after_compaction();
    queue.append(UserInputFragment::text("explicit retry"));
    assert!(queue.is_held());
    queue.bind_compaction_workspace("workspace");
    assert!(queue.authorize_after_fresh_idle(
        "workspace",
        "thread_1",
        &WorkspaceId::host_windows("C:\\work"),
        Some(&beryl_backend::ThreadStatus::Idle)
    ));
    assert!(!queue.is_held());
    assert_eq!(
        fragment_texts(&queue.into_fragments()),
        vec!["first human input", "explicit retry"]
    );
}

#[test]
fn reopened_queue_rebases_presentation_without_changing_fragment_identity_or_order() {
    let mut queue = PendingTurnInputQueue::new(
        "thread_1".into(),
        WorkspaceId::host_windows("C:\\work"),
        false,
        TurnStartOptions::default(),
        3,
        UserInputFragment::text("first"),
    );
    queue.append(UserInputFragment::text("second"));
    let fragments = queue.fragments().to_vec();
    queue.hold_after_compaction();
    queue.bind_compaction_workspace("workspace");
    assert!(queue.rebase_after_reopen(
        "workspace",
        "thread_1",
        &WorkspaceId::host_windows("C:\\work"),
        12
    ));
    assert_eq!(queue.turn_index(), 12);
    assert_eq!(queue.fragments(), fragments);
    assert!(queue.is_held());
}

#[test]
fn generated_only_interrupted_queue_accepts_next_human_fragment_without_auto_resume() {
    let generated = UserInputFragment::text(PHASE_CONTINUE_RESUME_TEXT);
    let mut queue = PendingTurnInputQueue::new(
        "thread_1".into(),
        WorkspaceId::host_windows("C:\\work"),
        false,
        TurnStartOptions::default(),
        3,
        generated.clone(),
    );
    queue.mark_generated_lifecycle_fragment(generated.id);
    queue.hold_after_compaction();
    assert_eq!(queue.fragment_count(), 0);
    queue.append(UserInputFragment::text("resume now"));
    assert!(queue.is_held());
    assert_eq!(fragment_texts(&queue.into_fragments()), vec!["resume now"]);
}

fn fragment_texts(fragments: &[UserInputFragment]) -> Vec<String> {
    fragments
        .iter()
        .map(|fragment| fragment.text.clone())
        .collect()
}

#[test]
fn live_delivery_decision_holds_interrupted_input_through_active_unavailable_and_stale_idle_then_releases_once()
 {
    let target = WorkspaceId::host_windows("C:\\work");
    let generated = UserInputFragment::text(PHASE_CONTINUE_RESUME_TEXT);
    let mut queue = PendingTurnInputQueue::new(
        "thread_1".into(),
        target.clone(),
        false,
        TurnStartOptions::default(),
        3,
        generated.clone(),
    );
    assert!(queue.bind_compaction_workspace("workspace"));
    queue.mark_generated_lifecycle_fragment(generated.id);
    queue.append(UserInputFragment::text("accepted while compacting"));
    assert_eq!(queue.hold_after_compaction(), Some(generated));
    queue.append(UserInputFragment::text("explicit resume"));
    let held = queue.clone();
    for status in [
        None,
        Some(beryl_backend::ThreadStatus::Active {
            active_flags: vec![],
        }),
    ] {
        assert!(!queue.authorize_after_fresh_idle(
            "workspace",
            "thread_1",
            &target,
            status.as_ref()
        ));
        assert_eq!(queue, held);
    }
    for (workspace, thread, execution) in [
        ("other", "thread_1", target.clone()),
        ("workspace", "other", target.clone()),
        (
            "workspace",
            "thread_1",
            WorkspaceId::host_windows("C:\\other"),
        ),
    ] {
        assert!(!queue.authorize_after_fresh_idle(
            workspace,
            thread,
            &execution,
            Some(&beryl_backend::ThreadStatus::Idle)
        ));
        assert_eq!(queue, held);
    }
    assert!(queue.authorize_after_fresh_idle(
        "workspace",
        "thread_1",
        &target,
        Some(&beryl_backend::ThreadStatus::Idle)
    ));
    assert!(!queue.authorize_after_fresh_idle(
        "workspace",
        "thread_1",
        &target,
        Some(&beryl_backend::ThreadStatus::Idle)
    ));
    assert_eq!(
        fragment_texts(&queue.into_fragments()),
        vec!["accepted while compacting", "explicit resume"]
    );
}

#[test]
fn live_reopen_decision_rejects_other_workspaces_threads_and_targets_without_mutating_queue() {
    let target = WorkspaceId::host_windows("C:\\work");
    let mut queue = PendingTurnInputQueue::new(
        "thread_1".into(),
        target.clone(),
        false,
        TurnStartOptions::default(),
        3,
        UserInputFragment::text("accepted human input"),
    );
    assert!(!queue.is_compaction_queue());
    assert!(!queue.rebase_after_reopen("workspace", "thread_1", &target, 20));
    assert!(queue.bind_compaction_workspace("workspace"));
    let original = queue.clone();
    for (workspace, thread, execution) in [
        ("other", "thread_1", target.clone()),
        ("workspace", "other", target.clone()),
        (
            "workspace",
            "thread_1",
            WorkspaceId::host_windows("C:\\other"),
        ),
    ] {
        assert!(!queue.rebase_after_reopen(workspace, thread, &execution, 20));
        assert_eq!(queue, original);
    }
    assert!(!queue.bind_compaction_workspace("other"));
    assert!(queue.rebase_after_reopen("workspace", "thread_1", &target, 20));
    assert_eq!(queue.turn_index(), 20);
    assert!(queue.is_held());
    assert_eq!(queue.fragments(), original.fragments());
}
