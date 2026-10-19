use beryl_model::{
    ImageLabelOrdinal, InputGateRevision, ProjectionRevision, SealedAssetReferenceSetProof,
    SyndicItemId,
};

use crate::{ContentEncoding, ContentReference, ReplacementEditIntent};

use super::super::{DraftEditorCurrentSelectorV1, DraftPieceBuildLifecycleV1};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftSealedImportBasisV1 {
    pub(crate) selector: DraftEditorCurrentSelectorV1,
    pub(crate) gate_revision: InputGateRevision,
    pub(crate) intent: ReplacementEditIntent,
    pub(crate) item_id: SyndicItemId,
    pub(crate) item_revision: ProjectionRevision,
    pub(crate) content: ContentReference,
    pub(crate) assets: Option<SealedAssetReferenceSetProof>,
}

impl DraftSealedImportBasisV1 {
    pub const fn selector(self) -> DraftEditorCurrentSelectorV1 {
        self.selector
    }
    pub const fn gate_revision(self) -> InputGateRevision {
        self.gate_revision
    }
    pub const fn intent(self) -> ReplacementEditIntent {
        self.intent
    }
    pub const fn item_id(self) -> SyndicItemId {
        self.item_id
    }
    pub const fn item_revision(self) -> ProjectionRevision {
        self.item_revision
    }
    pub const fn content(self) -> ContentReference {
        self.content
    }
    pub const fn asset_reference_set(self) -> Option<SealedAssetReferenceSetProof> {
        self.assets
    }

