use super::*;

pub(super) fn validate_build_identity(
    key: DraftComposerBuildKeyV1,
    build: &DraftComposerBuildRecordV1,
) -> Result<(), DraftComposerMaterializationErrorV1> {
    let encoder = build.encoder();
    let records = build.records();
    let source = key.source().summary();
    let records_are_empty = records == empty_record_frontier();
    let encoder_piece_limit = encoder
        .logical_utf8_bytes()
        .checked_add(encoder.marker_count());
    let record_piece_limit = records
        .logical_utf8_bytes()
        .checked_add(records.marker_count());
    if build.local_shape_error().is_some()
        || build.key() != key
        || build.encoder().carry().len() > DRAFT_COMPOSER_CARRY_MAX_BYTES
        || encoder.cursor().piece_index() > source.piece_count()
        || (encoder.cursor().piece_index() == source.piece_count()
            && encoder.cursor().atom_encoded_offset() != 0)
        || encoder.source_piece_count() > source.piece_count()
        || encoder.source_piece_count() != encoder.cursor().piece_index()
        || encoder.logical_utf8_bytes() > source.logical_utf8_bytes()
        || encoder.marker_count() > source.marker_count()
        || encoder.marker_count() > encoder.source_piece_count()
        || encoder.marker_count() > encoder.piece_count()
        || !encoder_piece_limit.is_some_and(|limit| encoder.piece_count() <= limit)
        || encoder.active_text_span_encoded_start().is_some()
            != encoder.active_text_span_logical_start().is_some()
        || records.cursor().piece_index() > source.piece_count()
        || records.logical_utf8_bytes() > source.logical_utf8_bytes()
        || records.marker_count() > source.marker_count()
        || records.marker_count() > records.piece_count()
        || !record_piece_limit.is_some_and(|limit| records.piece_count() <= limit)
        || records.cursor().piece_index() > encoder.cursor().piece_index()
        || records.encoded_bytes() > build.output_encoded_bytes().max(9)
        || records.chunk_start() > build.output_encoded_bytes()
        || records.chunk_ordinal() == 0
    {
        return Err(DraftComposerMaterializationErrorV1::BuildCollision);
    }
    match (build.output(), build.output_revision()) {
        (Some(output), Some(revision)) => {
            if output.id() != content_id_for_summary(output.summary())
                || output.revision() != revision
                || output.encoding() != ContentEncoding::ComposerV1
                || output.summary().atom_count() != key.source().summary().piece_count()
                || output.summary().logical_utf8_bytes()
                    != key.source().summary().logical_utf8_bytes()
                || output.summary().image_marker_count() != key.source().summary().marker_count()
                || encoder.chunk_count() != build.output_chunk_count()
                || build.output_chunk_count() > output.summary().chunk_count()
                || build.output_encoded_bytes() > output.summary().encoded_bytes()
                || encoder.encoded_bytes() < build.output_encoded_bytes()
                || encoder.chain_digest() != build.output_chain_digest()
                || records.chunk_ordinal() > build.output_chunk_count().saturating_add(1)
            {
                return Err(DraftComposerMaterializationErrorV1::BuildCollision);
            }
        }
        (None, None) => {
            if build.output_chunk_count() != 0
                || build.output_encoded_bytes() != 0
                || build.output_chain_digest() != content_chain_seed(ContentEncoding::ComposerV1)
                || !records_are_empty
            {
                return Err(DraftComposerMaterializationErrorV1::BuildCollision);
            }
        }
        _ => return Err(DraftComposerMaterializationErrorV1::BuildCollision),
    }
    let output_frontier = build.output_encoded_bytes().max(9);
    match build.lifecycle() {
        DraftComposerBuildLifecycleV1::Open(DraftComposerBuildPhaseV1::Planning) => {
            let flushed = encoder
                .encoded_bytes()
                .checked_sub(encoder.carry().len() as u64);
            let maximum_flushed = encoder
                .chunk_count()
                .checked_mul(DRAFT_COMPOSER_CARRY_MAX_BYTES as u64);
            let chunk_frontier_valid = match (encoder.chunk_count(), flushed, maximum_flushed) {
                (0, Some(0), Some(0)) => {
                    encoder.chain_digest() == content_chain_seed(ContentEncoding::ComposerV1)
                        && !encoder.carry().is_empty()
                }
                (count, Some(bytes), Some(maximum)) if count != 0 => {
                    bytes >= count
                        && bytes <= maximum
                        && encoder.chain_digest() != content_chain_seed(ContentEncoding::ComposerV1)
                }
                _ => false,
            };
            if build.output().is_some()
                || !records_are_empty
                || encoder.chunk_count() > encoder.piece_count()
                || !chunk_frontier_valid
            {
                return Err(DraftComposerMaterializationErrorV1::BuildCollision);
            }
        }
        DraftComposerBuildLifecycleV1::Open(DraftComposerBuildPhaseV1::Writing) => {
            if build.output().is_none()
                || records.encoded_bytes() != output_frontier
                || records.chunk_start() != build.output_encoded_bytes()
                || records.chunk_ordinal() != build.output_chunk_count().saturating_add(1)
            {
                return Err(DraftComposerMaterializationErrorV1::BuildCollision);
            }
        }
        DraftComposerBuildLifecycleV1::Open(DraftComposerBuildPhaseV1::Draining {
            final_chunk,
        }) => {
            if build.output().is_none()
                || !encoder.carry().is_empty()
                || encoder.encoded_bytes() != build.output_encoded_bytes()
                || records.encoded_bytes() > build.output_encoded_bytes()
                || (records.encoded_bytes() == build.output_encoded_bytes()
                    && (records.cursor().piece_index() != source.piece_count()
                        || records.cursor().atom_encoded_offset() != 0))
                || records.chunk_ordinal() != build.output_chunk_count()
                || *final_chunk
                    != (encoder.cursor().piece_index() == source.piece_count()
                        && encoder.cursor().atom_encoded_offset() == 0)
            {
                return Err(DraftComposerMaterializationErrorV1::BuildCollision);
            }
        }
        DraftComposerBuildLifecycleV1::Open(DraftComposerBuildPhaseV1::ReadyToSeal)
        | DraftComposerBuildLifecycleV1::Sealed(_) => {
            let Some(output) = build.output() else {
                return Err(DraftComposerMaterializationErrorV1::BuildCollision);
            };
            if !encoder.carry().is_empty()
                || encoder.cursor().piece_index() != source.piece_count()
                || encoder.source_piece_count() != source.piece_count()
                || records.cursor().piece_index() != source.piece_count()
                || encoder.encoded_bytes() != output.summary().encoded_bytes()
                || encoder.logical_utf8_bytes() != output.summary().logical_utf8_bytes()
                || encoder.piece_count() != output.summary().piece_count()
                || encoder.marker_count() != output.summary().image_marker_count()
                || encoder.marker_digest() != output.summary().marker_digest()
                || encoder.maximum_image_label() != output.summary().maximum_image_label()
                || records.encoded_bytes() != output.summary().encoded_bytes()
                || records.logical_utf8_bytes() != output.summary().logical_utf8_bytes()
                || records.piece_count() != output.summary().piece_count()
                || records.marker_count() != output.summary().image_marker_count()
                || records.marker_digest() != output.summary().marker_digest()
                || records.maximum_image_label() != output.summary().maximum_image_label()
                || build.output_chunk_count() != output.summary().chunk_count()
                || build.output_encoded_bytes() != output.summary().encoded_bytes()
                || build.output_chain_digest() != output.summary().digest()
            {
                return Err(DraftComposerMaterializationErrorV1::BuildCollision);
            }
            if let DraftComposerBuildLifecycleV1::Sealed(reference) = build.lifecycle()
                && *reference != output
            {
                return Err(DraftComposerMaterializationErrorV1::BuildCollision);
            }
        }
        DraftComposerBuildLifecycleV1::Cancelled
        | DraftComposerBuildLifecycleV1::Failed(_)
        | DraftComposerBuildLifecycleV1::Superseded(_) => {
            let reachable = [
                DraftComposerBuildPhaseV1::Planning,
                DraftComposerBuildPhaseV1::Writing,
                DraftComposerBuildPhaseV1::Draining { final_chunk: false },
                DraftComposerBuildPhaseV1::Draining { final_chunk: true },
                DraftComposerBuildPhaseV1::ReadyToSeal,
            ]
            .into_iter()
            .any(|phase| {
                copy_build(
                    build,
                    build.encoder().clone(),
                    build.records(),
                    build.output(),
                    build.output_revision(),
                    build.output_chunk_count(),
                    build.output_encoded_bytes(),
                    build.output_chain_digest(),
                    DraftComposerBuildLifecycleV1::Open(phase),
                )
                .local_shape_error()
                .is_none()
            });
            if !reachable {
                return Err(DraftComposerMaterializationErrorV1::BuildCollision);
            }
        }
    }
    Ok(())
}

