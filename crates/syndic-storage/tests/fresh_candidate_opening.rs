#![cfg(feature = "test-faults")]

#[path = "abandon_fresh_candidate/shared.rs"]
#[allow(dead_code)]
mod shared;
#[path = "draft_edit_history/support.rs"]
#[allow(dead_code, unused_imports)]
mod support;

use beryl_home_store::{CommandCancellation, HomeCandidateRecoveryAccess};
use shared::abandon_request;
use support::*;
use syndic_storage::DraftEditorCandidateSessionAbandonFreshOutcomeV1;

fn fail_home(store: &HomeStore, faults: &FaultController) {
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(store.home_revision().is_err());
}

fn candidate_execute(
    access: &HomeCandidateRecoveryAccess<'_>,
    contribution: MutationContribution,
) -> CommandOutcome {
    let mut command = HomeCommand::new(access.home_revision().unwrap());
    command.add(contribution).unwrap();
    access.execute(command)
}

fn marker_demand(objects: usize, bytes: usize) -> DraftPieceMarkerDemandV1 {
    DraftPieceMarkerDemandV1::new(
        DraftPieceMarkerScopeV1::InclusiveRange { start: 0, end: 0 },
        DraftPieceMarkerDirectionV1::Forward,
        None,
        objects,
        bytes,
    )
}

#[path = "fresh_candidate_opening/cancellation.rs"]
mod cancellation;
#[path = "fresh_candidate_opening/classification.rs"]
mod classification;
#[path = "fresh_candidate_opening/construction.rs"]
mod construction;
#[path = "fresh_candidate_opening/content.rs"]
mod content;
#[path = "fresh_candidate_opening/outcomes.rs"]
mod outcomes;
#[path = "fresh_candidate_opening/rejections.rs"]
mod rejections;