    pub(crate) fn is_locally_exact(self) -> bool {
        let summary = self.content.summary();
        self.content.encoding() == ContentEncoding::ComposerV1
            && self.selector.thread_revision() == self.intent.selected_path().thread_revision()
            && self.selector.root().key().draft_id() == self.selector.draft_id()
            && self.selector.history().root() == self.selector.root()
            && summary.encoded_bytes() >= 9
            && summary.piece_count().checked_add(1).is_some()
            && match self.assets {
                Some(proof) => {
                    proof.sequential().marker_count() == summary.image_marker_count()
                        && proof.sequential().marker_digest() == summary.marker_digest()
                        && proof.sequential().maximum_image_label() == summary.maximum_image_label()
                        && proof.ordered_assets().marker_count() == summary.image_marker_count()
                        && summary.image_marker_count() != 0
                }
                None => summary.image_marker_count() == 0,
            }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftSealedImportPhaseV1 {
    Streaming,
    CrossValidating,
    Complete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftSealedImportDecoderV1 {
    BeforeHeader,
    BetweenAtoms,
    TextBody,
    End,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftSealedComposerFrontierV1<B> {
    pub(crate) basis: B,
    pub(crate) phase: DraftSealedImportPhaseV1,
    pub(crate) encoded_cursor: u64,
    pub(crate) atoms: u64,
    pub(crate) logical_bytes: u64,
    pub(crate) markers: u64,
    pub(crate) decoder: DraftSealedImportDecoderV1,
    pub(crate) text_remaining: u64,
    pub(crate) next_piece: u64,
    pub(crate) piece_text_offset: u64,
    pub(crate) next_leaf_rank: u64,
    pub(crate) checked_markers: u64,
    pub(crate) sequential_digest: [u8; 32],
    pub(crate) maximum_label: Option<ImageLabelOrdinal>,
    pub(crate) ordered_digest: [u8; 32],
}

impl<B: SealedComposerBasisV1> DraftSealedComposerFrontierV1<B> {
    pub fn basis(&self) -> B {
        self.basis.clone()
    }
    pub const fn phase(&self) -> DraftSealedImportPhaseV1 {
        self.phase
    }
    pub const fn encoded_cursor(&self) -> u64 {
        self.encoded_cursor
    }
    pub const fn marker_count(&self) -> u64 {
        self.markers
    }

    pub(crate) fn is_locally_exact(&self, lifecycle: DraftPieceBuildLifecycleV1) -> bool {
        let summary = self.basis.content().summary();
        let fold_count = if self.phase == DraftSealedImportPhaseV1::Streaming {
            self.markers
        } else {
            self.checked_markers
        };
        if !self.basis.is_locally_exact()
            || self.encoded_cursor > summary.encoded_bytes()
            || self.atoms > summary.atom_count()
            || self.logical_bytes > summary.logical_utf8_bytes()
            || self.markers > summary.image_marker_count()
            || self.next_piece == 0
            || self.next_piece > summary.piece_count().saturating_add(1)
            || self.checked_markers > self.markers
            || (self.decoder != DraftSealedImportDecoderV1::TextBody && self.text_remaining != 0)
            || (self.decoder == DraftSealedImportDecoderV1::TextBody && self.text_remaining == 0)
            || (fold_count == 0) != self.maximum_label.is_none()
            || self.maximum_label > summary.maximum_image_label()
            || fold_count == 0
                && (self.sequential_digest != beryl_model::sequential_marker_digest_seed()
                    || self.ordered_digest != beryl_model::ordered_marker_asset_digest_seed())
        {
            return false;
        }
        if self.decoder == DraftSealedImportDecoderV1::BeforeHeader
            && (self.encoded_cursor != 0
                || self.atoms != 0
                || self.logical_bytes != 0
                || self.markers != 0
                || self.next_piece != 1
                || self.piece_text_offset != 0
                || self.next_leaf_rank != 0
                || self.checked_markers != 0
                || self.sequential_digest != beryl_model::sequential_marker_digest_seed()
                || self.ordered_digest != beryl_model::ordered_marker_asset_digest_seed())
        {
            return false;
        }
        let source_complete = self.decoder == DraftSealedImportDecoderV1::End
            && self.encoded_cursor == summary.encoded_bytes()
            && self.atoms == summary.atom_count()
            && self.logical_bytes == summary.logical_utf8_bytes()
            && self.markers == summary.image_marker_count()
            && self.next_piece == summary.piece_count().saturating_add(1)
            && self.piece_text_offset == 0;
        let folds_complete = self.sequential_digest == summary.marker_digest()
            && self.maximum_label == summary.maximum_image_label()
            && self.basis.assets().is_none_or(|proof| {
                self.ordered_digest == proof.ordered_assets().marker_asset_digest()
            });
        match self.phase {
            DraftSealedImportPhaseV1::Streaming => {
                self.decoder != DraftSealedImportDecoderV1::End
                    && self.next_leaf_rank == 0
                    && self.checked_markers == 0
                    && matches!(
                        lifecycle,
                        DraftPieceBuildLifecycleV1::Open
                            | DraftPieceBuildLifecycleV1::Rejected
                            | DraftPieceBuildLifecycleV1::Cancelled
                            | DraftPieceBuildLifecycleV1::Error
                    )
            }
            DraftSealedImportPhaseV1::CrossValidating => {
                source_complete
                    && (self.checked_markers != self.markers || folds_complete)
                    && matches!(
                        lifecycle,
                        DraftPieceBuildLifecycleV1::Open
                            | DraftPieceBuildLifecycleV1::Rejected
                            | DraftPieceBuildLifecycleV1::Cancelled
                            | DraftPieceBuildLifecycleV1::Error
                    )
            }
            DraftSealedImportPhaseV1::Complete => {
                source_complete
                    && folds_complete
                    && self.checked_markers == self.markers
                    && matches!(
                        lifecycle,
                        DraftPieceBuildLifecycleV1::Complete
                            | DraftPieceBuildLifecycleV1::Committed
                            | DraftPieceBuildLifecycleV1::Conflict
                            | DraftPieceBuildLifecycleV1::Error
                            | DraftPieceBuildLifecycleV1::Cancelled
                    )
            }
        }
    }
}

pub type DraftSealedImportFrontierV1 = DraftSealedComposerFrontierV1<DraftSealedImportBasisV1>;

pub(crate) trait SealedComposerBasisV1: Clone {
    fn content(&self) -> ContentReference;
    fn assets(&self) -> Option<SealedAssetReferenceSetProof>;
    fn is_locally_exact(&self) -> bool;
    fn wrap(
        frontier: DraftSealedComposerFrontierV1<Self>,
    ) -> super::super::DraftPieceBuildFrontierV1
    where
        Self: Sized;
}

impl SealedComposerBasisV1 for DraftSealedImportBasisV1 {
    fn content(&self) -> ContentReference {
        self.content
    }
    fn assets(&self) -> Option<SealedAssetReferenceSetProof> {
        self.assets
    }
    fn is_locally_exact(&self) -> bool {
        (*self).is_locally_exact()
    }
    fn wrap(frontier: DraftSealedImportFrontierV1) -> super::super::DraftPieceBuildFrontierV1 {
        super::super::DraftPieceBuildFrontierV1::SealedImport(frontier)
    }
}

#[derive(Clone, Copy)]
pub(crate) struct SealedComposerMarkerAssociationV1 {
    ordinal: u64,
    marker_id: beryl_model::SyndicDraftMarkerId,
    label: ImageLabelOrdinal,
    asset: beryl_model::AssetId,
}
impl SealedComposerMarkerAssociationV1 {
    pub fn ordinal(self) -> u64 {
        self.ordinal
    }
    pub fn marker_id(self) -> beryl_model::SyndicDraftMarkerId {
        self.marker_id
    }
    pub fn label(self) -> ImageLabelOrdinal {
        self.label
    }
    pub fn asset_id(self) -> beryl_model::AssetId {
        self.asset
    }
}
impl From<beryl_model::HistoricalInputMarkerAssociationV1> for SealedComposerMarkerAssociationV1 {
    fn from(a: beryl_model::HistoricalInputMarkerAssociationV1) -> Self {
        Self {
            ordinal: a.ordinal(),
            marker_id: a.marker_id(),
            label: a.label(),
            asset: a.asset_id(),
        }
    }
}
impl From<beryl_model::ComposerRecallMarkerAssociationV1> for SealedComposerMarkerAssociationV1 {
    fn from(a: beryl_model::ComposerRecallMarkerAssociationV1) -> Self {
        Self {
            ordinal: a.ordinal(),
            marker_id: a.marker_id(),
            label: a.label(),
            asset: a.asset_id(),
        }
    }
}
