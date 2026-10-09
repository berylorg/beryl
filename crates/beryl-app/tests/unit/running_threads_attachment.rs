use super::super::tests::{finish, home_support, install_source, mount, source, support};
use super::*;
use beryl_home_store::{CommandOutcome, HomeCommand, test_faults::FaultPoint};
use beryl_model::{InputGateRevision, SyndicDraftId, SyndicThreadId, SyndicTurnId, WindowId};
use syndic_storage::{
    CreateThread, DraftEditHistoryPolicyV1, InputGateRecord, InputGateState, SyndicPointReadLimit,
    SyndicTimestamp,
    test_faults::{FixtureBatch, FixtureRecord},
};

#[path = "running_threads_attachment/live_session.rs"]
mod live_session;

#[derive(Clone, Copy, PartialEq, Eq)]
enum CompletionMode {
    Success,
    CancelClaim,
    ReconcileClaim,
    CancelSave,
    ReconcileSave,
    RefuseCommittedDisposal,
    LiveCheckedOut,
}

#[track_caller]
fn drive(cx: &mut gpui::TestAppContext, mut ready: impl FnMut(&mut gpui::TestAppContext) -> bool) {
    let deadline = std::time::Instant::now() + Duration::from_secs(8);
    eprintln!("activation milestone {}", std::panic::Location::caller());
    loop {
        cx.executor().advance_clock(Duration::from_millis(10));
        support::draw(cx);
        if ready(cx) {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "activation did not settle within its bounded fixture deadline"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[gpui::test]
fn edited_prior_attaches_unviewed_thread_without_execution_effects(cx: &mut gpui::TestAppContext) {
    activate(cx, 81, CompletionMode::Success);
}

#[gpui::test]
fn cancelled_claim_retires_target_and_preserves_edited_prior(cx: &mut gpui::TestAppContext) {
    activate(cx, 91, CompletionMode::CancelClaim);
}

#[gpui::test]
fn indeterminate_claim_retains_prior_and_lease_until_reconciled(cx: &mut gpui::TestAppContext) {
    activate(cx, 101, CompletionMode::ReconcileClaim);
}

#[gpui::test]
fn cancelled_selection_save_preserves_prior_candidate_and_editor(cx: &mut gpui::TestAppContext) {
    activate(cx, 111, CompletionMode::CancelSave);
}

#[gpui::test]
fn indeterminate_selection_save_settles_before_claim_publication(cx: &mut gpui::TestAppContext) {
    activate(cx, 121, CompletionMode::ReconcileSave);
}

#[gpui::test]
fn committed_disposal_failure_retains_claim_and_cleanup_custody(cx: &mut gpui::TestAppContext) {
    activate(cx, 131, CompletionMode::RefuseCommittedDisposal);
}

#[gpui::test]
fn unviewed_callback_preserves_exact_checked_out_execution_session(cx: &mut gpui::TestAppContext) {
    activate(cx, 141, CompletionMode::LiveCheckedOut);
}

#[path = "running_threads_attachment/fixture.rs"]
mod fixture;
use fixture::activate;

#[path = "running_threads_attachment/ordinary_intake.rs"]
mod ordinary_intake;
