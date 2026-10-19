use super::super::*;
use crate::{ContentEncoding, ContentReference};
use beryl_model::SealedAssetReferenceSetProof;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DraftComposerRecallBasisV1 {
    pub(crate) key: DraftEditorCandidateSessionRecordKeyV1,
    pub(crate) acceptance_digest: [u8; 32],
    pub(crate) thread: beryl_model::SyndicThreadId,
    pub(crate) content: ContentReference,
    pub(crate) assets: Option<SealedAssetReferenceSetProof>,
}

impl DraftComposerRecallBasisV1 {
    pub(crate) fn operation_id(&self) -> Option<DraftPieceOperationIdV1> {
        match self.key {
            DraftEditorCandidateSessionRecordKeyV1::DisposalReceipt { operation_id, .. } => {
                Some(operation_id)
            }
            _ => None,
        }
    }
}

pub type DraftComposerRecallFrontierV1 = DraftSealedComposerFrontierV1<DraftComposerRecallBasisV1>;

impl super::super::import::model::SealedComposerBasisV1 for DraftComposerRecallBasisV1 {
    fn content(&self) -> ContentReference {
        self.content
    }
    fn assets(&self) -> Option<SealedAssetReferenceSetProof> {
        self.assets
    }
    fn is_locally_exact(&self) -> bool {
        let content = self.content();
        let summary = content.summary();
        matches!(
            self.key,
            DraftEditorCandidateSessionRecordKeyV1::DisposalReceipt { .. }
        ) && content.encoding() == ContentEncoding::ComposerV1
            && summary.atom_count() != 0
            && match self.assets() {
                Some(proof) => {
                    summary.image_marker_count() != 0
                        && proof.sequential().marker_count() == summary.image_marker_count()
                        && proof.sequential().marker_digest() == summary.marker_digest()
                        && proof.sequential().maximum_image_label() == summary.maximum_image_label()
                        && proof.ordered_assets().marker_count() == summary.image_marker_count()
                }
                None => summary.image_marker_count() == 0,
            }
    }
    fn wrap(frontier: DraftComposerRecallFrontierV1) -> DraftPieceBuildFrontierV1 {
        DraftPieceBuildFrontierV1::OrdinaryRecall(frontier)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DraftComposerBrowseCheckpointV1 {
    pub(crate) home: beryl_model::BerylHomeId,
    pub(crate) activation: DraftEditorCandidateActivationBindingV1,
    pub(crate) caret: DraftCompositePositionV1,
    pub(crate) selection: DraftCompositePositionV1,
}
impl DraftComposerBrowseCheckpointV1 {
    pub fn candidate(&self) -> DraftEditorCandidateActivationBindingV1 {
        self.activation
    }
    pub fn caret(&self) -> DraftCompositePositionV1 {
        self.caret
    }
    pub fn selection(&self) -> DraftCompositePositionV1 {
        self.selection
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DraftComposerRootReplacementPurposeV1 {
    AcceptedRecall(DraftComposerRecallBasisV1),
    CheckpointRestore {
        activation: DraftEditorCandidateActivationBindingV1,
        caret: DraftCompositePositionV1,
        selection: DraftCompositePositionV1,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DraftComposerRootReplacementV1 {
    pub(crate) purpose: DraftComposerRootReplacementPurposeV1,
    pub(crate) adoption: DraftPieceCommittedAdoptionV1,
}
impl DraftComposerRootReplacementV1 {
    pub fn adoption(&self) -> &DraftPieceCommittedAdoptionV1 {
        &self.adoption
    }
    pub fn purpose(&self) -> &DraftComposerRootReplacementPurposeV1 {
        &self.purpose
    }
}
