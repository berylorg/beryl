#[path = "phase141_syndic_composer_host/support.rs"]
mod support;

use std::num::NonZeroU64;

use beryl_app::composer_host::{
    ComposerHostActivationOutcome, ComposerHostActivationRequest, ComposerHostError,
    ComposerHostInitialDemand, ComposerHostOpenDisposition, ComposerHostReadTarget,
    ComposerHostRequestId, ComposerHostRequestKey, ComposerHostRequestKind,
    ComposerHostRequestPurpose, ComposerHostResponseValue, ComposerHostRestorationSeed,
    SyndicComposerHost,
};
use beryl_home_store::CommandCancellation;
use syndic_storage::{
    DraftByThreadRecord, DraftEditorCandidateSessionIdV1, DraftPieceMalformedRangeRequestV1,
    DraftPieceMarkerAtV1, DraftPieceMarkerDemandV1, DraftPieceMarkerDirectionV1,
    DraftPieceMarkerEdgeProofRequestV1, DraftPieceMarkerEdgeProofV1, DraftPieceMarkerScopeV1,
    DraftPieceOperationIdV1, DraftPieceRangeSourceErrorV1, DraftPieceTextDemandV1,
    SelectedPathProof, ThreadRecord,
};

use support::{current, fixture, point, populate};

#[cfg(feature = "test-faults")]
use support::{committed, execute, run_transaction, transaction, transaction_for_session};

#[cfg(feature = "test-faults")]
use syndic_storage::{
    DraftEditorCandidateSessionReadOutcomeV1, DraftPieceReplacementV1, DraftPieceV1,
};

#[cfg(feature = "test-faults")]
use syndic_storage::test_faults::{
    DraftPieceDescendantCorruption, DraftPieceDescendantTarget, DraftPieceImmutableDeletion,
    FixtureBatch, FixtureRecord, arm_draft_piece_candidate_read_fault,
    arm_draft_piece_current_read_fault, delete_draft_piece_immutable_record,
    inject_draft_piece_descendant_corruption,
};

#[path = "phase141_syndic_composer_host/activation_replay.rs"]
mod activation_replay;
#[path = "phase141_syndic_composer_host/range_custody.rs"]
mod range_custody;
#[cfg(feature = "test-faults")]
#[path = "phase141_syndic_composer_host/read_drift.rs"]
mod read_drift;
#[path = "phase141_syndic_composer_host/stale_completion.rs"]
mod stale_completion;
#[cfg(feature = "test-faults")]
#[path = "phase141_syndic_composer_host/typed_failures.rs"]
mod typed_failures;

fn activation(
    thread: beryl_model::SyndicThreadId,
    session: u8,
    operation: u8,
    first_demands: Vec<ComposerHostInitialDemand>,
) -> ComposerHostActivationRequest {
    ComposerHostActivationRequest::new(
        thread,
        DraftEditorCandidateSessionIdV1::from_bytes([session; 16]),
        DraftPieceOperationIdV1::from_bytes([operation; 16]),
        NonZeroU64::MIN,
        None,
        first_demands.into_boxed_slice(),
    )
}

fn request_id(value: u64) -> ComposerHostRequestId {
    ComposerHostRequestId::new(NonZeroU64::new(value).unwrap())
}

fn key(binding: beryl_app::composer_host::ComposerHostBinding, id: u64) -> ComposerHostRequestKey {
    ComposerHostRequestKey::new(
        binding,
        request_id(id),
        ComposerHostRequestPurpose::Viewport,
    )
}

fn marker_demand(
    cursor: Option<syndic_storage::DraftCompositeSearchKeyV1>,
    count: usize,
    bytes: usize,
) -> DraftPieceMarkerDemandV1 {
    DraftPieceMarkerDemandV1::new(
        DraftPieceMarkerScopeV1::ExactAnchor(3),
        DraftPieceMarkerDirectionV1::Forward,
        cursor,
        count,
        bytes,
    )
}

fn text_request() -> ComposerHostRequestKind {
    ComposerHostRequestKind::Text {
        target: ComposerHostReadTarget::Candidate,
        demand: DraftPieceTextDemandV1::Forward(0),
        max_bytes: 4,
    }
}

fn run(
    host: &mut SyndicComposerHost,
    store: &beryl_home_store::HomeStore,
    binding: beryl_app::composer_host::ComposerHostBinding,
    id: u64,
    kind: ComposerHostRequestKind,
) -> Result<ComposerHostResponseValue, ComposerHostError> {
    let pending = host.begin_request(key(binding, id), kind)?;
    let execution = host.execute_pending(store, pending);
    host.complete_request(execution)
        .map(|response| response.value().clone())
}