pub(super) fn validate_mapping(
    key: DraftComposerMaterializationKeyV1,
    mapping: DraftComposerMaterializationRecordV1,
) -> Result<(), DraftComposerMaterializationErrorV1> {
    if mapping.key() != key
        || mapping.source_digest() != key.source().combined_digest()
        || mapping.source_piece_count() != key.source().summary().piece_count()
        || mapping.source_utf8_bytes() != key.source().summary().logical_utf8_bytes()
        || mapping.source_marker_count() != key.source().summary().marker_count()
        || mapping.content().encoding() != ContentEncoding::ComposerV1
    {
        return Err(DraftComposerMaterializationErrorV1::MappingCollision);
    }
    Ok(())
}

pub(super) fn validate_sealed_mapping_closure(
    storage: &SyndicStorage,
    store: &HomeStore,
    key: DraftComposerMaterializationKeyV1,
    mapping: DraftComposerMaterializationRecordV1,
) -> Result<(), DraftComposerMaterializationErrorV1> {
    validate_mapping(key, mapping)?;
    let source = storage
        .point::<crate::draft_piece::codec::DraftPieceRootsFamily>(
            store,
            key.source().key(),
            storage_point_limit::<crate::draft_piece::codec::DraftPieceRootsFamily>(),
        )?
        .ok_or(DraftComposerMaterializationErrorV1::InvalidBuild)?;
    if source.reference() != key.source() {
        return Err(DraftComposerMaterializationErrorV1::InvalidBuild);
    }
    let _ = read_materialization_page(storage, store, key.source(), 0, 1, 65_536)?;
    let origin_key =
        DraftComposerBuildKeyV1::new(key.source(), key.format(), mapping.sealing_operation());
    let origin = storage
        .point::<DraftComposerBuildsFamily>(
            store,
            origin_key,
            storage_point_limit::<DraftComposerBuildsFamily>(),
        )?
        .ok_or(DraftComposerMaterializationErrorV1::InvalidBuild)?;
    validate_build_identity(origin_key, &origin)?;
    validate_output_frontier_records(storage, store, &origin)?;
    if origin.lifecycle() != &DraftComposerBuildLifecycleV1::Sealed(mapping.content())
        || origin.output() != Some(mapping.content())
        || mapping.content().id() != content_id_for_summary(mapping.content().summary())
    {
        return Err(DraftComposerMaterializationErrorV1::InvalidBuild);
    }
    validate_sealed_content(storage, store, mapping.content())
}

