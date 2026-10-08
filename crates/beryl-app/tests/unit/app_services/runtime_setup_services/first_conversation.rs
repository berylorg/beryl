use super::super::*;
use crate::{
    main_window::*, running_owner::RunningProcessOwner, runtime_admission::OnboardingFacts,
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

#[path = "../committed_first_conversation/support.rs"]
mod support;
#[path = "../../../pending_composer_activation/support.rs"]
mod widget_support;

#[test]
fn terminal_first_admission_reconciliation_transfers_original_custody_after_home_failure() {
    let (directory, mut process, prepared, appearance, faults) = support::prepared_process();
    process
        .graph_mut()
        .unwrap()
        .handoff
        .as_mut()
        .unwrap()
        .shutdown()
        .unwrap();
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let (_, _, original_window) = support::commit_onboarding(&process, None);
    let graph = process.graph().unwrap();
    let home = graph.home();
    let state = graph.state();
    let generation = home.health().generation().unwrap();
    let original = home.pending_reconciliations();
    assert_eq!(original.len(), 1);
    let bootstrap = state.session().minimal_bootstrap(home).unwrap().unwrap();
    let mut command = HomeCommand::new(home.home_revision().unwrap());
    command
        .add(state.session().update_placement(
            state.session().revision(home).unwrap(),
            beryl_state::UpdateWindowPlacement::new(
                bootstrap.header().revision(),
                original_window.window_id(),
                original_window.revision(),
                beryl_model::WindowPlacement::new(
                    beryl_model::WindowBounds::new(20, 20, 840, 620).unwrap(),
                    beryl_model::WindowDisplayState::Normal,
                    None,
                    None,
                ),
            ),
        ))
        .unwrap();
    assert!(matches!(
        home.execute(command),
        beryl_home_store::CommandOutcome::Committed { .. }
    ));
    let flight = graph.runtime_setup().test_first_flight();
    flight.start_reconciliation().unwrap();
    super::await_flight(&flight);
    let outcome = flight.take_reconciliation_outcome().unwrap();
    assert!(matches!(
        outcome,
        crate::runtime_admission::AdmissionReconciliationOutcome::Unavailable { .. }
    ));
    flight.retain_reconciliation_outcome(outcome);
    let pending = home.pending_reconciliations();
    assert_eq!(pending.len(), original.len());
    recovery_support::fail(&process, &faults);
    let mut capture = graph
        .capture_first_conversation_recovery()
        .unwrap()
        .unwrap();
    assert_eq!(capture.facts().window(), &original_window);
    assert!(!capture.committed());
    assert!(flight.take_reconciliation_outcome().is_none());
    let mut disposal = prepared.dispose();
    let mut disposed = false;
    for _ in 0..16 {
        match disposal.advance() {
            MainWindowRestoreSetOutcome::Pending(next) => disposal = next,
            MainWindowRestoreSetOutcome::Failed { .. } => {
                disposed = true;
                break;
            }
            _ => panic!("original threadless preparation did not dispose"),
        }
    }
    assert!(
        disposed,
        "original threadless preparation retained disposal"
    );
    drop((appearance, flight));
    process.process.fence().unwrap();
    process.retire_failed_service_graph(generation).unwrap();
    let mut candidate = process.recover_retired_service_home(generation).unwrap();
    assert!(
        capture
            .settle(&candidate.recovery_access().unwrap())
            .unwrap_err()
            .contains("terminal unavailable")
    );
    drop(capture);
    candidate.abort().close().unwrap();
    drop(process);
    assert_reopens(&directory);
}
