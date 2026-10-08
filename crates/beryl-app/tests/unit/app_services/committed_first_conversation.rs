use super::*;
use crate::{
    main_window::*,
    running_owner::{InterruptedExitRecoveryOutcome, RunningProcessOwner},
    runtime_admission::OnboardingFacts,
    startup_owner::{StartedProcess, StartupCommands},
    theme_runtime::GpuiAppearanceWindowSet,
};
use beryl_home_store::HomeCommand;
use beryl_model::{
    AdmittedHostPath, Availability, ExecutionBinding, PathFlavor, RootId, RuntimeId,
    RuntimeLaunchForm, RuntimeMode, RuntimeNativePath, SyndicDraftId, SyndicThreadId, WindowId,
};
use beryl_state::{
    AvailabilitySnapshot, CatalogSourceRevisions, CreateRuntimeWithHomeRoot, PublishCatalogClaim,
    RecordRevision, RememberedTarget, RootRegistration, RuntimeRegistration, UnixMillis,
    WindowClaimReplacementPreparation,
};
use gpui::{AppContext, TestAppContext};
use std::{cell::RefCell, rc::Rc};
use syndic_storage::{CreateThread, DraftEditHistoryPolicyV1};

#[path = "../../pending_composer_activation/support.rs"]
mod widget_support;

#[path = "committed_first_conversation/support.rs"]
mod support;
use support::*;
#[path = "committed_first_conversation/scenario.rs"]
mod scenario;
use scenario::run;

#[gpui::test]
fn ordinary_recovery_mounts_committed_first_conversation_without_a_predecessor(
    cx: &mut TestAppContext,
) {
    run(cx, false, false, false, false, false);
}

#[gpui::test]
fn ordinary_recovery_retains_rejected_widget_release_and_retries_original_first_conversation(
    cx: &mut TestAppContext,
) {
    run(cx, true, false, false, false, false);
}

#[gpui::test]
fn ordinary_recovery_retries_original_admission_after_cached_reconciliation_failure(
    cx: &mut TestAppContext,
) {
    run(cx, false, true, false, false, false);
}

#[gpui::test]
fn ordinary_recovery_waits_for_committed_open_classification_and_blocks_close_exit(
    cx: &mut TestAppContext,
) {
    run(cx, false, false, true, false, false);
}

#[gpui::test]
fn ordinary_recovery_refuses_stale_committed_first_window_without_editor_or_publication(
    cx: &mut TestAppContext,
) {
    run(cx, false, false, false, true, false);
}

#[gpui::test]
fn ordinary_recovery_cancels_before_publication_and_retries_original_first_conversation(
    cx: &mut TestAppContext,
) {
    run(cx, false, false, false, false, true);
}