pub(super) fn validate_build_manifest(
    build: &DraftComposerBuildRecordV1,
    manifest: &ContentManifestRecord,
) -> Result<(), DraftComposerMaterializationErrorV1> {
    let reference = build
        .output()
        .ok_or(DraftComposerMaterializationErrorV1::InvalidBuild)?;
    if manifest.id() != reference.id()
        || manifest.owner().is_some()
        || manifest.encoding() != ContentEncoding::ComposerV1
        || !matches!(
            manifest.lifecycle(),
            ContentLifecycle::Building | ContentLifecycle::Sealed
        )
        || manifest.expected() != reference.summary()
        || manifest.revision()
            < build
                .output_revision()
                .ok_or(DraftComposerMaterializationErrorV1::InvalidBuild)?
        || manifest.chunk_count() < build.output_chunk_count()
        || manifest.encoded_bytes() < build.output_encoded_bytes()
        || manifest.chunk_count() > reference.summary().chunk_count()
        || manifest.encoded_bytes() > reference.summary().encoded_bytes()
        || (manifest.chunk_count() == build.output_chunk_count()
            && (manifest.encoded_bytes() != build.output_encoded_bytes()
                || manifest.chain_digest() != build.output_chain_digest()))
        || (manifest.chunk_count() > build.output_chunk_count()
            && manifest.encoded_bytes() <= build.output_encoded_bytes())
        || (manifest.lifecycle() == ContentLifecycle::Sealed
            && (manifest.chunk_count() != reference.summary().chunk_count()
                || manifest.encoded_bytes() != reference.summary().encoded_bytes()
                || manifest.chain_digest() != reference.summary().digest()))
    {
        return Err(DraftComposerMaterializationErrorV1::InvalidOutput);
    }
    Ok(())
}

