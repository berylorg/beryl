use beryl_home_store::{CursorDirection, DomainReader, SidecarByteLimit, SidecarVerifier};
use beryl_model::{
    ImageLabelOrdinal, OrderedMarkerAssetSummaryV1, SealedAssetReferenceSetProof,
    SequentialMarkerSummaryV1, advance_ordered_marker_asset_digest,
    advance_sequential_marker_digest, ordered_marker_asset_digest_seed,
    sequential_marker_digest_seed,
};

use super::{
    AssetDomain, AssetEntryKey, AssetLabelDisposition, AssetLabelFirstKey, AssetMarkerKey,
    AssetReferenceSetLifecycle, AssetReferenceSetManifest, AssetValidationError,
    asset_sidecar_address,
    codec::{
        AssetMetadataCodec, AssetOwnerHeadCodec, AssetReferenceCompletionEvidenceCodec,
        AssetReferenceEntryCodec, AssetReferenceLabelFirstCodec, AssetReferenceManifestCodec,
        AssetReferenceMarkerCodec,
    },
    completion, digest,
};

mod range;

use range::{
    all_entry_range, asset_range, completion_evidence_limit, entry_limit, entry_range, index_limit,
    label_range, limits, manifest_limit, marker_range, metadata_limit, owner_range, set_range,
};

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
    validate_manifests(reader)?;
    validate_completion_evidence(reader)?;
    validate_all_entries(reader)?;
    validate_marker_indexes(reader)?;
    validate_label_indexes(reader)?;
    validate_owner_heads(reader)?;
    Ok(())
}

fn validate_completion_evidence(
    reader: &DomainReader<'_, AssetDomain>,
) -> Result<(), AssetValidationError> {
    let mut after = None;
    loop {
        let page = reader.cursor::<AssetReferenceCompletionEvidenceCodec>(
            &set_range(after),
            CursorDirection::Forward,
            limits(),
        )?;
        for entry in page.records() {
            if entry.key() != &entry.value().set_id {
                return invariant("asset completion-evidence key disagrees with its identity");
            }
            let manifest = reader
                .point::<AssetReferenceManifestCodec>(entry.key(), manifest_limit())?
                .ok_or(AssetValidationError::Invariant(
                    "asset completion evidence has no manifest",
                ))?;
            let valid = match manifest.lifecycle {
                AssetReferenceSetLifecycle::Building => {
                    completion::building_matches(&manifest, entry.value())
                }
                AssetReferenceSetLifecycle::Sealed => SealedAssetReferenceSetProof::new(
                    manifest.set_id,
                    manifest.sequential,
                    manifest.ordered_assets,
                    manifest.entry_frontier,
                    manifest.asset_chain_digest,
                )
                .ok()
                .is_some_and(|proof| completion::sealed_matches(&manifest, entry.value(), proof)),
            };
            if !valid {
                return invariant("asset completion evidence does not match its manifest");
            }
        }
        if !page.has_more() {
            return Ok(());
        }
        after = page.records().last().map(|entry| *entry.key());
        if after.is_none() {
            return invariant("asset completion-evidence cursor reported more without a record");
        }
    }
}

fn validate_metadata(
    reader: &DomainReader<'_, AssetDomain>,
    sidecars: Option<&SidecarVerifier<'_>>,
) -> Result<(), AssetValidationError> {
    let mut after = None;
    loop {
        let page = reader.cursor::<AssetMetadataCodec>(
            &asset_range(after),
            CursorDirection::Forward,
            limits(),
        )?;
        for entry in page.records() {
            if entry.key() != &entry.value().asset_id {
                return invariant("asset metadata key does not match its identity");
            }
            if let Some(sidecars) = sidecars {
                sidecars
                    .verify(
                        &asset_sidecar_address(entry.value().asset_id),
                        SidecarByteLimit::new(entry.value().asset_id.length()),
                    )
                    .map_err(AssetValidationError::Sidecar)?;
            }
        }
        if !page.has_more() {
            return Ok(());
        }
        after = page.records().last().map(|entry| *entry.key());
        if after.is_none() {
            return invariant("asset metadata cursor reported more without a record");
        }
    }
}

fn validate_manifests(reader: &DomainReader<'_, AssetDomain>) -> Result<(), AssetValidationError> {
    let mut after = None;
    loop {
        let page = reader.cursor::<AssetReferenceManifestCodec>(
            &set_range(after),
            CursorDirection::Forward,
            limits(),
        )?;
        for entry in page.records() {
            if entry.key() != &entry.value().set_id {
                return invariant("asset reference-set manifest key disagrees with its identity");
            }
            if reader
                .point::<AssetReferenceCompletionEvidenceCodec>(
                    entry.key(),
                    completion_evidence_limit(),
                )?
                .is_none()
            {
                return invariant("asset reference-set manifest has no completion evidence");
            }
            if entry.value().sequential.marker_count() != entry.value().entry_frontier
                || entry.value().ordered_assets.marker_count() != entry.value().entry_frontier
            {
                return invariant("asset reference-set count disagrees with its entry frontier");
            }
            validate_manifest_entries(reader, entry.value())?;
        }
        if !page.has_more() {
            return Ok(());
        }
        after = page.records().last().map(|entry| *entry.key());
        if after.is_none() {
            return invariant("asset manifest cursor reported more without a record");
        }
    }
}

