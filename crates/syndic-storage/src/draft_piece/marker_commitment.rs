use beryl_model::{
    AssetId, DraftMarkerCommitmentV1, ImageLabelOrdinal, SyndicDraftId, SyndicDraftMarkerId,
};
use sha2::{Digest, Sha256};

use super::{DraftPieceDigestV1, DraftPieceRecordIdV1};

const EMPTY_COMMITMENT: &[u8] = b"syndic/draft-marker-order-commitment-root/v1/empty";
const COMMITMENT_LEAF: &[u8] = b"syndic/draft-marker-order-commitment-leaf/v2";
const COMMITMENT_NODE: &[u8] = b"syndic/draft-marker-order-commitment-node/v1";

pub fn canonical_empty_draft_marker_commitment_v1() -> DraftMarkerCommitmentV1 {
    DraftMarkerCommitmentV1::new(
        *commitment_digest_parts(EMPTY_COMMITMENT, &[]).as_bytes(),
        0,
        None,
    )
    .expect("canonical empty commitment has a valid shape")
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) enum DraftMarkerOrderRecordKindV1 {
    Internal,
    Leaf,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct DraftMarkerOrderRecordKeyV1 {
    draft_id: SyndicDraftId,
    kind: DraftMarkerOrderRecordKindV1,
    id: DraftPieceRecordIdV1,
}

impl DraftMarkerOrderRecordKeyV1 {
    pub(crate) const fn new(
        draft_id: SyndicDraftId,
        kind: DraftMarkerOrderRecordKindV1,
        id: DraftPieceRecordIdV1,
    ) -> Self {
        Self { draft_id, kind, id }
    }

    pub(crate) const fn draft_id(self) -> SyndicDraftId {
        self.draft_id
    }
    pub(crate) const fn kind(self) -> DraftMarkerOrderRecordKindV1 {
        self.kind
    }
    pub(crate) const fn id(self) -> DraftPieceRecordIdV1 {
        self.id
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DraftMarkerOrderChildV1 {
    id: DraftPieceRecordIdV1,
    digest: DraftPieceDigestV1,
    marker_count: u64,
    maximum_image_label: Option<ImageLabelOrdinal>,
}

impl DraftMarkerOrderChildV1 {
    pub(crate) const fn new(
        id: DraftPieceRecordIdV1,
        digest: DraftPieceDigestV1,
        marker_count: u64,
        maximum_image_label: Option<ImageLabelOrdinal>,
    ) -> Option<Self> {
        if marker_count == 0 || maximum_image_label.is_none() {
            return None;
        }
        Some(Self {
            id,
            digest,
            marker_count,
            maximum_image_label,
        })
    }

    pub(crate) const fn id(self) -> DraftPieceRecordIdV1 {
        self.id
    }
    pub(crate) const fn digest(self) -> DraftPieceDigestV1 {
        self.digest
    }
    pub(crate) const fn marker_count(self) -> u64 {
        self.marker_count
    }
    pub(crate) const fn maximum_image_label(self) -> Option<ImageLabelOrdinal> {
        self.maximum_image_label
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum DraftMarkerOrderRecordV1 {
    Internal {
        key: DraftMarkerOrderRecordKeyV1,
        height: u8,
        children: Vec<DraftMarkerOrderChildV1>,
        digest: DraftPieceDigestV1,
    },
    Leaf {
        key: DraftMarkerOrderRecordKeyV1,
        marker_id: SyndicDraftMarkerId,
        label: ImageLabelOrdinal,
        asset_id: AssetId,
        digest: DraftPieceDigestV1,
    },
}

impl DraftMarkerOrderRecordV1 {
    pub(crate) const fn key(&self) -> DraftMarkerOrderRecordKeyV1 {
        match self {
            Self::Internal { key, .. } | Self::Leaf { key, .. } => *key,
        }
    }
    pub(crate) const fn digest(&self) -> DraftPieceDigestV1 {
        match self {
            Self::Internal { digest, .. } | Self::Leaf { digest, .. } => *digest,
        }
    }
    pub(crate) const fn height(&self) -> u8 {
        match self {
            Self::Internal { height, .. } => *height,
            Self::Leaf { .. } => 0,
        }
    }
    pub(crate) fn children(&self) -> Option<&[DraftMarkerOrderChildV1]> {
        match self {
            Self::Internal { children, .. } => Some(children),
            Self::Leaf { .. } => None,
        }
    }
    pub(crate) const fn marker(&self) -> Option<(SyndicDraftMarkerId, ImageLabelOrdinal, AssetId)> {
        match self {
            Self::Leaf {
                marker_id,
                label,
                asset_id,
                ..
            } => Some((*marker_id, *label, *asset_id)),
            Self::Internal { .. } => None,
        }
    }
}

pub(crate) fn marker_order_leaf_digest(
    marker_id: SyndicDraftMarkerId,
    label: ImageLabelOrdinal,
    asset_id: AssetId,
) -> DraftPieceDigestV1 {
    commitment_digest_parts(
        COMMITMENT_LEAF,
        &[
            marker_id.as_bytes(),
            &label.get().to_be_bytes(),
            &[asset_id.version() as u8],
            &asset_id.digest(),
            &asset_id.length().get().to_be_bytes(),
        ],
    )
}

pub(crate) fn marker_order_node_digest(
    height: u8,
    children: &[DraftMarkerOrderChildV1],
) -> DraftPieceDigestV1 {
    let mut bytes = Vec::with_capacity(9 + children.len() * 60);
    bytes.push(height);
    bytes.extend_from_slice(&(children.len() as u64).to_be_bytes());
    for child in children {
        bytes.extend_from_slice(child.id().as_bytes());
        bytes.extend_from_slice(child.digest().as_bytes());
        bytes.extend_from_slice(&child.marker_count().to_be_bytes());
        bytes.extend_from_slice(
            &child
                .maximum_image_label()
                .expect("nonempty child")
                .get()
                .to_be_bytes(),
        );
    }
    commitment_digest_parts(COMMITMENT_NODE, &[&bytes])
}

fn commitment_digest_parts(domain: &[u8], parts: &[&[u8]]) -> DraftPieceDigestV1 {
    let mut digest = Sha256::new();
    digest.update((domain.len() as u64).to_be_bytes());
    digest.update(domain);
    for part in parts {
        digest.update((part.len() as u64).to_be_bytes());
        digest.update(part);
    }
    DraftPieceDigestV1::from_bytes(digest.finalize().into())
}
