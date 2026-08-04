use beryl_home_store::DomainReader;
use beryl_model::{SyndicContentDigest, SyndicContentId};

use crate::{
    ContentEncoding, ContentLifecycle, ContentManifestRecord, ProviderItemBuildRecord,
    advance_content_chain, codec::*, content_chain_seed, domain::SyndicDomain,
    error::SyndicValidationError,
};

use super::{
    super::scan::{point, require, scan},
    invariant,
};

#[derive(Clone, Copy)]
struct PhysicalFrontier {
    chunk_count: u64,
    encoded_bytes: u64,
    chain_digest: SyndicContentDigest,
}

impl PhysicalFrontier {
    const fn manifest(manifest: &ContentManifestRecord) -> Self {
        Self {
            chunk_count: manifest.chunk_count(),
            encoded_bytes: manifest.encoded_bytes(),
            chain_digest: manifest.chain_digest(),
        }
    }

    const fn build(build: &ProviderItemBuildRecord) -> Self {
        Self {
            chunk_count: build.staged_chunk_count(),
            encoded_bytes: build.staged_encoded_bytes(),
            chain_digest: build.staged_chain_digest(),
        }
    }
}

pub(super) fn validate(
    reader: &DomainReader<'_, SyndicDomain>,
) -> Result<(), SyndicValidationError> {
    validate_manifests(reader)?;
    validate_provider_builds(reader)?;
    validate_chunks(reader)?;
    validate_byte_spans(reader)
}

fn validate_manifests(
    reader: &DomainReader<'_, SyndicDomain>,
) -> Result<(), SyndicValidationError> {
    scan::<ContentManifestsFamily>(reader, |key, manifest| {
        let expected = manifest.expected();
        if *key != manifest.id()
            || manifest.chunk_count() > expected.chunk_count()
            || manifest.encoded_bytes() > expected.encoded_bytes()
            || expected.piece_count() == u64::MAX
            || (expected.piece_count() == 0)
                != (expected.logical_utf8_bytes() == 0 && expected.image_marker_count() == 0)
        {
            return invariant("content manifest identity or frontier is invalid");
        }

        if manifest.encoding() == ContentEncoding::ProviderItemV1 {
            validate_provider_manifest(reader, manifest)?;
        } else {
            validate_generic_manifest(manifest)?;
        }

        if manifest.chunk_count() == 0
            && (manifest.encoded_bytes() != 0
                || manifest.chain_digest() != content_chain_seed(manifest.encoding()))
        {
            return invariant("empty content frontier is invalid");
        }
        if manifest.lifecycle().is_immutable()
            && (manifest.chunk_count() != expected.chunk_count()
                || manifest.encoded_bytes() != expected.encoded_bytes()
                || manifest.chain_digest() != expected.digest())
        {
            return invariant("sealed content does not equal its final manifest");
        }
        Ok(())
    })
}

fn validate_generic_manifest(
    manifest: &ContentManifestRecord,
) -> Result<(), SyndicValidationError> {
    let expected = manifest.expected();
    match (manifest.owner(), manifest.lifecycle()) {
        (None, ContentLifecycle::Building | ContentLifecycle::Sealed)
            if manifest.id() == SyndicContentId::from_digest(*expected.digest().as_bytes()) =>
        {
            Ok(())
        }
        (Some(owner), ContentLifecycle::Live | ContentLifecycle::Finalized)
            if manifest.id() == crate::content::live_item_content_id(owner)
                && manifest.encoding() == ContentEncoding::Utf8V1
                && manifest.chunk_count() == expected.chunk_count()
                && manifest.encoded_bytes() == expected.encoded_bytes()
                && manifest.chain_digest() == expected.digest()
                && expected.piece_count() == expected.chunk_count()
                && expected.encoded_bytes() == expected.logical_utf8_bytes()
                && expected.atom_count() == 1
                && expected.image_marker_count() == 0
                && expected.marker_digest()
                    == crate::content::input_marker_digest(std::iter::empty())
                && expected.maximum_image_label().is_none() =>
        {
            Ok(())
        }
        _ => invariant("content manifest ownership or lifecycle is invalid"),
    }
}

