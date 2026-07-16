use std::num::NonZeroU64;

use beryl_home_store::{
    CursorDirection, CursorRange, CursorReadLimits, DomainReader, PointReadLimit, SidecarAddress,
    SidecarByteLimit, SidecarDigest, SidecarNamespace, SidecarVerifier,
};
use beryl_model::{AssetId, SyndicDraftId, SyndicDraftMarkerId, SyndicProjectionId};

use super::{
    ASSET_METADATA_LIMIT, ASSET_NAMESPACE, ASSET_REFERENCE_LIMIT, AssetDomain, AssetReferenceOwner,
    AssetValidationError, MAX_ASSET_BYTES,
    codec::{
        AssetMetadataCodec, AssetReferenceCodec, AssetReferenceIndexCodec, AssetReferenceIndexKey,
    },
};

const PAGE_ITEMS: usize = 128;
const PAGE_BYTES: usize = 2 * 1_024 * 1_024;

pub(super) fn validate(reader: &DomainReader<'_, AssetDomain>) -> Result<(), AssetValidationError> {
    validate_records(reader, None)
}

pub(super) fn validate_reopen(
    reader: &DomainReader<'_, AssetDomain>,
    sidecars: &SidecarVerifier<'_>,
) -> Result<(), AssetValidationError> {
    validate_records(reader, Some(sidecars))
}

fn validate_records(
    reader: &DomainReader<'_, AssetDomain>,
    sidecars: Option<&SidecarVerifier<'_>>,
) -> Result<(), AssetValidationError> {
    validate_metadata(reader, sidecars)?;
    validate_primary_references(reader)?;
    validate_reference_indexes(reader)?;
    Ok(())
}

fn validate_metadata(
    reader: &DomainReader<'_, AssetDomain>,
    sidecars: Option<&SidecarVerifier<'_>>,
) -> Result<(), AssetValidationError> {
    let mut after = None;
    loop {
        let range = asset_range(after);
        let page =
            reader.cursor::<AssetMetadataCodec>(&range, CursorDirection::Forward, limits())?;
        for entry in page.records() {
            let record = entry.value();
            if entry.key() != &record.asset_id {
                return Err(AssetValidationError::Invariant(
                    "asset metadata key does not match its identity",
                ));
            }
            if record.asset_id.length().get() > MAX_ASSET_BYTES {
                return Err(AssetValidationError::Invariant(
                    "asset exceeds the durable byte bound",
                ));
            }
            let count = count_asset_references(reader, record.asset_id)?;
            if count != record.reference_count {
                return Err(AssetValidationError::Invariant(
                    "asset reference count disagrees with its index",
                ));
            }
            if let Some(sidecars) = sidecars {
                verify_sidecar(sidecars, record.asset_id)?;
            }
        }
        if !page.has_more() {
            return Ok(());
        }
        after = page.records().last().map(|entry| *entry.key());
        if after.is_none() {
            return Err(AssetValidationError::Invariant(
                "asset cursor reported more without a record",
            ));
        }
    }
}

fn count_asset_references(
    reader: &DomainReader<'_, AssetDomain>,
    asset_id: AssetId,
) -> Result<u64, AssetValidationError> {
    let mut after = None;
    let mut count = 0_u64;
    loop {
        let start = AssetReferenceIndexKey::minimum(asset_id, after);
        let end = AssetReferenceIndexKey::maximum(asset_id);
        let range = if after.is_some() {
            CursorRange::after(start, end)
        } else {
            CursorRange::closed(start, end)
        };
        let page = reader.cursor::<AssetReferenceIndexCodec>(
            &range,
            CursorDirection::Forward,
            limits(),
        )?;
        for entry in page.records() {
            if entry.key().asset_id != asset_id
                || entry.value().asset_id != asset_id
                || entry.key().owner != entry.value().owner
            {
                return Err(AssetValidationError::Invariant(
                    "asset reference index record is inconsistent",
                ));
            }
            let primary =
                reader.point::<AssetReferenceCodec>(&entry.key().owner, reference_limit())?;
            if primary.as_ref() != Some(entry.value()) {
                return Err(AssetValidationError::Invariant(
                    "asset reference index lacks an exact primary record",
                ));
            }
            count = count.checked_add(1).ok_or(AssetValidationError::Invariant(
                "asset reference count overflow",
            ))?;
        }
        if !page.has_more() {
            return Ok(count);
        }
        after = page.records().last().map(|entry| entry.key().owner);
        if after.is_none() {
            return Err(AssetValidationError::Invariant(
                "asset reference cursor reported more without a record",
            ));
        }
    }
}

