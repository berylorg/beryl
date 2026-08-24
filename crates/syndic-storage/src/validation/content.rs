use beryl_home_store::DomainReader;
use beryl_model::{
    ImageLabelOrdinal, SealedAssetReferenceSetProof, SyndicContentId,
    advance_sequential_marker_digest, sequential_marker_digest_seed,
};
use sha2::{Digest, Sha256};

use crate::draft_piece::DraftPieceRootsFamily;
use crate::{
    ContentEncoding, ContentLifecycle, codec::*, domain::SyndicDomain, error::SyndicValidationError,
};

use super::scan::{point, require, scan};

mod physical;
mod range;

pub(crate) use range::read_projection_text_range;

pub(crate) fn read_encoded_range(
    reader: &DomainReader<'_, SyndicDomain>,
    content: SyndicContentId,
    committed_bytes: u64,
    start: u64,
    end: u64,
) -> Result<Vec<u8>, SyndicValidationError> {
    range::read_encoded_range(reader, content, committed_bytes, start, end)
}

pub(super) fn validate(
    reader: &DomainReader<'_, SyndicDomain>,
) -> Result<(), SyndicValidationError> {
    physical::validate(reader)?;
    validate_text_spans(reader)?;
    validate_content_pieces(reader)?;
    validate_draft_references(reader)?;
    validate_accepted_references(reader)?;
    validate_canonical_references(reader)
}

fn validate_text_spans(
    reader: &DomainReader<'_, SyndicDomain>,
) -> Result<(), SyndicValidationError> {
    let mut owner = None;
    let mut expected_logical = 0_u64;
    let mut previous_piece_ordinal = 0_u64;
    let mut previous_encoded_end = 0_u64;
    scan::<ContentTextSpansFamily>(reader, |key, span| {
        if owner != Some(key.owner) {
            if let Some(previous) = owner {
                finish_text_span_owner(reader, previous, expected_logical)?;
            }
            owner = Some(key.owner);
            expected_logical = 0;
            previous_piece_ordinal = 0;
            previous_encoded_end = 0;
            let manifest = require::<ContentManifestsFamily>(
                reader,
                &key.owner,
                "content text-span owner manifest is missing",
            )?;
            if manifest.encoding() == ContentEncoding::ProviderItemV1 {
                return invariant("provider content has a generic text span");
            }
        }
        if key.owner != span.content_id()
            || key.logical_start != span.logical_start()
            || span.logical_start() != expected_logical
            || span.piece_ordinal().get() <= previous_piece_ordinal
            || span.encoded_start() < previous_encoded_end
        {
            return invariant("content text-span key or contiguous frontier disagrees");
        }
        let byte_span = require::<ContentByteSpansFamily>(
            reader,
            &ContentByteSpanKey {
                owner: span.content_id(),
                start: span.chunk_start(),
            },
            "content text-span physical byte span is missing",
        )?;
        let chunk = require::<ContentChunksFamily>(
            reader,
            &ContentChunkKey {
                owner: span.content_id(),
                ordinal: span.chunk_ordinal(),
            },
            "content text-span chunk is missing",
        )?;
        let piece = require::<ContentPiecesFamily>(
            reader,
            &ContentPieceKey {
                owner: span.content_id(),
                ordinal: span.piece_ordinal(),
            },
            "content text span piece is missing",
        )?;
        if piece != crate::ContentPieceRecord::Text(*span) {
            return invariant("content text span piece disagrees");
        }
        if byte_span.ordinal() != span.chunk_ordinal()
            || span.encoded_start() < byte_span.start()
            || span.encoded_end() > byte_span.end()
        {
            return invariant("content text span lies outside its physical chunk");
        }
        let start = usize::try_from(span.encoded_start() - byte_span.start())
            .map_err(|_| SyndicValidationError::Invariant("content text offset overflowed"))?;
        let end = usize::try_from(span.encoded_end() - byte_span.start())
            .map_err(|_| SyndicValidationError::Invariant("content text offset overflowed"))?;
        let bytes = chunk
            .bytes()
            .get(start..end)
            .ok_or(SyndicValidationError::Invariant(
                "content text span lies outside its chunk bytes",
            ))?;
        let digest: [u8; 32] = Sha256::digest(bytes).into();
        if std::str::from_utf8(bytes).is_err() || digest != span.digest() {
            return invariant("content text span digest disagrees");
        }
        expected_logical = span.logical_end();
        previous_encoded_end = span.encoded_end();
        previous_piece_ordinal = span.piece_ordinal().get();
        Ok(())
    })?;
    if let Some(owner) = owner {
        finish_text_span_owner(reader, owner, expected_logical)?;
    }
    scan::<ContentManifestsFamily>(reader, |_, manifest| {
        let first = ContentTextSpanKey {
            owner: manifest.id(),
            logical_start: 0,
        };
        if manifest.lifecycle() != ContentLifecycle::Building
            && (manifest.expected().logical_utf8_bytes() == 0)
                == point::<ContentTextSpansFamily>(reader, &first)?.is_some()
        {
            return invariant("content text zero-span frontier disagrees");
        }
        Ok(())
    })
}

