use beryl_home_store::DomainReader;
use beryl_model::SyndicContentId;
use sha2::{Digest, Sha256};

use crate::{
    CanonicalItemKind, ContentEncoding, ContentLifecycle, InputMarkerOwner, advance_content_chain,
    codec::*, content::input_marker_digest, content_chain_seed, domain::SyndicDomain,
    error::SyndicValidationError,
};

use super::scan::{point, require, scan};

mod range;

pub(super) use range::read_logical_range;

pub(super) fn validate(
    reader: &DomainReader<'_, SyndicDomain>,
) -> Result<(), SyndicValidationError> {
    validate_manifests(reader)?;
    validate_chunks(reader)?;
    validate_byte_spans(reader)?;
    validate_text_spans(reader)?;
    validate_content_pieces(reader)?;
    validate_draft_references(reader)?;
    validate_accepted_references(reader)?;
    validate_canonical_references(reader)?;
    validate_marker_resolutions(reader)
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
    let mut markers = Vec::new();
    scan::<ContentPiecesFamily>(reader, |key, piece| {
        if owner != Some(key.owner) {
            if let Some(previous) = owner {
                finish_piece_owner(reader, previous, expected_ordinal - 1, &markers)?;
            }
            owner = Some(key.owner);
            expected_ordinal = 1;
            previous_logical = 0;
            marker_since_text = false;
            markers.clear();
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
                        != u64::try_from(markers.len())
                            .ok()
                            .and_then(|count| count.checked_add(1))
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
                markers.push((*marker_id, *label));
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
        finish_piece_owner(reader, owner, expected_ordinal - 1, &markers)?;
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
    markers: &[(beryl_model::SyndicDraftMarkerId, crate::ImageLabelOrdinal)],
) -> Result<(), SyndicValidationError> {
    let manifest = require::<ContentManifestsFamily>(
        reader,
        &owner,
        "content piece owner manifest is missing",
    )?;
    if piece_count > manifest.expected().piece_count()
        || u64::try_from(markers.len()).ok() > Some(manifest.expected().image_marker_count())
        || (manifest.lifecycle() != ContentLifecycle::Building
            && (piece_count != manifest.expected().piece_count()
                || u64::try_from(markers.len()).ok()
                    != Some(manifest.expected().image_marker_count())
                || input_marker_digest(markers.iter().copied())
                    != manifest.expected().marker_digest()))
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

fn validate_byte_spans(
    reader: &DomainReader<'_, SyndicDomain>,
) -> Result<(), SyndicValidationError> {
    let mut owner = None;
    let mut expected_start = 0_u64;
    let mut expected_ordinal = 1_u64;
    scan::<ContentByteSpansFamily>(reader, |key, span| {
        if owner != Some(key.owner) {
            if let Some(previous) = owner {
                finish_span_owner(reader, previous, expected_start, expected_ordinal - 1)?;
            }
            owner = Some(key.owner);
            expected_start = 0;
            expected_ordinal = 1;
        }
        if key.owner != span.content_id()
            || key.start != span.start()
            || span.start() != expected_start
            || span.ordinal().get() != expected_ordinal
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
    if let Some(owner) = owner {
        finish_span_owner(reader, owner, expected_start, expected_ordinal - 1)?;
    }
    scan::<ContentManifestsFamily>(reader, |_, manifest| {
        let first = ContentByteSpanKey {
            owner: manifest.id(),
            start: 0,
        };
        if (manifest.chunk_count() == 0)
            == point::<ContentByteSpansFamily>(reader, &first)?.is_some()
        {
            return invariant("content zero-span frontier disagrees");
        }
        Ok(())
    })
}

fn finish_span_owner(
    reader: &DomainReader<'_, SyndicDomain>,
    owner: SyndicContentId,
    encoded_bytes: u64,
    chunk_count: u64,
) -> Result<(), SyndicValidationError> {
    let manifest = require::<ContentManifestsFamily>(
        reader,
        &owner,
        "content byte-span owner manifest is missing",
    )?;
    if manifest.encoded_bytes() != encoded_bytes || manifest.chunk_count() != chunk_count {
        return invariant("content byte spans disagree with their manifest frontier");
    }
    Ok(())
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
            || expected.image_marker_count() > crate::record::MAX_COMPOSER_IMAGE_MARKERS as u64
        {
            return invariant("content manifest identity or frontier is invalid");
        }
        match (manifest.owner(), manifest.lifecycle()) {
            (None, ContentLifecycle::Building | ContentLifecycle::Sealed)
                if manifest.id() == SyndicContentId::from_digest(*expected.digest().as_bytes()) => {
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
                        == crate::content::input_marker_digest(std::iter::empty()) => {}
            _ => return invariant("content manifest ownership or lifecycle is invalid"),
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

fn validate_chunks(reader: &DomainReader<'_, SyndicDomain>) -> Result<(), SyndicValidationError> {
    let mut owner = None;
    let mut expected_ordinal = 1_u64;
    let mut observed_bytes = 0_u64;
    let mut chain = None;
    scan::<ContentChunksFamily>(reader, |key, chunk| {
        if owner != Some(key.owner) {
            if let Some(previous) = owner {
                finish_chunk_owner(
                    reader,
                    previous,
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
            chain = Some(content_chain_seed(manifest.encoding()));
        }
        if key.owner != chunk.content_id()
            || key.ordinal != chunk.ordinal()
            || key.ordinal.get() != expected_ordinal
        {
            return invariant("content chunk key or contiguous order disagrees");
        }
        let manifest = require::<ContentManifestsFamily>(
            reader,
            &key.owner,
            "content chunk owner manifest is missing",
        )?;
        if key.ordinal.get() > manifest.chunk_count() {
            return invariant("content chunk extends beyond its committed frontier");
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
    if let Some(owner) = owner {
        finish_chunk_owner(
            reader,
            owner,
            expected_ordinal - 1,
            observed_bytes,
            chain.expect("owner chain exists"),
        )?;
    }
    scan::<ContentManifestsFamily>(reader, |_, manifest| {
        let first = ContentChunkKey {
            owner: manifest.id(),
            ordinal: crate::ContentChunkOrdinal::FIRST,
        };
        if (manifest.chunk_count() == 0) == point::<ContentChunksFamily>(reader, &first)?.is_some()
        {
            return invariant("content zero-chunk frontier disagrees");
        }
        Ok(())
    })
}

fn finish_chunk_owner(
    reader: &DomainReader<'_, SyndicDomain>,
    owner: SyndicContentId,
    chunk_count: u64,
    encoded_bytes: u64,
    chain: beryl_model::SyndicContentDigest,
) -> Result<(), SyndicValidationError> {
    let manifest = require::<ContentManifestsFamily>(
        reader,
        &owner,
        "content chunk owner manifest is missing",
    )?;
    if manifest.chunk_count() != chunk_count
        || manifest.encoded_bytes() != encoded_bytes
        || manifest.chain_digest() != chain
    {
        return invariant("content chunks disagree with their manifest frontier");
    }
    Ok(())
}

fn validate_draft_references(
    reader: &DomainReader<'_, SyndicDomain>,
) -> Result<(), SyndicValidationError> {
    scan::<DraftsFamily>(reader, |_, draft| {
        require_sealed_reference(reader, draft.content(), ContentEncoding::ComposerV1)
    })
}

fn validate_accepted_references(
    reader: &DomainReader<'_, SyndicDomain>,
) -> Result<(), SyndicValidationError> {
    scan::<AcceptedInputsFamily>(reader, |_, input| {
        require_sealed_reference(reader, input.content(), ContentEncoding::ComposerV1)?;
        if input.marker_count() != input.content().summary().image_marker_count() {
            return invariant("accepted-input marker count disagrees with content");
        }
        validate_marker_presence(
            reader,
            InputMarkerOwner::AcceptedInput(input.id()),
            input.marker_count(),
        )
    })
}

fn validate_canonical_references(
    reader: &DomainReader<'_, SyndicDomain>,
) -> Result<(), SyndicValidationError> {
    scan::<CanonicalItemsFamily>(reader, |_, item| {
        match (item.kind(), item.payload().content()) {
            (CanonicalItemKind::UserInput, Some(content)) => {
                require_sealed_reference(reader, content, ContentEncoding::ComposerV1)?;
            }
            (
                CanonicalItemKind::AssistantMessage(_)
                | CanonicalItemKind::ProviderText(_)
                | CanonicalItemKind::Operational(_),
                Some(_),
            ) => require_canonical_text_reference(reader, item)?,
            (
                CanonicalItemKind::Activity(_)
                | CanonicalItemKind::GeneratedMedia
                | CanonicalItemKind::Unsupported(_),
                None,
            ) => {}
            _ => return invariant("canonical-item kind and content authority disagree"),
        }
        let marker_count = item.payload().marker_count();
        let content_markers = item
            .payload()
            .content()
            .map_or(0, |content| content.summary().image_marker_count());
        if marker_count != content_markers {
            return invariant("canonical-item marker count disagrees with content");
        }
        validate_marker_presence(
            reader,
            InputMarkerOwner::CanonicalItem(item.id()),
            marker_count,
        )
    })
}

fn require_canonical_text_reference(
    reader: &DomainReader<'_, SyndicDomain>,
    item: &crate::CanonicalItemRecord,
) -> Result<(), SyndicValidationError> {
    let reference = item
        .payload()
        .content()
        .ok_or(SyndicValidationError::Invariant(
            "canonical text item omitted content authority",
        ))?;
    let manifest = require::<ContentManifestsFamily>(
        reader,
        &reference.id(),
        "canonical text content target is missing",
    )?;
    let valid = reference.encoding() == ContentEncoding::Utf8V1
        && manifest.current_reference() == Some(reference)
        && match manifest.lifecycle() {
            ContentLifecycle::Sealed => manifest.owner().is_none(),
            ContentLifecycle::Live | ContentLifecycle::Finalized => {
                manifest.owner() == Some(item.id())
            }
            ContentLifecycle::Building => false,
        };
    if !valid {
        return invariant("canonical text does not select one exact published manifest");
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

fn validate_marker_presence(
    reader: &DomainReader<'_, SyndicDomain>,
    owner: InputMarkerOwner,
    count: u64,
) -> Result<(), SyndicValidationError> {
    let first = InputMarkerKey {
        owner,
        ordinal: crate::InputMarkerOrdinal::FIRST,
    };
    if (count == 0) == point::<InputMarkerResolutionsFamily>(reader, &first)?.is_some() {
        return invariant("input marker zero frontier disagrees");
    }
    Ok(())
}

fn validate_marker_resolutions(
    reader: &DomainReader<'_, SyndicDomain>,
) -> Result<(), SyndicValidationError> {
    let mut owner = None;
    let mut expected = 1_u64;
    let mut markers = Vec::new();
    scan::<InputMarkerResolutionsFamily>(reader, |key, resolution| {
        if owner != Some(key.owner) {
            if let Some(previous) = owner {
                validate_marker_owner(reader, previous, expected - 1, &markers)?;
            }
            owner = Some(key.owner);
            expected = 1;
            markers.clear();
        }
        if key.owner != resolution.owner()
            || key.ordinal != resolution.ordinal()
            || key.ordinal.get() != expected
        {
            return invariant("input marker key or contiguous order disagrees");
        }
        let marker = resolution.marker();
        markers.push((marker.marker_id(), marker.label()));
        expected = expected
            .checked_add(1)
            .ok_or(SyndicValidationError::Invariant(
                "input marker order exhausted",
            ))?;
        Ok(())
    })?;
    if let Some(owner) = owner {
        validate_marker_owner(reader, owner, expected - 1, &markers)?;
    }
    Ok(())
}

fn validate_marker_owner(
    reader: &DomainReader<'_, SyndicDomain>,
    owner: InputMarkerOwner,
    observed: u64,
    markers: &[(beryl_model::SyndicDraftMarkerId, crate::ImageLabelOrdinal)],
) -> Result<(), SyndicValidationError> {
    let content = match owner {
        InputMarkerOwner::AcceptedInput(id) => require::<AcceptedInputsFamily>(
            reader,
            &id,
            "input marker owner accepted input is missing",
        )?
        .content(),
        InputMarkerOwner::CanonicalItem(id) => require::<CanonicalItemsFamily>(
            reader,
            &id,
            "input marker owner canonical item is missing",
        )?
        .payload()
        .content()
        .ok_or(SyndicValidationError::Invariant(
            "canonical input marker owner omitted content authority",
        ))?,
    };
    if content.summary().image_marker_count() != observed {
        return invariant("input marker frontier disagrees with its owner");
    }
    if input_marker_digest(markers.iter().copied()) != content.summary().marker_digest() {
        return invariant("input marker identities disagree with owner content");
    }
    Ok(())
}

fn invariant<T>(message: &'static str) -> Result<T, SyndicValidationError> {
    Err(SyndicValidationError::Invariant(message))
}