fn validate_primary_references(
    reader: &DomainReader<'_, AssetDomain>,
) -> Result<(), AssetValidationError> {
    let mut after = None;
    loop {
        let range = owner_range(after);
        let page =
            reader.cursor::<AssetReferenceCodec>(&range, CursorDirection::Forward, limits())?;
        for entry in page.records() {
            if entry.key() != &entry.value().owner {
                return Err(AssetValidationError::Invariant(
                    "asset reference key does not match its owner",
                ));
            }
            let metadata =
                reader.point::<AssetMetadataCodec>(&entry.value().asset_id, metadata_limit())?;
            if metadata.is_none() {
                return Err(AssetValidationError::Invariant(
                    "asset reference names missing metadata",
                ));
            }
            let index_key = AssetReferenceIndexKey {
                asset_id: entry.value().asset_id,
                owner: entry.value().owner,
            };
            let indexed =
                reader.point::<AssetReferenceIndexCodec>(&index_key, reference_limit())?;
            if indexed.as_ref() != Some(entry.value()) {
                return Err(AssetValidationError::Invariant(
                    "asset reference lacks an exact index record",
                ));
            }
        }
        if !page.has_more() {
            return Ok(());
        }
        after = page.records().last().map(|entry| *entry.key());
        if after.is_none() {
            return Err(AssetValidationError::Invariant(
                "asset reference cursor reported more without a record",
            ));
        }
    }
}

fn validate_reference_indexes(
    reader: &DomainReader<'_, AssetDomain>,
) -> Result<(), AssetValidationError> {
    let mut after = None;
    loop {
        let range = index_range(after);
        let page = reader.cursor::<AssetReferenceIndexCodec>(
            &range,
            CursorDirection::Forward,
            limits(),
        )?;
        for entry in page.records() {
            if entry.key().asset_id != entry.value().asset_id
                || entry.key().owner != entry.value().owner
            {
                return Err(AssetValidationError::Invariant(
                    "asset index key does not match its reference",
                ));
            }
            let metadata =
                reader.point::<AssetMetadataCodec>(&entry.key().asset_id, metadata_limit())?;
            if metadata.is_none() {
                return Err(AssetValidationError::Invariant(
                    "asset index names missing metadata",
                ));
            }
            let primary =
                reader.point::<AssetReferenceCodec>(&entry.key().owner, reference_limit())?;
            if primary.as_ref() != Some(entry.value()) {
                return Err(AssetValidationError::Invariant(
                    "asset index lacks an exact primary reference",
                ));
            }
        }
        if !page.has_more() {
            return Ok(());
        }
        after = page.records().last().map(|entry| *entry.key());
        if after.is_none() {
            return Err(AssetValidationError::Invariant(
                "asset index cursor reported more without a record",
            ));
        }
    }
}

fn verify_sidecar(
    verifier: &SidecarVerifier<'_>,
    asset_id: AssetId,
) -> Result<(), AssetValidationError> {
    let namespace = SidecarNamespace::new(ASSET_NAMESPACE)
        .map_err(|_| AssetValidationError::Invariant("asset sidecar namespace is invalid"))?;
    let address = SidecarAddress::new(
        namespace,
        SidecarDigest::from_bytes(asset_id.digest()),
        asset_id.length().get(),
    );
    verifier
        .verify(
            &address,
            SidecarByteLimit::new(
                NonZeroU64::new(MAX_ASSET_BYTES).expect("asset bound is nonzero"),
            ),
        )
        .map_err(AssetValidationError::Sidecar)
}

fn asset_range(after: Option<AssetId>) -> CursorRange<AssetId> {
    let start = after.unwrap_or_else(min_asset);
    if after.is_some() {
        CursorRange::after(start, max_asset())
    } else {
        CursorRange::closed(start, max_asset())
    }
}

fn owner_range(after: Option<AssetReferenceOwner>) -> CursorRange<AssetReferenceOwner> {
    let start = after.unwrap_or_else(min_owner);
    if after.is_some() {
        CursorRange::after(start, max_owner())
    } else {
        CursorRange::closed(start, max_owner())
    }
}

fn index_range(after: Option<AssetReferenceIndexKey>) -> CursorRange<AssetReferenceIndexKey> {
    let start = after.unwrap_or_else(|| AssetReferenceIndexKey::minimum(min_asset(), None));
    let end = AssetReferenceIndexKey::maximum(max_asset());
    if after.is_some() {
        CursorRange::after(start, end)
    } else {
        CursorRange::closed(start, end)
    }
}

fn min_asset() -> AssetId {
    AssetId::sha256_v1([0; 32], NonZeroU64::MIN)
}

fn max_asset() -> AssetId {
    AssetId::sha256_v1([u8::MAX; 32], NonZeroU64::MAX)
}

fn min_owner() -> AssetReferenceOwner {
    AssetReferenceOwner::CurrentDraftMarker {
        draft_id: SyndicDraftId::from_bytes([0; 16]),
        marker_id: SyndicDraftMarkerId::from_bytes([0; 16]),
    }
}

fn max_owner() -> AssetReferenceOwner {
    AssetReferenceOwner::TranscriptProjection {
        projection_id: SyndicProjectionId::from_bytes([u8::MAX; 16]),
    }
}

fn limits() -> CursorReadLimits {
    CursorReadLimits::new(PAGE_ITEMS, PAGE_BYTES).expect("asset validation limits are nonzero")
}

fn metadata_limit() -> PointReadLimit {
    PointReadLimit::new(ASSET_METADATA_LIMIT + 4).expect("asset metadata limit is nonzero")
}

fn reference_limit() -> PointReadLimit {
    PointReadLimit::new(ASSET_REFERENCE_LIMIT + 4).expect("asset reference limit is nonzero")
}