fn validate_content_pieces(
    reader: &DomainReader<'_, SyndicDomain>,
) -> Result<(), SyndicValidationError> {
    let mut owner = None;
    let mut expected_ordinal = 1_u64;
    let mut previous_logical = 0_u64;
    let mut marker_since_text = false;
    let mut marker_count = 0_u64;
    let mut marker_digest = sequential_marker_digest_seed();
    let mut maximum_image_label = None;
    scan::<ContentPiecesFamily>(reader, |key, piece| {
        if owner != Some(key.owner) {
            if let Some(previous) = owner {
                finish_piece_owner(
                    reader,
                    previous,
                    expected_ordinal - 1,
                    marker_count,
                    marker_digest,
                    maximum_image_label,
                )?;
            }
            owner = Some(key.owner);
            expected_ordinal = 1;
            previous_logical = 0;
            marker_since_text = false;
            marker_count = 0;
            marker_digest = sequential_marker_digest_seed();
            maximum_image_label = None;
        }
        if key.owner != piece.content_id()
            || key.ordinal != piece.ordinal()
            || piece.ordinal().get() != expected_ordinal
            || piece.logical_offset() < previous_logical
        {
            return invariant("content piece key or contiguous order disagrees");
        }
        let manifest = require::<ContentManifestsFamily>(
            reader,
            &piece.content_id(),
            "content piece owner manifest is missing",
        )?;
        if manifest.encoding() == ContentEncoding::ProviderItemV1 {
            return invariant("provider content has a generic content piece");
        }
        if piece.encoded_end() > manifest.encoded_bytes() {
            return invariant("content piece extends beyond committed content");
        }
        match piece {
            crate::ContentPieceRecord::Text(span) => {
                let indexed = require::<ContentTextSpansFamily>(
                    reader,
                    &ContentTextSpanKey {
                        owner: span.content_id(),
                        logical_start: span.logical_start(),
                    },
                    "content text piece offset index is missing",
                )?;
                if indexed != *span || span.break_before() != marker_since_text {
                    return invariant("content text piece and offset index disagree");
                }
                previous_logical = span.logical_end();
                marker_since_text = false;
            }
            crate::ContentPieceRecord::ImageMarker {
                atom_ordinal,
                marker_ordinal,
                logical_offset,
                encoded_start,
                encoded_end,
                marker_id,
                label,
                digest,
                ..
            } => {
                if manifest.encoding() != ContentEncoding::ComposerV1
                    || atom_ordinal.get() > manifest.expected().atom_count()
                    || marker_ordinal.get()
                        != marker_count
                            .checked_add(1)
                            .ok_or(SyndicValidationError::Invariant(
                                "content marker order exhausted",
                            ))?
                    || *logical_offset > manifest.expected().logical_utf8_bytes()
                {
                    return invariant("content image-marker piece disagrees");
                }
                let encoded = range::read_encoded_range(
                    reader,
                    piece.content_id(),
                    manifest.encoded_bytes(),
                    *encoded_start,
                    *encoded_end,
                )?;
                let mut expected = Vec::with_capacity(25);
                expected.push(1);
                expected.extend_from_slice(marker_id.as_bytes());
                expected.extend_from_slice(&label.get().to_be_bytes());
                let actual_digest: [u8; 32] = Sha256::digest(&encoded).into();
                if encoded != expected || actual_digest != *digest {
                    return invariant("content image-marker bytes or digest disagree");
                }
                marker_count = marker_ordinal.get();
                marker_digest = advance_sequential_marker_digest(marker_digest, *marker_id, *label);
                maximum_image_label = Some(
                    maximum_image_label.map_or(*label, |maximum| std::cmp::max(maximum, *label)),
                );
                previous_logical = *logical_offset;
                marker_since_text = true;
            }
        }
        expected_ordinal =
            expected_ordinal
                .checked_add(1)
                .ok_or(SyndicValidationError::Invariant(
                    "content piece order exhausted",
                ))?;
        Ok(())
    })?;
    if let Some(owner) = owner {
        finish_piece_owner(
            reader,
            owner,
            expected_ordinal - 1,
            marker_count,
            marker_digest,
            maximum_image_label,
        )?;
    }
    scan::<ContentManifestsFamily>(reader, |_, manifest| {
        let first = ContentPieceKey {
            owner: manifest.id(),
            ordinal: crate::ContentPieceOrdinal::FIRST,
        };
        let has_render_piece = manifest.expected().logical_utf8_bytes() != 0
            || manifest.expected().image_marker_count() != 0;
        if manifest.lifecycle() != ContentLifecycle::Building
            && has_render_piece != point::<ContentPiecesFamily>(reader, &first)?.is_some()
        {
            return invariant("content piece zero frontier disagrees");
        }
        Ok(())
    })
}