fn validate_manifest_entries(
    reader: &DomainReader<'_, AssetDomain>,
    manifest: &AssetReferenceSetManifest,
) -> Result<(), AssetValidationError> {
    let mut after = None;
    let mut count = 0_u64;
    let mut chain_digest = digest::seed(manifest.set_id);
    let mut marker_digest = sequential_marker_digest_seed();
    let mut ordered_marker_asset_digest = ordered_marker_asset_digest_seed();
    let mut maximum_image_label = None;
    loop {
        let page = reader.cursor::<AssetReferenceEntryCodec>(
            &entry_range(manifest.set_id, after),
            CursorDirection::Forward,
            limits(),
        )?;
        for stored in page.records() {
            let entry = stored.value();
            let expected_ordinal = count.checked_add(1).ok_or(AssetValidationError::Invariant(
                "asset reference-set count overflow",
            ))?;
            if stored.key().set_id != manifest.set_id
                || entry.set_id != manifest.set_id
                || stored.key().ordinal != entry.ordinal
                || entry.ordinal.get() != expected_ordinal
            {
                return invariant("asset reference-set entries are not exact and contiguous");
            }
            if reader
                .point::<AssetMetadataCodec>(&entry.asset_id, metadata_limit())?
                .is_none()
            {
                return invariant("asset reference entry names missing metadata");
            }
            let marker = reader.point::<AssetReferenceMarkerCodec>(
                &AssetMarkerKey {
                    set_id: manifest.set_id,
                    marker_id: entry.marker_id,
                },
                index_limit(),
            )?;
            if marker != Some(entry.ordinal) {
                return invariant("asset marker uniqueness index disagrees with its entry");
            }
            let label = reader
                .point::<AssetReferenceLabelFirstCodec>(
                    &AssetLabelFirstKey {
                        set_id: manifest.set_id,
                        label: entry.label,
                    },
                    index_limit(),
                )?
                .ok_or(AssetValidationError::Invariant(
                    "asset entry lacks its label-first index",
                ))?;
            if label.asset_id != entry.asset_id {
                return invariant("asset label-first index names a different asset");
            }
            let first_ordinal = match entry.label_disposition {
                AssetLabelDisposition::First => {
                    if label.first_ordinal != entry.ordinal {
                        return invariant("first asset label entry is not its label-first index");
                    }
                    entry.ordinal
                }
                AssetLabelDisposition::Repeated { first_ordinal } => {
                    if label.first_ordinal != first_ordinal || first_ordinal >= entry.ordinal {
                        return invariant("repeated asset label has an invalid first ordinal");
                    }
                    let first = reader
                        .point::<AssetReferenceEntryCodec>(
                            &AssetEntryKey {
                                set_id: manifest.set_id,
                                ordinal: first_ordinal,
                            },
                            entry_limit(),
                        )?
                        .ok_or(AssetValidationError::Invariant(
                            "repeated asset label names a missing first entry",
                        ))?;
                    if first.label != entry.label
                        || first.asset_id != entry.asset_id
                        || first.label_disposition != AssetLabelDisposition::First
                    {
                        return invariant("repeated asset label disagrees with its first entry");
                    }
                    first_ordinal
                }
            };
            chain_digest = digest::advance(
                chain_digest,
                entry.ordinal,
                entry.marker_id,
                entry.label,
                entry.asset_id,
                first_ordinal,
            );
            if entry.chain_digest != chain_digest {
                return invariant("asset reference entry chain digest is invalid");
            }
            marker_digest =
                advance_sequential_marker_digest(marker_digest, entry.marker_id, entry.label);
            ordered_marker_asset_digest = advance_ordered_marker_asset_digest(
                ordered_marker_asset_digest,
                entry.marker_id,
                entry.label,
                entry.asset_id,
            );
            maximum_image_label = Some(
                maximum_image_label.map_or(entry.label, |maximum: ImageLabelOrdinal| {
                    maximum.max(entry.label)
                }),
            );
            count = expected_ordinal;
        }
        if !page.has_more() {
            break;
        }
        after = page.records().last().map(|entry| entry.key().ordinal);
        if after.is_none() {
            return invariant("asset entry cursor reported more without a record");
        }
    }
    if count != manifest.entry_frontier
        || manifest.sequential
            != SequentialMarkerSummaryV1::new(marker_digest, count, maximum_image_label)
                .expect("replayed sequential marker summary is valid")
        || manifest.ordered_assets
            != OrderedMarkerAssetSummaryV1::new(ordered_marker_asset_digest, count)
        || chain_digest != manifest.asset_chain_digest
    {
        return invariant("asset reference-set manifest does not match its entry chain");
    }
    if manifest.lifecycle == AssetReferenceSetLifecycle::Sealed
        && SealedAssetReferenceSetProof::new(
            manifest.set_id,
            manifest.sequential,
            manifest.ordered_assets,
            manifest.entry_frontier,
            manifest.asset_chain_digest,
        )
        .is_err()
    {
        return invariant(
            "sealed asset reference-set does not match its sequential marker summary",
        );
    }
    Ok(())
}

