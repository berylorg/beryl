use crate::{
    AssetId, ImageLabelOrdinal, SealedAssetReferenceSetProof, SyndicDraftId, SyndicDraftMarkerId,
    SyndicItemId,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HistoricalInputMarkerAssociationV1 {
    ordinal: u64,
    marker_id: SyndicDraftMarkerId,
    label: ImageLabelOrdinal,
    asset_id: AssetId,
}

impl HistoricalInputMarkerAssociationV1 {
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
pub struct HistoricalInputMarkerPageV1 {
    draft_id: SyndicDraftId,
    session_id: [u8; 16],
    operation_id: [u8; 16],
    receipt_ordinal: u64,
    receipt_digest: [u8; 32],
    canonical_header: Vec<u8>,
    item_id: SyndicItemId,
    proof: SealedAssetReferenceSetProof,
    encoded_start: u64,
    source_piece: u64,
    association: HistoricalInputMarkerAssociationV1,
}

impl HistoricalInputMarkerPageV1 {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        draft_id: SyndicDraftId,
        session_id: [u8; 16],
        operation_id: [u8; 16],
        receipt_ordinal: u64,
        receipt_digest: [u8; 32],
        canonical_header: Vec<u8>,
        item_id: SyndicItemId,
        proof: SealedAssetReferenceSetProof,
        encoded_start: u64,
        source_piece: u64,
        association: HistoricalInputMarkerAssociationV1,
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
            item_id,
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
    pub const fn item_id(&self) -> SyndicItemId {
        self.item_id
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
    pub const fn association(&self) -> HistoricalInputMarkerAssociationV1 {
        self.association
    }
}
