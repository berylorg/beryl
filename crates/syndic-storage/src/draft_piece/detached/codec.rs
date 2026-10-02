use super::*;
use beryl_model::{AssetId, ImageLabelOrdinal, SyndicDraftMarkerId};
use std::num::NonZeroU64;

pub(super) const HEADER_BYTES: u64 = 32;
pub(super) const MARKER_BYTES: usize = 81;
pub(super) fn marker_key(marker: DraftPieceMarkerAtV1) -> DraftCompositeSearchKeyV1 {
    DraftCompositeSearchKeyV1::Marker {
        anchor: marker.anchor(),
        order_key: marker.marker().order_key(),
        marker_id: marker.marker().marker_id(),
    }
}
pub(super) fn encode_marker(at: DraftPieceMarkerAtV1) -> [u8; MARKER_BYTES] {
    let marker = at.marker();
    let mut bytes = [0; MARKER_BYTES];
    bytes[..8].copy_from_slice(&at.anchor().to_le_bytes());
    bytes[8..24].copy_from_slice(marker.marker_id().as_bytes());
    bytes[24..32].copy_from_slice(&marker.order_key().to_le_bytes());
    bytes[32..40].copy_from_slice(&marker.label().get().to_le_bytes());
    bytes[40] = marker.asset_id().version() as u8;
    bytes[41..73].copy_from_slice(&marker.asset_id().digest());
    bytes[73..81].copy_from_slice(&marker.asset_id().length().get().to_le_bytes());
    bytes
}
pub(super) fn decode_marker(bytes: &[u8]) -> Result<DraftPieceMarkerAtV1> {
    if bytes.len() != MARKER_BYTES || bytes[40] != 1 {
        return Err(DetachedDraftReadErrorV1::Invariant);
    }
    let u64_at = |start| {
        u64::from_le_bytes(
            bytes[start..start + 8]
                .try_into()
                .expect("fixed marker field"),
        )
    };
    let label =
        ImageLabelOrdinal::new(u64_at(32)).map_err(|_| DetachedDraftReadErrorV1::Invariant)?;
    let length = NonZeroU64::new(u64_at(73)).ok_or(DetachedDraftReadErrorV1::Invariant)?;
    let asset = AssetId::sha256_v1(
        bytes[41..73].try_into().expect("fixed marker digest"),
        length,
    );
    Ok(DraftPieceMarkerAtV1::new(
        u64_at(0),
        DraftPieceMarkerV1::new(
            SyndicDraftMarkerId::from_bytes(bytes[8..24].try_into().expect("fixed marker id")),
            u64_at(24),
            label,
            asset,
        ),
    ))
}
