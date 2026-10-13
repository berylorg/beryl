#![cfg(feature = "test-faults")]

#[path = "replacement_drafts/admission_edges.rs"]
mod admission_edges;
#[path = "replacement_drafts/assets.rs"]
mod assets;
#[path = "replacement_drafts/budget.rs"]
mod budget;
#[path = "replacement_drafts/budget_checkpoint_edges.rs"]
mod budget_checkpoint_edges;
#[path = "replacement_drafts/codec_cases.rs"]
mod codec_cases;
#[path = "replacement_drafts/faults.rs"]
mod faults;
#[path = "replacement_drafts/fixture.rs"]
mod fixture;
#[path = "replacement_drafts/flow.rs"]
mod flow;
#[path = "replacement_drafts/gates.rs"]
mod gates;
#[path = "replacement_drafts/history.rs"]
mod history;
#[path = "replacement_drafts/images.rs"]
mod images;
#[path = "replacement_drafts/lifecycle.rs"]
mod lifecycle;
#[path = "replacement_drafts/mixed_heads.rs"]
mod mixed_heads;
#[path = "draft_edit_history/support.rs"]
mod ordinary;
#[path = "replacement_drafts/proofs.rs"]
mod proofs;
#[path = "replacement_drafts/scope.rs"]
mod scope;
mod support;
#[path = "replacement_drafts/tree_validation.rs"]
mod tree_validation;

use beryl_home_store::test_faults::{FaultController, FaultPoint};
use beryl_home_store::{
    CommandOutcome, HomeCommand, HomeOpenOptions, HomeSchemaVersion, HomeStore,
    MutationContribution,
};
use beryl_model::{
    AssetId, AssetReferenceSetId, ImageLabelOrdinal, OrderedMarkerAssetSummaryV1,
    SealedAssetReferenceSetProof, SequentialMarkerSummaryV1, SyndicDraftMarkerId, SyndicItemId,
    SyndicTurnId,
};
use beryl_state::{AssetOwner, AssetOwnerHeadUpdate, BerylState, UpdateAssetOwnerHeads};
use fixture::*;
use flow::*;
use syndic_storage::test_faults::{
    FixtureRecord, fixture_advance_item_projection_digest, fixture_advance_transcript_digest,
    fixture_inline_paragraph_projection, fixture_item_projection_digest_seed,
    fixture_transcript_digest_seed,
};
use syndic_storage::*;

fn execute(store: &HomeStore, p: MutationContribution) -> CommandOutcome {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(p).unwrap();
    store.execute(command)
}
fn committed(outcome: CommandOutcome) {
    assert!(
        matches!(
            outcome,
            CommandOutcome::Committed {
                later_failure: None,
                ..
            }
        ),
        "{outcome:?}"
    );
}
fn point(offset: u64) -> DraftCompositePositionV1 {
    DraftCompositePositionV1::new(offset, DraftCompositeGapWitnessV1::Unambiguous)
}
fn operation(byte: u8) -> DraftPieceOperationIdV1 {
    DraftPieceOperationIdV1::from_bytes([byte; 16])
}