fn finish_piece_owner(
    reader: &DomainReader<'_, SyndicDomain>,
    owner: SyndicContentId,
    piece_count: u64,
    marker_count: u64,
    marker_digest: [u8; 32],
    maximum_image_label: Option<ImageLabelOrdinal>,
) -> Result<(), SyndicValidationError> {
    let manifest = require::<ContentManifestsFamily>(
        reader,
        &owner,
        "content piece owner manifest is missing",
    )?;
    if piece_count > manifest.expected().piece_count()
        || marker_count > manifest.expected().image_marker_count()
        || (manifest.lifecycle() != ContentLifecycle::Building
            && (piece_count != manifest.expected().piece_count()
                || marker_count != manifest.expected().image_marker_count()
                || marker_digest != manifest.expected().marker_digest()
                || maximum_image_label != manifest.expected().maximum_image_label()))
    {
        return invariant("content pieces disagree with their manifest");
    }
    Ok(())
}

fn finish_text_span_owner(
    reader: &DomainReader<'_, SyndicDomain>,
    owner: SyndicContentId,
    logical_bytes: u64,
) -> Result<(), SyndicValidationError> {
    let manifest = require::<ContentManifestsFamily>(
        reader,
        &owner,
        "content text-span owner manifest is missing",
    )?;
    if logical_bytes > manifest.expected().logical_utf8_bytes()
        || (manifest.lifecycle() != ContentLifecycle::Building
            && logical_bytes != manifest.expected().logical_utf8_bytes())
    {
        return invariant("content text spans disagree with their manifest frontier");
    }
    Ok(())
}

fn validate_draft_references(
    reader: &DomainReader<'_, SyndicDomain>,
) -> Result<(), SyndicValidationError> {
    scan::<DraftsFamily>(reader, |_, draft| {
        let root = require::<DraftPieceRootsFamily>(
            reader,
            &draft.piece_root().key(),
            "draft piece root is missing",
        )?;
        if root.reference() != draft.piece_root() {
            return invariant("draft piece root reference disagrees with its immutable record");
        }
        Ok(())
    })
}

