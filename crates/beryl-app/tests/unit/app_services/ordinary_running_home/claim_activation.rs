use super::*;
use crate::main_window::running_threads::activation::{
    RunningThreadActivation, RunningThreadActivationPreparation,
};
use beryl_home_store::{CommandCancellation, CommandOutcome, HomeCommand, test_faults::FaultScope};
use beryl_model::{SyndicDraftId, SyndicThreadId};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use syndic_storage::{
    CreateThread, DraftEditHistoryPolicyV1, SyndicPointReadLimit, SyndicTimestamp,
};

#[path = "claim_activation/collision.rs"]
mod collision;
#[path = "claim_activation/control.rs"]
mod control;
#[path = "claim_activation/durable_evidence.rs"]
mod durable_evidence;
#[path = "claim_activation/fixture.rs"]
mod fixture;
#[path = "claim_activation/lineage_entry.rs"]
mod lineage_entry;
#[path = "claim_activation/lineage_fixture.rs"]
mod lineage_fixture;
#[path = "claim_activation/marker_origin.rs"]
mod marker_origin;
#[path = "claim_activation/navigation_entry.rs"]
mod navigation_entry;
#[path = "claim_activation/outcome_evidence.rs"]
mod outcome_evidence;
#[path = "claim_activation/page_custody.rs"]
mod page_custody;
#[path = "claim_activation/scenario.rs"]
mod scenario;
#[path = "claim_activation/switcher_entry.rs"]
mod switcher_entry;
#[path = "claim_activation/transcript.rs"]
mod transcript;

pub(super) fn fail_confirmed_fixture_read(
    home: &beryl_home_store::HomeStore,
    faults: &FaultController,
) {
    let reached = AtomicBool::new(false);
    outcome_evidence::fail_original_confirmed_read(home, faults, &reached);
    assert!(reached.load(Ordering::Acquire));
}

#[derive(Clone, Copy, Debug)]
enum Cut {
    SaveNoncommit,
    SaveIndeterminate,
    ClaimNoncommit,
    ClaimIndeterminate,
    ClaimCommitted,
    DisposalCommitted,
}

impl Cut {
    fn committed(self) -> bool {
        matches!(
            self,
            Self::ClaimIndeterminate | Self::ClaimCommitted | Self::DisposalCommitted
        )
    }
}

#[derive(Clone, Copy)]
enum SaveExpectation {
    Dirty,
    DirtyMarker,
    DirtyPageSetup,
    SavedNoop,
}

#[test]
fn native_original_ordinary_save_noncommit_preserves_prior_and_unrelated_resident() {
    run(Cut::SaveNoncommit);
}
#[test]
fn native_original_ordinary_indeterminate_save_settles_before_unadmitted_claim() {
    run(Cut::SaveIndeterminate);
}
#[test]
fn native_original_ordinary_claim_noncommit_drains_unpublished_target_before_prior() {
    run(Cut::ClaimNoncommit);
}
#[test]
fn native_original_ordinary_indeterminate_claim_recovers_exact_nonempty_target() {
    run(Cut::ClaimIndeterminate);
}
#[test]
fn native_original_ordinary_committed_claim_recovers_exact_nonempty_target() {
    run(Cut::ClaimCommitted);
}
#[test]
fn native_original_ordinary_completed_disposal_recovers_exact_target_without_second_claim() {
    run(Cut::DisposalCommitted);
}

fn run(cut: Cut) {
    run_control(cut, control::Control::Normal);
}

fn run_control(cut: Cut, control: control::Control) {
    let final_selection = Rc::new(RefCell::new(None));
    let qualified_selection = final_selection.clone();
    run_mounted_with_prepared_home(
        2,
        0,
        fixture::prepare_home,
        final_selection,
        move |owner, faults, cx| {
            Box::pin(async move {
                let (window, window_id, claim) =
                    scenario::recover(owner, faults, cut, control, SaveExpectation::Dirty, cx)
                        .await;
                *qualified_selection.borrow_mut() = Some((window_id, claim.thread_id()));
                activate_exit(window, cx);
            })
        },
        true,
        2,
    );
}
