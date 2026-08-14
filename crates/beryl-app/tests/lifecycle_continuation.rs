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

use beryl_backend::TurnStartOptions;
use beryl_model::workspace::WorkspaceId;
use execution_detail::UserInputFragment;
use lifecycle_continuation::{
    PHASE_CONTINUE_RESUME_TEXT, context_compaction_queue_failure_message,
    pending_turn_queue_should_wait_for_compaction, phase_continue_request,
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
        LifecycleYieldOutcome::PlanComplete,
    ] {
        let lifecycle_yield = terminal_lifecycle_yield(outcome);
        assert_eq!(phase_continue_request(&lifecycle_yield), None);
    }
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

fn fragment_texts(fragments: &[UserInputFragment]) -> Vec<String> {
    fragments
        .iter()
        .map(|fragment| fragment.text.clone())
        .collect()
}
