use std::num::NonZeroU64;

use beryl_home_store::{CursorRange, CursorReadLimits, PointReadLimit};
use beryl_model::{
    AssetId, ImageLabelOrdinal, SyndicDraftId, SyndicDraftMarkerId, SyndicProjectionId,
};

use super::super::{
    AssetEntryKey, AssetLabelFirstKey, AssetMarkerKey, AssetOwner, AssetReferenceOrdinal,
    AssetReferenceSetId, ASSET_ENTRY_LIMIT, ASSET_INDEX_LIMIT, ASSET_MANIFEST_LIMIT,
    ASSET_METADATA_LIMIT,
};

const PAGE_ITEMS: usize = 128;
const PAGE_BYTES: usize = 2 * 1_024 * 1_024;

pub(super) fn asset_range(after: Option<AssetId>) -> CursorRange<AssetId> {
    bounded_range(after, min_asset(), max_asset())
}

pub(super) fn set_range(after: Option<AssetReferenceSetId>) -> CursorRange<AssetReferenceSetId> {
    bounded_range(after, min_set(), max_set())
}

pub(super) fn entry_range(
    set_id: AssetReferenceSetId,
    after: Option<AssetReferenceOrdinal>,
) -> CursorRange<AssetEntryKey> {
    let start = AssetEntryKey {
        set_id,
        ordinal: after.unwrap_or(AssetReferenceOrdinal::FIRST),
    };
    let end = AssetEntryKey {
        set_id,
        ordinal: max_ordinal(),
    };
    if after.is_some() {
        CursorRange::after(start, end)
    } else {
        CursorRange::closed(start, end)
    }
}

pub(super) fn all_entry_range(after: Option<AssetEntryKey>) -> CursorRange<AssetEntryKey> {
    bounded_range(
        after,
        AssetEntryKey {
            set_id: min_set(),
            ordinal: AssetReferenceOrdinal::FIRST,
        },
        AssetEntryKey {
            set_id: max_set(),
            ordinal: max_ordinal(),
        },
    )
}

pub(super) fn marker_range(after: Option<AssetMarkerKey>) -> CursorRange<AssetMarkerKey> {
    bounded_range(
        after,
        AssetMarkerKey {
            set_id: min_set(),
            marker_id: SyndicDraftMarkerId::from_bytes([0; 16]),
        },
        AssetMarkerKey {
            set_id: max_set(),
            marker_id: SyndicDraftMarkerId::from_bytes([u8::MAX; 16]),
        },
    )
}

pub(super) fn label_range(after: Option<AssetLabelFirstKey>) -> CursorRange<AssetLabelFirstKey> {
    bounded_range(
        after,
        AssetLabelFirstKey {
            set_id: min_set(),
            label: ImageLabelOrdinal::FIRST,
        },
        AssetLabelFirstKey {
            set_id: max_set(),
            label: ImageLabelOrdinal::new(u64::MAX).expect("maximum label is nonzero"),
        },
    )
}

pub(super) fn owner_range(after: Option<AssetOwner>) -> CursorRange<AssetOwner> {
    bounded_range(
        after,
        AssetOwner::CurrentDraft(SyndicDraftId::from_bytes([0; 16])),
        AssetOwner::TranscriptProjection(SyndicProjectionId::from_bytes([u8::MAX; 16])),
    )
}

fn bounded_range<K>(after: Option<K>, minimum: K, maximum: K) -> CursorRange<K> {
    match after {
        Some(after) => CursorRange::after(after, maximum),
        None => CursorRange::closed(minimum, maximum),
    }
}

fn min_asset() -> AssetId {
    AssetId::sha256_v1([0; 32], NonZeroU64::MIN)
}

fn max_asset() -> AssetId {
    AssetId::sha256_v1([u8::MAX; 32], NonZeroU64::MAX)
}

const fn min_set() -> AssetReferenceSetId {
    AssetReferenceSetId::from_bytes([0; 16])
}

const fn max_set() -> AssetReferenceSetId {
    AssetReferenceSetId::from_bytes([u8::MAX; 16])
}

fn max_ordinal() -> AssetReferenceOrdinal {
    AssetReferenceOrdinal::new(u64::MAX).expect("maximum ordinal is nonzero")
}

pub(super) fn limits() -> CursorReadLimits {
    CursorReadLimits::new(PAGE_ITEMS, PAGE_BYTES).expect("asset validation bounds are nonzero")
}

pub(super) fn metadata_limit() -> PointReadLimit {
    PointReadLimit::new(ASSET_METADATA_LIMIT + 4).expect("metadata point bound is nonzero")
}

pub(super) fn manifest_limit() -> PointReadLimit {
    PointReadLimit::new(ASSET_MANIFEST_LIMIT + 4).expect("manifest point bound is nonzero")
}

pub(super) fn entry_limit() -> PointReadLimit {
    PointReadLimit::new(ASSET_ENTRY_LIMIT + 4).expect("entry point bound is nonzero")
}

pub(super) fn index_limit() -> PointReadLimit {
    PointReadLimit::new(ASSET_INDEX_LIMIT + 4).expect("index point bound is nonzero")
}
