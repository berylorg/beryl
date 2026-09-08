#![cfg(feature = "test-faults")]

include!("durable_builder/support.rs");

use std::num::NonZeroU64;

use beryl_model::AssetId;
use sha2::{Digest, Sha256};
use syndic_storage::test_faults::{
    DraftMarkerAdmissionFixtureSnapshotV1, draft_marker_admission_fixture_contribution,
    home_store_syndic_point_acquisition_count, reset_home_store_syndic_point_acquisition_count,
};
use syndic_storage::{
    DRAFT_MARKER_ADMISSION_COMMAND_MAX_ENCODED_BYTES, DraftMarkerAdmissionAssignmentGroupV1,
    DraftMarkerAdmissionChildV1, DraftMarkerAdmissionCommandIdV1, DraftMarkerAdmissionEvidenceV1,
    DraftMarkerAdmissionIndexTestErrorV1, DraftMarkerAdmissionIndexTestStateV1,
    DraftMarkerAdmissionNodeIdV1, DraftMarkerAdmissionNodeKeyV1, DraftMarkerAdmissionNodeKindV1,
    DraftMarkerAdmissionNodePayloadV1, DraftMarkerAdmissionNodeV1,
    DraftMarkerAdmissionOperationIdV1, DraftMarkerAdmissionOwnerV1,
    DraftMarkerAdmissionPageIdentityV1, DraftMarkerAdmissionReceiptTransitionV1,
    DraftMarkerAdmissionReplayReceiptV1, DraftMarkerAdmissionRootV1,
    DraftMarkerAdmissionSourceKeyV1, DraftMarkerAdmissionTargetDispositionV1,
    DraftMarkerAdmissionTreeV1, DraftMarkerLabelAssignmentOutcomeV1,
    DraftMarkerLabelReadinessDispositionV1, DraftMarkerLabelReadinessPageRequestV1,
    DraftMarkerReadinessCandidateSourceV1, DraftMarkerReadinessSourceAssociationV1,
    DraftMarkerReadinessSourceSelectorV1, DraftPieceRootBuildIdentityV1, DraftPieceRootReferenceV1,
    DraftPieceSettlementKeyV1,
};

#[path = "draft_marker_readiness_source_proof/support.rs"]
mod readiness_support;

use readiness_support::{association, marked_session, owner};

#[path = "draft_marker_admission_deletion/support.rs"]
mod support;
use support::*;

#[path = "draft_marker_admission_deletion/fixtures.rs"]
mod fixtures;
use fixtures::*;

#[path = "draft_marker_admission_deletion/tree_operations.rs"]
mod tree_operations;

#[path = "draft_marker_admission_deletion/admission.rs"]
mod admission;
