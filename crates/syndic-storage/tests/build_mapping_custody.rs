#![cfg(feature = "test-faults")]

include!("durable_builder/support.rs");

use syndic_storage::test_faults::{
    DraftBuildMappingRootForTest, DraftBuildMappingSnapshotForTest,
    delete_draft_build_mapping_record_for_test, draft_build_mapping_record_for_test,
    draft_build_mapping_root_key_for_test, draft_build_mapping_snapshot,
    staged_outcome_build_for_test, substitute_draft_build_mapping_record_for_test,
};
use syndic_storage::{
    DraftPieceBuildRecordV1, DraftPieceReconciledCommandV1, DraftPieceTransactionOutcomeV1,
    StagedDraftPieceCommandCompletionV1, StagedDraftPieceDurableClassificationV1 as Durable,
    StagedDraftPieceOutcomeFlightV1, StagedDraftPieceOutcomeStateV1 as State,
    StagedDraftPieceTerminalElectionV1 as Election,
};

#[path = "build_mapping_custody/support.rs"]
mod mapping_support;
#[path = "build_mapping_custody/outcomes.rs"]
mod outcomes;
#[path = "build_mapping_custody/records.rs"]
mod records;

use mapping_support::*;