fn validate_all_entries(
    reader: &DomainReader<'_, AssetDomain>,
) -> Result<(), AssetValidationError> {
    let mut after = None;
    loop {
        let page = reader.cursor::<AssetReferenceEntryCodec>(
            &all_entry_range(after),
            CursorDirection::Forward,
            limits(),
        )?;
        for entry in page.records() {
            if entry.key().set_id != entry.value().set_id
                || entry.key().ordinal != entry.value().ordinal
            {
                return invariant("asset entry key disagrees with its value");
            }
            if reader
                .point::<AssetReferenceManifestCodec>(&entry.key().set_id, manifest_limit())?
                .is_none()
            {
                return invariant("asset reference entry has no manifest");
            }
        }
        if !page.has_more() {
            return Ok(());
        }
        after = page.records().last().map(|entry| *entry.key());
        if after.is_none() {
            return invariant("all-entry cursor reported more without a record");
        }
    }
}

fn validate_marker_indexes(
    reader: &DomainReader<'_, AssetDomain>,
) -> Result<(), AssetValidationError> {
    let mut after = None;
    loop {
        let page = reader.cursor::<AssetReferenceMarkerCodec>(
            &marker_range(after),
            CursorDirection::Forward,
            limits(),
        )?;
        for indexed in page.records() {
            let entry = reader
                .point::<AssetReferenceEntryCodec>(
                    &AssetEntryKey {
                        set_id: indexed.key().set_id,
                        ordinal: *indexed.value(),
                    },
                    entry_limit(),
                )?
                .ok_or(AssetValidationError::Invariant(
                    "asset marker index names a missing entry",
                ))?;
            if entry.marker_id != indexed.key().marker_id {
                return invariant("asset marker index names a different marker");
            }
        }
        if !page.has_more() {
            return Ok(());
        }
        after = page.records().last().map(|entry| *entry.key());
        if after.is_none() {
            return invariant("asset marker-index cursor reported more without a record");
        }
    }
}

fn validate_label_indexes(
    reader: &DomainReader<'_, AssetDomain>,
) -> Result<(), AssetValidationError> {
    let mut after = None;
    loop {
        let page = reader.cursor::<AssetReferenceLabelFirstCodec>(
            &label_range(after),
            CursorDirection::Forward,
            limits(),
        )?;
        for indexed in page.records() {
            let entry = reader
                .point::<AssetReferenceEntryCodec>(
                    &AssetEntryKey {
                        set_id: indexed.key().set_id,
                        ordinal: indexed.value().first_ordinal,
                    },
                    entry_limit(),
                )?
                .ok_or(AssetValidationError::Invariant(
                    "asset label-first index names a missing entry",
                ))?;
            if entry.label != indexed.key().label
                || entry.asset_id != indexed.value().asset_id
                || entry.label_disposition != AssetLabelDisposition::First
            {
                return invariant("asset label-first index disagrees with its first entry");
            }
        }
        if !page.has_more() {
            return Ok(());
        }
        after = page.records().last().map(|entry| *entry.key());
        if after.is_none() {
            return invariant("asset label-index cursor reported more without a record");
        }
    }
}

fn validate_owner_heads(
    reader: &DomainReader<'_, AssetDomain>,
) -> Result<(), AssetValidationError> {
    let mut after = None;
    loop {
        let page = reader.cursor::<AssetOwnerHeadCodec>(
            &owner_range(after),
            CursorDirection::Forward,
            limits(),
        )?;
        for entry in page.records() {
            let head = entry.value();
            if entry.key() != &head.owner {
                return invariant("asset owner-head key disagrees with its owner");
            }
            let manifest = reader
                .point::<AssetReferenceManifestCodec>(&head.set.set_id(), manifest_limit())?
                .ok_or(AssetValidationError::Invariant(
                    "asset owner head names a missing reference set",
                ))?;
            if manifest.lifecycle != AssetReferenceSetLifecycle::Sealed
                || SealedAssetReferenceSetProof::new(
                    manifest.set_id,
                    manifest.sequential,
                    manifest.ordered_assets,
                    manifest.entry_frontier,
                    manifest.asset_chain_digest,
                )
                .ok()
                    != Some(head.set)
            {
                return invariant("asset owner head does not select its exact sealed set proof");
            }
        }
        if !page.has_more() {
            return Ok(());
        }
        after = page.records().last().map(|entry| *entry.key());
        if after.is_none() {
            return invariant("asset owner-head cursor reported more without a record");
        }
    }
}

fn invariant<T>(message: &'static str) -> Result<T, AssetValidationError> {
    Err(AssetValidationError::Invariant(message))
}
