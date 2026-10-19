use sha2::{Digest, Sha256};

use crate::{
    AssetId, ImageLabelOrdinal, SealedAssetReferenceSetProof, SyndicAcceptedInputId, SyndicDraftId,
    SyndicDraftMarkerId, SyndicItemId, SyndicTurnId,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComposerRecallSourceOwnerV1 {
    AcceptedInput(SyndicAcceptedInputId),
    SubmittedTurnItem {
        turn_id: SyndicTurnId,
        item_id: SyndicItemId,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ComposerRecallMarkerAssociationV1 {
    ordinal: u64,
    marker_id: SyndicDraftMarkerId,
    label: ImageLabelOrdinal,
    asset_id: AssetId,
}

impl ComposerRecallMarkerAssociationV1 {
    pub const fn new(
        ordinal: u64,
        marker_id: SyndicDraftMarkerId,
        label: ImageLabelOrdinal,
        asset_id: AssetId,
    ) -> Option<Self> {
        if ordinal == 0 {
            return None;
        }
        Some(Self {
            ordinal,
            marker_id,
            label,
            asset_id,
        })
    }

    pub const fn ordinal(self) -> u64 {
        self.ordinal
    }
    pub const fn marker_id(self) -> SyndicDraftMarkerId {
        self.marker_id
    }
    pub const fn label(self) -> ImageLabelOrdinal {
        self.label
    }
    pub const fn asset_id(self) -> AssetId {
        self.asset_id
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComposerRecallMarkerPageV1 {
    draft_id: SyndicDraftId,
    session_id: [u8; 16],
    operation_id: [u8; 16],
    receipt_ordinal: u64,
    receipt_digest: [u8; 32],
    canonical_header: Vec<u8>,
    source_draft_id: SyndicDraftId,
    source_session_id: [u8; 16],
    source_operation_id: [u8; 16],
    acceptance_digest: [u8; 32],
    source_owner: ComposerRecallSourceOwnerV1,
    proof: SealedAssetReferenceSetProof,
    encoded_start: u64,
    source_piece: u64,
    association: ComposerRecallMarkerAssociationV1,
}

impl ComposerRecallMarkerPageV1 {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        draft_id: SyndicDraftId,
        session_id: [u8; 16],
        operation_id: [u8; 16],
        receipt_ordinal: u64,
        receipt_digest: [u8; 32],
        canonical_header: Vec<u8>,
        source_draft_id: SyndicDraftId,
        source_session_id: [u8; 16],
        source_operation_id: [u8; 16],
        acceptance_digest: [u8; 32],
        source_owner: ComposerRecallSourceOwnerV1,
        proof: SealedAssetReferenceSetProof,
        encoded_start: u64,
        source_piece: u64,
        association: ComposerRecallMarkerAssociationV1,
    ) -> Option<Self> {
        if receipt_ordinal == 0
            || canonical_header.is_empty()
            || canonical_header.len() > 65_536
            || source_piece == 0
            || association.ordinal() > proof.entry_frontier()
            || encoded_start.checked_add(25).is_none()
        {
            return None;
        }
        Some(Self {
            draft_id,
            session_id,
            operation_id,
            receipt_ordinal,
            receipt_digest,
            canonical_header,
            source_draft_id,
            source_session_id,
            source_operation_id,
            acceptance_digest,
            source_owner,
            proof,
            encoded_start,
            source_piece,
            association,
        })
    }

    pub const fn draft_id(&self) -> SyndicDraftId {
        self.draft_id
    }
    pub const fn session_id(&self) -> [u8; 16] {
        self.session_id
    }
    pub const fn operation_id(&self) -> [u8; 16] {
        self.operation_id
    }
    pub const fn receipt_ordinal(&self) -> u64 {
        self.receipt_ordinal
    }
    pub const fn receipt_digest(&self) -> [u8; 32] {
        self.receipt_digest
    }
    pub fn canonical_header(&self) -> &[u8] {
        &self.canonical_header
    }
    pub const fn source_draft_id(&self) -> SyndicDraftId {
        self.source_draft_id
    }
    pub const fn source_session_id(&self) -> [u8; 16] {
        self.source_session_id
    }
    pub const fn source_operation_id(&self) -> [u8; 16] {
        self.source_operation_id
    }
    pub const fn acceptance_digest(&self) -> [u8; 32] {
        self.acceptance_digest
    }
    pub const fn source_owner(&self) -> ComposerRecallSourceOwnerV1 {
        self.source_owner
    }
    pub const fn proof(&self) -> SealedAssetReferenceSetProof {
        self.proof
    }
    pub const fn encoded_start(&self) -> u64 {
        self.encoded_start
    }
    pub const fn source_piece(&self) -> u64 {
        self.source_piece
    }
    pub const fn association(&self) -> ComposerRecallMarkerAssociationV1 {
        self.association
    }
}

pub fn composer_recall_marker_correlation(page: &ComposerRecallMarkerPageV1) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(b"syndic/composer-recall-marker-window/v1");
    hash.update(page.draft_id().as_bytes());
    hash.update(page.session_id());
    hash.update(page.operation_id());
    hash.update(page.receipt_ordinal().to_be_bytes());
    hash.update(page.receipt_digest());
    hash.update((page.canonical_header().len() as u64).to_be_bytes());
    hash.update(page.canonical_header());
    hash.update(page.source_draft_id().as_bytes());
    hash.update(page.source_session_id());
    hash.update(page.source_operation_id());
    hash.update(page.acceptance_digest());
    match page.source_owner() {
        ComposerRecallSourceOwnerV1::AcceptedInput(id) => {
            hash.update([0]);
            hash.update(id.as_bytes());
        }
        ComposerRecallSourceOwnerV1::SubmittedTurnItem { turn_id, item_id } => {
            hash.update([1]);
            hash.update(turn_id.as_bytes());
            hash.update(item_id.as_bytes());
        }
    }
    let proof = page.proof();
    hash.update(proof.set_id().as_bytes());
    hash.update(proof.sequential().marker_digest());
    hash.update(proof.sequential().marker_count().to_be_bytes());
    hash.update(
        proof
            .sequential()
            .maximum_image_label()
            .map_or(0, |v| v.get())
            .to_be_bytes(),
    );
    hash.update(proof.ordered_assets().marker_asset_digest());
    hash.update(proof.ordered_assets().marker_count().to_be_bytes());
    hash.update(proof.entry_frontier().to_be_bytes());
    hash.update(proof.asset_chain_digest().as_bytes());
    hash.update(page.encoded_start().to_be_bytes());
    hash.update(page.source_piece().to_be_bytes());
    let association = page.association();
    hash.update(association.ordinal().to_be_bytes());
    hash.update(association.marker_id().as_bytes());
    hash.update(association.label().get().to_be_bytes());
    hash.update([association.asset_id().version() as u8]);
    hash.update(association.asset_id().digest());
    hash.update(association.asset_id().length().get().to_be_bytes());
    hash.finalize().into()
}