pub(super) fn validate_sealed_content(
    storage: &SyndicStorage,
    store: &HomeStore,
    reference: ContentReference,
) -> Result<(), DraftComposerMaterializationErrorV1> {
    let manifest = storage
        .point::<ContentManifestsFamily>(
            store,
            reference.id(),
            storage_point_limit::<ContentManifestsFamily>(),
        )?
        .ok_or(DraftComposerMaterializationErrorV1::InvalidOutput)?;
    if manifest.sealed_reference() != Some(reference)
        || manifest.owner().is_some()
        || manifest.chunk_count() != reference.summary().chunk_count()
        || manifest.encoded_bytes() != reference.summary().encoded_bytes()
        || manifest.chain_digest() != reference.summary().digest()
    {
        return Err(DraftComposerMaterializationErrorV1::InvalidOutput);
    }
    Ok(())
}

pub(super) fn validate_output_frontier_records(
    storage: &SyndicStorage,
    store: &HomeStore,
    build: &DraftComposerBuildRecordV1,
) -> Result<(), DraftComposerMaterializationErrorV1> {
    let Some(output) = build.output() else {
        return Ok(());
    };
    if build.output_chunk_count() != 0 {
        let ordinal = ContentChunkOrdinal::new(build.output_chunk_count())
            .map_err(|_| DraftComposerMaterializationErrorV1::InvalidOutput)?;
        let chunk = storage
            .point::<ContentChunksFamily>(
                store,
                ContentChunkKey {
                    owner: output.id(),
                    ordinal,
                },
                storage_point_limit::<ContentChunksFamily>(),
            )?
            .ok_or(DraftComposerMaterializationErrorV1::InvalidOutput)?;
        let length = u64::try_from(chunk.bytes().len())
            .map_err(|_| DraftComposerMaterializationErrorV1::LengthOverflow)?;
        let start = build
            .output_encoded_bytes()
            .checked_sub(length)
            .ok_or(DraftComposerMaterializationErrorV1::InvalidOutput)?;
        let span = storage
            .point::<ContentByteSpansFamily>(
                store,
                ContentByteSpanKey {
                    owner: output.id(),
                    start,
                },
                storage_point_limit::<ContentByteSpansFamily>(),
            )?
            .ok_or(DraftComposerMaterializationErrorV1::InvalidOutput)?;
        let expected = ContentByteSpanRecord::for_chunk(&chunk, start)
            .map_err(|_| DraftComposerMaterializationErrorV1::InvalidOutput)?;
        if span != expected || span.end() != build.output_encoded_bytes() {
            return Err(DraftComposerMaterializationErrorV1::InvalidOutput);
        }
    }
    if build.records().piece_count() != 0 {
        let ordinal = ContentPieceOrdinal::new(build.records().piece_count())
            .map_err(|_| DraftComposerMaterializationErrorV1::InvalidOutput)?;
        let piece = storage
            .point::<ContentPiecesFamily>(
                store,
                ContentPieceKey {
                    owner: output.id(),
                    ordinal,
                },
                storage_point_limit::<ContentPiecesFamily>(),
            )?
            .ok_or(DraftComposerMaterializationErrorV1::InvalidOutput)?;
        if piece.content_id() != output.id()
            || piece.ordinal() != ordinal
            || piece.encoded_end() > build.records().encoded_bytes()
        {
            return Err(DraftComposerMaterializationErrorV1::InvalidOutput);
        }
        if let ContentPieceRecord::Text(span) = piece {
            let stored = storage
                .point::<ContentTextSpansFamily>(
                    store,
                    ContentTextSpanKey {
                        owner: output.id(),
                        logical_start: span.logical_start(),
                    },
                    storage_point_limit::<ContentTextSpansFamily>(),
                )?
                .ok_or(DraftComposerMaterializationErrorV1::InvalidOutput)?;
            if stored != span {
                return Err(DraftComposerMaterializationErrorV1::InvalidOutput);
            }
        }
    }
    Ok(())
}
