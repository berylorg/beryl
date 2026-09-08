#![cfg(feature = "test-faults")]

include!("durable_builder/support.rs");

use std::num::NonZeroU64;

use beryl_model::{
    AssetId, AssetReferenceSetId, ContentRevision, DraftRevision, InputGateRevision,
    OrderedMarkerAssetSummaryV1, SealedAssetReferenceSetProof, SequentialMarkerSummaryV1,
    ThreadRevision, advance_ordered_marker_asset_digest, advance_sequential_marker_digest,
    ordered_marker_asset_digest_seed, sequential_marker_digest_seed,
};
use beryl_state::{
    AppendAssetReferencePage, AssetMediaType, AssetReferencePageEntry,
    AssetReferenceSetStagingAuthority, BeginAssetReferenceSet, BerylState, PublishAssetMetadata,
    SealAssetReferenceSet,
};
use sha2::{Digest, Sha256};
use syndic_storage::test_faults::{FixtureBatch, FixtureRecord};
use syndic_storage::{
    AcceptedInputAdmissionProof, AcceptedInputOrdinal, AcceptedInputRecord,
    AcceptedRouteGeneration, ComposerAtom, ComposerPayload, DraftImageLabelProtectionHeadV1,
    DraftMarkerAdmissionCommandIdV1, DraftMarkerAdmissionOperationIdV1,
    DraftMarkerAdmissionOwnerV1, DraftMarkerLabelReadinessProofV1,
    DraftMarkerReadinessAcceptedSourceV1, DraftMarkerReadinessSourceAssociationV1,
    DraftMarkerReadinessSourceSelectorV1, DraftMarkerReadinessWitnessFactoryV1,
    DraftPieceErrorReasonV1, DraftPieceReconciledCommandV1, DraftPieceTextDemandV1,
    DraftPieceTransactionOutcomeV1, ImageLabelAuthorityHeadV1, ImageLabelFrontier,
    ImageLabelOriginOwner, ImageLabelOriginSpanRecord, PreparedContent, SelectedPathProof,
    ThreadLineageDepth, ThreadLineageProof, ThreadRecord, child_thread_lineage_digest,
    empty_selected_path_digest,
};
use syndic_storage::{
    DraftMarkerLabelAssignmentOutcomeV1, DraftMarkerLabelReadinessDispositionV1 as Disposition,
    DraftMarkerLabelReadinessPageRequestV1, DraftMarkerLabelReadinessPageSubmissionOutcomeV1,
};

#[path = "draft_marker_readiness_accepted_proof/support.rs"]
mod accepted_support;
#[path = "staged_build_outcomes/admitted.rs"]
mod admitted;
#[path = "staged_build_outcomes/faults.rs"]
mod faults;
#[path = "draft_marker_fresh_readiness/support.rs"]
mod fresh_support;
#[path = "staged_build_outcomes/integrity.rs"]
mod integrity;
#[path = "staged_build_outcomes/outcomes.rs"]
mod outcomes;
#[path = "support/mod.rs"]
mod support;

use accepted_support::*;
use fresh_support::*;