fn validate_accepted_references(
    reader: &DomainReader<'_, SyndicDomain>,
) -> Result<(), SyndicValidationError> {
    scan::<AcceptedInputsFamily>(reader, |_, input| {
        require_sealed_reference(reader, input.content(), ContentEncoding::ComposerV1)?;
        validate_asset_reference_set(input.content(), input.asset_reference_set())
    })
}

fn validate_canonical_references(
    reader: &DomainReader<'_, SyndicDomain>,
) -> Result<(), SyndicValidationError> {
    scan::<CanonicalItemsFamily>(reader, |_, item| {
        if let Some(provider) = item.provider() {
            require_canonical_provider_reference(reader, item, provider)?;
        } else if item.presentation_content().is_none() {
            return invariant("canonical item omitted all content authority");
        }
        if let Some(content) = item.presentation_content() {
            require_sealed_reference(reader, content, ContentEncoding::ComposerV1)?;
        }
        if let Some(content) = item.presentation_content() {
            validate_asset_reference_set(content, item.presentation().asset_reference_set())?;
        } else if item.presentation().asset_reference_set().is_some() {
            return invariant("provider-only canonical item carries a composer asset proof");
        }
        Ok(())
    })
}

fn require_canonical_provider_reference(
    reader: &DomainReader<'_, SyndicDomain>,
    item: &crate::CanonicalItemRecord,
    provider: &crate::SealedProviderFrameReference,
) -> Result<(), SyndicValidationError> {
    let reference = provider.content();
    let manifest = require::<ContentManifestsFamily>(
        reader,
        &reference.id(),
        "canonical provider content target is missing",
    )?;
    let lifecycle_is_valid = match manifest.lifecycle() {
        ContentLifecycle::Live => true,
        ContentLifecycle::Finalized => provider.stream_state().is_complete(),
        ContentLifecycle::Building | ContentLifecycle::Sealed => false,
    };
    let valid = reference.encoding() == ContentEncoding::ProviderItemV1
        && manifest.encoding() == ContentEncoding::ProviderItemV1
        && manifest.owner() == Some(item.id())
        && lifecycle_is_valid
        && manifest.current_reference() == Some(reference)
        && item.provider_content() == Some(reference);
    if !valid {
        return invariant("canonical provider does not select one exact published manifest");
    }
    Ok(())
}

fn require_sealed_reference(
    reader: &DomainReader<'_, SyndicDomain>,
    reference: crate::ContentReference,
    encoding: ContentEncoding,
) -> Result<(), SyndicValidationError> {
    let manifest = require::<ContentManifestsFamily>(
        reader,
        &reference.id(),
        "content reference target is missing",
    )?;
    if encoding != reference.encoding()
        || manifest.lifecycle() != ContentLifecycle::Sealed
        || manifest.sealed_reference() != Some(reference)
    {
        return invariant("content reference does not select one exact sealed manifest");
    }
    Ok(())
}

fn validate_asset_reference_set(
    content: crate::ContentReference,
    proof: Option<SealedAssetReferenceSetProof>,
) -> Result<(), SyndicValidationError> {
    let expected = content
        .sealed_marker_summary()
        .map_err(|_| SyndicValidationError::Invariant("content marker summary is invalid"))?;
    match (content.summary().image_marker_count(), proof) {
        (0, None) => Ok(()),
        (0, Some(_)) | (_, None) => {
            invariant("content and optional asset-reference proof disagree")
        }
        (_, Some(proof)) if proof.sequential() == expected.sequential() => Ok(()),
        (_, Some(_)) => invariant("asset-reference proof source disagrees with content"),
    }
}

fn invariant<T>(message: &'static str) -> Result<T, SyndicValidationError> {
    Err(SyndicValidationError::Invariant(message))
}