fn validate_provider_manifest(
    reader: &DomainReader<'_, SyndicDomain>,
    manifest: &ContentManifestRecord,
) -> Result<(), SyndicValidationError> {
    let expected = manifest.expected();
    if expected.piece_count() != 0
        || expected.logical_utf8_bytes() != 0
        || expected.atom_count() != 0
        || expected.image_marker_count() != 0
        || expected.marker_digest() != crate::content::input_marker_digest(std::iter::empty())
        || expected.maximum_image_label().is_some()
    {
        return invariant("provider content has a generic logical frontier");
    }

    let build = provider_build_for_manifest(reader, manifest)?;
    match (manifest.lifecycle(), build.as_ref()) {
        (ContentLifecycle::Building, Some(build)) if build.prior().is_none() => {
            let target = build.target().content();
            if manifest.owner() != Some(build.item_id())
                || target.encoding() != ContentEncoding::ProviderItemV1
                || manifest.id() != target.id()
                || manifest.revision() != target.revision()
                || manifest.expected() != target.summary()
                || manifest.chunk_count() != 0
                || manifest.encoded_bytes() != 0
                || manifest.chain_digest() != content_chain_seed(ContentEncoding::ProviderItemV1)
            {
                return invariant("provider building manifest does not equal its build anchor");
            }
        }
        (ContentLifecycle::Live, Some(build)) => {
            let prior = build.prior().ok_or(SyndicValidationError::Invariant(
                "live provider build omitted its published prior",
            ))?;
            let published = prior.content();
            let summary = published.summary();
            if manifest.owner() != Some(build.item_id())
                || manifest.current_reference() != Some(published)
                || manifest.chunk_count() != summary.chunk_count()
                || manifest.encoded_bytes() != summary.encoded_bytes()
                || manifest.chain_digest() != summary.digest()
            {
                return invariant("live provider manifest does not equal its build prior");
            }
        }
        (ContentLifecycle::Live | ContentLifecycle::Finalized, None) => {
            if manifest.owner().is_none()
                || manifest.chunk_count() != expected.chunk_count()
                || manifest.encoded_bytes() != expected.encoded_bytes()
                || manifest.chain_digest() != expected.digest()
            {
                return invariant("published provider manifest is not fully sealed");
            }
        }
        _ => return invariant("provider manifest ownership or lifecycle is invalid"),
    }
    Ok(())
}

fn provider_build_for_manifest(
    reader: &DomainReader<'_, SyndicDomain>,
    manifest: &ContentManifestRecord,
) -> Result<Option<ProviderItemBuildRecord>, SyndicValidationError> {
    let owner = manifest.owner().ok_or(SyndicValidationError::Invariant(
        "provider manifest omitted its item owner",
    ))?;
    let build = point::<ProviderItemBuildsFamily>(reader, &owner)?;
    if let Some(build) = &build
        && (build.item_id() != owner || build.target().content().id() != manifest.id())
    {
        return invariant("provider manifest and owner-keyed build disagree");
    }
    Ok(build)
}

fn validate_provider_builds(
    reader: &DomainReader<'_, SyndicDomain>,
) -> Result<(), SyndicValidationError> {
    scan::<ProviderItemBuildsFamily>(reader, |key, build| {
        if *key != build.item_id() {
            return invariant("provider build key and item owner disagree");
        }
        let manifest = require::<ContentManifestsFamily>(
            reader,
            &build.target().content().id(),
            "provider build content manifest is missing",
        )?;
        if manifest.owner() != Some(build.item_id())
            || manifest.encoding() != ContentEncoding::ProviderItemV1
        {
            return invariant("provider build does not own one provider manifest");
        }
        validate_provider_manifest(reader, &manifest)?;
        super::super::provider_frame::validate_staged_provider_narrative(reader, build)?;
        super::super::provider_frame::validate_provider_completion_comparison(reader, build)
            .map_err(provider_frame_validation_error)?;
        if build.frame_staged() {
            super::super::provider_frame::validate_staged_provider_frame(reader, build)
                .map_err(provider_frame_validation_error)?;
        }
        Ok(())
    })
}

fn provider_frame_validation_error(
    error: super::super::provider_frame::ProviderFrameStorageValidationError,
) -> SyndicValidationError {
    match error {
        super::super::provider_frame::ProviderFrameStorageValidationError::Read(source) => {
            SyndicValidationError::Read(source)
        }
        super::super::provider_frame::ProviderFrameStorageValidationError::Invariant(message) => {
            SyndicValidationError::Invariant(message)
        }
    }
}

fn effective_frontier(
    reader: &DomainReader<'_, SyndicDomain>,
    manifest: &ContentManifestRecord,
) -> Result<PhysicalFrontier, SyndicValidationError> {
    if manifest.encoding() != ContentEncoding::ProviderItemV1 {
        return Ok(PhysicalFrontier::manifest(manifest));
    }
    Ok(provider_build_for_manifest(reader, manifest)?
        .as_ref()
        .map_or_else(
            || PhysicalFrontier::manifest(manifest),
            PhysicalFrontier::build,
        ))
}

fn validate_chunks(reader: &DomainReader<'_, SyndicDomain>) -> Result<(), SyndicValidationError> {
    let mut owner = None;
    let mut frontier = None;
    let mut expected_ordinal = 1_u64;
    let mut observed_bytes = 0_u64;
    let mut chain = None;
    scan::<ContentChunksFamily>(reader, |key, chunk| {
        if owner != Some(key.owner) {
            if owner.is_some() {
                finish_chunk_owner(
                    frontier.expect("owner frontier exists"),
                    expected_ordinal - 1,
                    observed_bytes,
                    chain.expect("owner chain exists"),
                )?;
            }
            owner = Some(key.owner);
            expected_ordinal = 1;
            observed_bytes = 0;
            let manifest = require::<ContentManifestsFamily>(
                reader,
                &key.owner,
                "content chunk owner manifest is missing",
            )?;
            frontier = Some(effective_frontier(reader, &manifest)?);
            chain = Some(content_chain_seed(manifest.encoding()));
        }
        let physical = frontier.expect("owner frontier exists");
        if key.owner != chunk.content_id()
            || key.ordinal != chunk.ordinal()
            || key.ordinal.get() != expected_ordinal
        {
            return invariant("content chunk key or contiguous order disagrees");
        }
        if key.ordinal.get() > physical.chunk_count {
            return invariant("content chunk extends beyond its physical frontier");
        }
        observed_bytes = observed_bytes
            .checked_add(chunk.bytes().len() as u64)
            .ok_or(SyndicValidationError::Invariant(
                "content byte frontier overflowed",
            ))?;
        chain = Some(advance_content_chain(
            chain.expect("owner chain exists"),
            chunk,
        ));
        expected_ordinal =
            expected_ordinal
                .checked_add(1)
                .ok_or(SyndicValidationError::Invariant(
                    "content chunk order exhausted",
                ))?;
        Ok(())
    })?;
    if owner.is_some() {
        finish_chunk_owner(
            frontier.expect("owner frontier exists"),
            expected_ordinal - 1,
            observed_bytes,
            chain.expect("owner chain exists"),
        )?;
    }
    scan::<ContentManifestsFamily>(reader, |_, manifest| {
        let physical = effective_frontier(reader, manifest)?;
        let first = ContentChunkKey {
            owner: manifest.id(),
            ordinal: crate::ContentChunkOrdinal::FIRST,
        };
        if (physical.chunk_count == 0) == point::<ContentChunksFamily>(reader, &first)?.is_some() {
            return invariant("content zero-chunk frontier disagrees");
        }
        Ok(())
    })
}

fn finish_chunk_owner(
    frontier: PhysicalFrontier,
    chunk_count: u64,
    encoded_bytes: u64,
    chain: SyndicContentDigest,
) -> Result<(), SyndicValidationError> {
    if frontier.chunk_count != chunk_count
        || frontier.encoded_bytes != encoded_bytes
        || frontier.chain_digest != chain
    {
        return invariant("content chunks disagree with their physical frontier");
    }
    Ok(())
}

fn validate_byte_spans(
    reader: &DomainReader<'_, SyndicDomain>,
) -> Result<(), SyndicValidationError> {
    let mut owner = None;
    let mut frontier = None;
    let mut expected_start = 0_u64;
    let mut expected_ordinal = 1_u64;
    scan::<ContentByteSpansFamily>(reader, |key, span| {
        if owner != Some(key.owner) {
            if owner.is_some() {
                finish_span_owner(
                    frontier.expect("owner frontier exists"),
                    expected_start,
                    expected_ordinal - 1,
                )?;
            }
            owner = Some(key.owner);
            expected_start = 0;
            expected_ordinal = 1;
            let manifest = require::<ContentManifestsFamily>(
                reader,
                &key.owner,
                "content byte-span owner manifest is missing",
            )?;
            frontier = Some(effective_frontier(reader, &manifest)?);
        }
        let physical = frontier.expect("owner frontier exists");
        if key.owner != span.content_id()
            || key.start != span.start()
            || span.start() != expected_start
            || span.ordinal().get() != expected_ordinal
            || span.ordinal().get() > physical.chunk_count
            || span.end() > physical.encoded_bytes
        {
            return invariant("content byte-span key or contiguous frontier disagrees");
        }
        let chunk = require::<ContentChunksFamily>(
            reader,
            &ContentChunkKey {
                owner: span.content_id(),
                ordinal: span.ordinal(),
            },
            "content byte span chunk is missing",
        )?;
        if span.len() != chunk.bytes().len() as u64 || span.chunk_digest() != *chunk.digest() {
            return invariant("content byte span disagrees with its chunk");
        }
        expected_start = span.end();
        expected_ordinal =
            expected_ordinal
                .checked_add(1)
                .ok_or(SyndicValidationError::Invariant(
                    "content byte-span order exhausted",
                ))?;
        Ok(())
    })?;
    if owner.is_some() {
        finish_span_owner(
            frontier.expect("owner frontier exists"),
            expected_start,
            expected_ordinal - 1,
        )?;
    }
    scan::<ContentManifestsFamily>(reader, |_, manifest| {
        let physical = effective_frontier(reader, manifest)?;
        let first = ContentByteSpanKey {
            owner: manifest.id(),
            start: 0,
        };
        if (physical.chunk_count == 0) == point::<ContentByteSpansFamily>(reader, &first)?.is_some()
        {
            return invariant("content zero-span frontier disagrees");
        }
        Ok(())
    })
}

fn finish_span_owner(
    frontier: PhysicalFrontier,
    encoded_bytes: u64,
    chunk_count: u64,
) -> Result<(), SyndicValidationError> {
    if frontier.encoded_bytes != encoded_bytes || frontier.chunk_count != chunk_count {
        return invariant("content byte spans disagree with their physical frontier");
    }
    Ok(())
}
