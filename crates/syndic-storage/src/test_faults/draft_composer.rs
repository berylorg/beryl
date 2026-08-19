use std::convert::Infallible;

use beryl_home_store::{
    DomainMutation, DomainReader, HomeStore, MutationBuilder, MutationContribution,
    ReconciliationReservation,
};
use beryl_model::{SyndicContentDigest, SyndicContentId};

use crate::{codec::*, domain::SyndicDomain, draft_piece::*, *};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftComposerBuildCorruption {
    Cursor,
    CarryFrontier,
    MarkerFrontier,
    PlanningSourceOverflow,
    PlanningCursorMismatch,
    PlanningEofMismatch,
    PlanningPieceMaximum,
    PlanningMaximum,
    PlanningDigestCount,
    TerminalPlanningMaximum,
    TerminalFrontier,
    OutputSummary,
    SealedLifecycle,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftComposerOutputCorruption {
    Chunk,
    ByteSpan,
    TextSpan,
    Piece,
}

pub fn draft_composer_build_truncation_is_rejected(
    store: &HomeStore,
    storage: SyndicStorage,
    key: DraftComposerBuildKeyV1,
) -> bool {
    let Ok(Some(build)) = storage.point::<DraftComposerBuildsFamily>(
        store,
        key,
        SyndicPointReadLimit::new(65_536).unwrap(),
    ) else {
        return false;
    };
    let Ok(mut encoded) = DraftComposerBuildsFamily::encode_value(&build) else {
        return false;
    };
    encoded.pop();
    DraftComposerBuildsFamily::decode_value(&encoded).is_err()
}

pub fn draft_composer_provisional_output(
    store: &HomeStore,
    storage: SyndicStorage,
    key: DraftComposerBuildKeyV1,
) -> Option<ContentReference> {
    storage
        .point::<DraftComposerBuildsFamily>(store, key, SyndicPointReadLimit::new(65_536).unwrap())
        .ok()
        .flatten()
        .and_then(|build| build.output())
}

pub fn draft_composer_full_carry_remaining_bytes(
    store: &HomeStore,
    storage: SyndicStorage,
    key: DraftComposerBuildKeyV1,
) -> Option<u64> {
    let build = storage
        .point::<DraftComposerBuildsFamily>(store, key, SyndicPointReadLimit::new(65_536).ok()?)
        .ok()??;
    if !matches!(
        build.lifecycle(),
        DraftComposerBuildLifecycleV1::Open(DraftComposerBuildPhaseV1::Writing)
    ) || build.encoder().carry().len() != DRAFT_COMPOSER_CARRY_MAX_BYTES
        || build.output_encoded_bytes() != DRAFT_COMPOSER_CARRY_MAX_BYTES as u64
        || build.local_shape_error().is_some()
    {
        return None;
    }
    build
        .output()?
        .summary()
        .encoded_bytes()
        .checked_sub(build.encoder().encoded_bytes())
}

pub fn draft_composer_terminal_build_encoded_size(
    store: &HomeStore,
    storage: SyndicStorage,
    key: DraftComposerBuildKeyV1,
    successor: DraftComposerMaterializationOperationIdV1,
) -> Option<usize> {
    let build = storage
        .point::<DraftComposerBuildsFamily>(store, key, SyndicPointReadLimit::new(65_536).ok()?)
        .ok()??;
    if build.lifecycle() != &DraftComposerBuildLifecycleV1::Superseded(successor)
        || build.local_shape_error().is_some()
    {
        return None;
    }
    let encoded = DraftComposerBuildsFamily::encode_value(&build).ok()?;
    (DraftComposerBuildsFamily::decode_value(&encoded).ok()? == build).then_some(encoded.len())
}

pub fn draft_composer_terminal_build_has_maximal_shape(
    store: &HomeStore,
    storage: SyndicStorage,
    key: DraftComposerBuildKeyV1,
    successor: DraftComposerMaterializationOperationIdV1,
) -> bool {
    let Ok(Some(build)) = storage.point::<DraftComposerBuildsFamily>(
        store,
        key,
        SyndicPointReadLimit::new(65_536).unwrap(),
    ) else {
        return false;
    };
    let Some(output) = build.output() else {
        return false;
    };
    build.lifecycle() == &DraftComposerBuildLifecycleV1::Superseded(successor)
        && key.source().root_node().is_some()
        && key.source().marker_index_root().is_some()
        && build.encoder().carry().len() == DRAFT_COMPOSER_CARRY_MAX_BYTES
        && build.encoder().maximum_image_label() == Some(ImageLabelOrdinal::new(u64::MAX).unwrap())
        && build.records().maximum_image_label() == Some(ImageLabelOrdinal::new(u64::MAX).unwrap())
        && output.summary().maximum_image_label() == Some(ImageLabelOrdinal::new(u64::MAX).unwrap())
        && build.output_revision().is_some()
        && build.local_shape_error().is_none()
}

pub fn draft_composer_mapping_truncation_is_rejected(
    mapping: &DraftComposerMaterializationRecordV1,
) -> bool {
    let Ok(mut encoded) = DraftComposerMaterializationsFamily::encode_value(mapping) else {
        return false;
    };
    encoded.pop();
    DraftComposerMaterializationsFamily::decode_value(&encoded).is_err()
}

#[derive(Clone)]
enum Replacement {
    Build(DraftComposerBuildKeyV1, DraftComposerBuildRecordV1),
    Mapping(
        DraftComposerMaterializationKeyV1,
        DraftComposerMaterializationRecordV1,
    ),
    Manifest(SyndicContentId, ContentManifestRecord),
    Chunk(ContentChunkKey, ContentChunkRecord),
    DeleteSource(DraftPieceRootKeyV1),
    DeleteBuild(DraftComposerBuildKeyV1),
    DeleteByteSpan(ContentByteSpanKey),
    DeleteTextSpan(ContentTextSpanKey),
    DeletePiece(ContentPieceKey),
}

#[derive(Clone)]
struct Replace(Replacement);

pub fn inject_draft_composer_build_corruption(
    store: &HomeStore,
    storage: SyndicStorage,
    key: DraftComposerBuildKeyV1,
    corruption: DraftComposerBuildCorruption,
) -> MutationContribution {
    let build = storage
        .point::<DraftComposerBuildsFamily>(store, key, SyndicPointReadLimit::new(66_560).unwrap())
        .unwrap()
        .unwrap();
    let encoder = match corruption {
        DraftComposerBuildCorruption::Cursor => DraftComposerEncoderStateV1::new(
            DraftComposerSourceCursorV1::new(key.source().summary().piece_count() + 1, 0),
            build.encoder().source_piece_count(),
            build.encoder().encoded_bytes(),
            build.encoder().logical_utf8_bytes(),
            build.encoder().chunk_count(),
            build.encoder().piece_count(),
            build.encoder().marker_count(),
            build.encoder().marker_digest(),
            build.encoder().maximum_image_label(),
            build.encoder().chain_digest(),
            build.encoder().carry().to_vec(),
            build.encoder().break_before(),
            build.encoder().active_text_span_encoded_start(),
            build.encoder().active_text_span_logical_start(),
        ),
        DraftComposerBuildCorruption::CarryFrontier => DraftComposerEncoderStateV1::new(
            build.encoder().cursor(),
            build.encoder().source_piece_count(),
            build.encoder().encoded_bytes() + 1,
            build.encoder().logical_utf8_bytes(),
            build.encoder().chunk_count(),
            build.encoder().piece_count(),
            build.encoder().marker_count(),
            build.encoder().marker_digest(),
            build.encoder().maximum_image_label(),
            build.encoder().chain_digest(),
            build.encoder().carry().to_vec(),
            build.encoder().break_before(),
            build.encoder().active_text_span_encoded_start(),
            build.encoder().active_text_span_logical_start(),
        ),
        DraftComposerBuildCorruption::MarkerFrontier => DraftComposerEncoderStateV1::new(
            build.encoder().cursor(),
            build.encoder().source_piece_count(),
            build.encoder().encoded_bytes(),
            build.encoder().logical_utf8_bytes(),
            build.encoder().chunk_count(),
            build.encoder().piece_count(),
            build.encoder().marker_count(),
            build.encoder().marker_digest(),
            Some(ImageLabelOrdinal::new(1).unwrap()),
            build.encoder().chain_digest(),
            build.encoder().carry().to_vec(),
            build.encoder().break_before(),
            build.encoder().active_text_span_encoded_start(),
            build.encoder().active_text_span_logical_start(),
        ),
        DraftComposerBuildCorruption::PlanningSourceOverflow => DraftComposerEncoderStateV1::new(
            build.encoder().cursor(),
            u64::MAX,
            build.encoder().encoded_bytes(),
            build.encoder().logical_utf8_bytes(),
            build.encoder().chunk_count(),
            build.encoder().piece_count(),
            build.encoder().marker_count(),
            build.encoder().marker_digest(),
            build.encoder().maximum_image_label(),
            build.encoder().chain_digest(),
            build.encoder().carry().to_vec(),
            build.encoder().break_before(),
            build.encoder().active_text_span_encoded_start(),
            build.encoder().active_text_span_logical_start(),
        ),
        DraftComposerBuildCorruption::PlanningCursorMismatch => DraftComposerEncoderStateV1::new(
            DraftComposerSourceCursorV1::new(1, 0),
            0,
            build.encoder().encoded_bytes(),
            build.encoder().logical_utf8_bytes(),
            build.encoder().chunk_count(),
            build.encoder().piece_count(),
            build.encoder().marker_count(),
            build.encoder().marker_digest(),
            build.encoder().maximum_image_label(),
            build.encoder().chain_digest(),
            build.encoder().carry().to_vec(),
            build.encoder().break_before(),
            build.encoder().active_text_span_encoded_start(),
            build.encoder().active_text_span_logical_start(),
        ),
        DraftComposerBuildCorruption::PlanningEofMismatch => DraftComposerEncoderStateV1::new(
            DraftComposerSourceCursorV1::new(key.source().summary().piece_count(), 0),
            key.source().summary().piece_count() - 1,
            build.encoder().encoded_bytes(),
            build.encoder().logical_utf8_bytes(),
            build.encoder().chunk_count(),
            build.encoder().piece_count(),
            build.encoder().marker_count(),
            build.encoder().marker_digest(),
            build.encoder().maximum_image_label(),
            build.encoder().chain_digest(),
            build.encoder().carry().to_vec(),
            build.encoder().break_before(),
            build.encoder().active_text_span_encoded_start(),
            build.encoder().active_text_span_logical_start(),
        ),
        DraftComposerBuildCorruption::PlanningPieceMaximum => {
            let mut carry = vec![1];
            carry.extend_from_slice(&key.source().summary().piece_count().to_be_bytes());
            carry.push(0);
            carry.extend_from_slice(&5_u64.to_be_bytes());
            carry.extend_from_slice(b"first");
            DraftComposerEncoderStateV1::new(
                DraftComposerSourceCursorV1::new(1, 0),
                1,
                23,
                5,
                0,
                u64::MAX,
                0,
                beryl_model::content_marker_digest_seed(),
                None,
                content_chain_seed(ContentEncoding::ComposerV1),
                carry,
                false,
                None,
                None,
            )
        }
        DraftComposerBuildCorruption::PlanningMaximum
        | DraftComposerBuildCorruption::TerminalPlanningMaximum => {
            DraftComposerEncoderStateV1::new(
                DraftComposerSourceCursorV1::new(1, 0),
                1,
                9,
                0,
                u64::MAX,
                u64::MAX,
                0,
                beryl_model::content_marker_digest_seed(),
                None,
                SyndicContentDigest::from_bytes([0xD1; 32]),
                Vec::new(),
                false,
                None,
                None,
            )
        }
        DraftComposerBuildCorruption::PlanningDigestCount => DraftComposerEncoderStateV1::new(
            DraftComposerSourceCursorV1::new(1, 0),
            1,
            9,
            0,
            1,
            1,
            0,
            beryl_model::content_marker_digest_seed(),
            None,
            content_chain_seed(ContentEncoding::ComposerV1),
            Vec::new(),
            false,
            None,
            None,
        ),
        DraftComposerBuildCorruption::TerminalFrontier
        | DraftComposerBuildCorruption::OutputSummary
        | DraftComposerBuildCorruption::SealedLifecycle => build.encoder().clone(),
    };
    let output = match corruption {
        DraftComposerBuildCorruption::Cursor
        | DraftComposerBuildCorruption::CarryFrontier
        | DraftComposerBuildCorruption::MarkerFrontier
        | DraftComposerBuildCorruption::PlanningSourceOverflow
        | DraftComposerBuildCorruption::PlanningCursorMismatch
        | DraftComposerBuildCorruption::PlanningEofMismatch
        | DraftComposerBuildCorruption::PlanningPieceMaximum
        | DraftComposerBuildCorruption::PlanningMaximum
        | DraftComposerBuildCorruption::PlanningDigestCount
        | DraftComposerBuildCorruption::TerminalPlanningMaximum
        | DraftComposerBuildCorruption::TerminalFrontier => build.output(),
        DraftComposerBuildCorruption::OutputSummary => {
            let output = build.output().unwrap();
            let summary = output.summary();
            let corrupt = ContentSummary::new(
                summary.chunk_count(),
                summary.piece_count(),
                summary.encoded_bytes() + 1,
                summary.logical_utf8_bytes(),
                summary.atom_count(),
                summary.image_marker_count(),
                summary.marker_digest(),
                summary.maximum_image_label(),
                summary.digest(),
            )
            .unwrap();
            Some(ContentReference::new(
                output.id(),
                output.revision(),
                output.encoding(),
                corrupt,
            ))
        }
        DraftComposerBuildCorruption::SealedLifecycle => build.output(),
    };
    let records = match corruption {
        DraftComposerBuildCorruption::TerminalFrontier => DraftComposerRecordFrontierV1::new(
            build.records().cursor(),
            build.records().encoded_bytes() + 1,
            build.records().logical_utf8_bytes(),
            build.records().piece_count(),
            build.records().marker_count(),
            build.records().marker_digest(),
            build.records().maximum_image_label(),
            build.records().chunk_start(),
            build.records().chunk_ordinal(),
            build.records().break_before(),
        ),
        _ => build.records(),
    };
    let lifecycle = match corruption {
        DraftComposerBuildCorruption::TerminalFrontier
        | DraftComposerBuildCorruption::TerminalPlanningMaximum
        | DraftComposerBuildCorruption::SealedLifecycle => DraftComposerBuildLifecycleV1::Cancelled,
        _ => build.lifecycle().clone(),
    };
    let replacement = DraftComposerBuildRecordV1::new(
        key,
        encoder,
        records,
        output,
        build.output_revision(),
        build.output_chunk_count(),
        build.output_encoded_bytes(),
        build.output_chain_digest(),
        lifecycle,
    );
    contribution(store, storage, Replacement::Build(key, replacement))
}

pub fn inject_draft_composer_mapping_corruption(
    store: &HomeStore,
    storage: SyndicStorage,
    key: DraftComposerMaterializationKeyV1,
) -> MutationContribution {
    let mapping = storage
        .point::<DraftComposerMaterializationsFamily>(
            store,
            key,
            SyndicPointReadLimit::new(768).unwrap(),
        )
        .unwrap()
        .unwrap();
    let reference = mapping.content();
    let summary = reference.summary();
    let corrupt = ContentSummary::new(
        summary.chunk_count(),
        summary.piece_count(),
        summary.encoded_bytes() + 1,
        summary.logical_utf8_bytes(),
        summary.atom_count(),
        summary.image_marker_count(),
        summary.marker_digest(),
        summary.maximum_image_label(),
        summary.digest(),
    )
    .unwrap();
    let replacement = DraftComposerMaterializationRecordV1::new(
        key,
        mapping.sealing_operation(),
        ContentReference::new(
            reference.id(),
            reference.revision(),
            reference.encoding(),
            corrupt,
        ),
    );
    contribution(store, storage, Replacement::Mapping(key, replacement))
}

pub fn inject_draft_composer_manifest_corruption(
    store: &HomeStore,
    storage: SyndicStorage,
    content: SyndicContentId,
) -> MutationContribution {
    let manifest = storage
        .point::<ContentManifestsFamily>(store, content, SyndicPointReadLimit::new(768).unwrap())
        .unwrap()
        .unwrap();
    let replacement = ContentManifestRecord::new(
        content,
        manifest.revision(),
        manifest.encoding(),
        manifest.lifecycle(),
        manifest.chunk_count(),
        manifest.encoded_bytes(),
        SyndicContentDigest::from_bytes([0xD7; 32]),
        manifest.expected(),
    );
    contribution(store, storage, Replacement::Manifest(content, replacement))
}

pub fn inject_draft_composer_chunk_corruption(
    store: &HomeStore,
    storage: SyndicStorage,
    content: SyndicContentId,
) -> MutationContribution {
    let key = ContentChunkKey {
        owner: content,
        ordinal: ContentChunkOrdinal::FIRST,
    };
    let chunk = storage
        .point::<ContentChunksFamily>(
            store,
            key,
            SyndicPointReadLimit::new(crate::CONTENT_CHUNK_MAX_BYTES + 128).unwrap(),
        )
        .unwrap()
        .unwrap();
    let mut bytes = chunk.bytes().to_vec();
    let last = bytes.len() - 1;
    bytes[last] ^= 0x80;
    let replacement = ContentChunkRecord::new(content, chunk.ordinal(), bytes).unwrap();
    contribution(store, storage, Replacement::Chunk(key, replacement))
}

pub fn inject_draft_composer_prepared_chunk(
    store: &HomeStore,
    storage: SyndicStorage,
    prepared: &PreparedDraftComposerStepV1,
    collision: bool,
) -> MutationContribution {
    let chunk = prepared.fault_chunk().unwrap();
    let key = ContentChunkKey {
        owner: chunk.content_id(),
        ordinal: chunk.ordinal(),
    };
    let replacement = if collision {
        let mut bytes = chunk.bytes().to_vec();
        let last = bytes.len() - 1;
        bytes[last] ^= 0x80;
        ContentChunkRecord::new(chunk.content_id(), chunk.ordinal(), bytes).unwrap()
    } else {
        chunk
    };
    contribution(store, storage, Replacement::Chunk(key, replacement))
}

pub fn inject_draft_composer_output_corruption(
    store: &HomeStore,
    storage: SyndicStorage,
    content: SyndicContentId,
    corruption: DraftComposerOutputCorruption,
) -> MutationContribution {
    match corruption {
        DraftComposerOutputCorruption::Chunk => {
            inject_draft_composer_chunk_corruption(store, storage, content)
        }
        DraftComposerOutputCorruption::ByteSpan => contribution(
            store,
            storage,
            Replacement::DeleteByteSpan(ContentByteSpanKey {
                owner: content,
                start: 0,
            }),
        ),
        DraftComposerOutputCorruption::TextSpan => contribution(
            store,
            storage,
            Replacement::DeleteTextSpan(ContentTextSpanKey {
                owner: content,
                logical_start: 0,
            }),
        ),
        DraftComposerOutputCorruption::Piece => contribution(
            store,
            storage,
            Replacement::DeletePiece(ContentPieceKey {
                owner: content,
                ordinal: ContentPieceOrdinal::FIRST,
            }),
        ),
    }
}

pub fn delete_draft_composer_source(
    store: &HomeStore,
    storage: SyndicStorage,
    source: DraftPieceRootReferenceV1,
) -> MutationContribution {
    contribution(store, storage, Replacement::DeleteSource(source.key()))
}

pub fn delete_draft_composer_origin_build(
    store: &HomeStore,
    storage: SyndicStorage,
    mapping: DraftComposerMaterializationRecordV1,
) -> MutationContribution {
    contribution(
        store,
        storage,
        Replacement::DeleteBuild(DraftComposerBuildKeyV1::new(
            mapping.key().source(),
            mapping.key().format(),
            mapping.sealing_operation(),
        )),
    )
}

fn contribution(
    store: &HomeStore,
    storage: SyndicStorage,
    replacement: Replacement,
) -> MutationContribution {
    storage
        .handle
        .contribution(storage.revision(store).unwrap(), Replace(replacement))
}

impl DomainMutation<SyndicDomain> for Replace {
    type Error = Infallible;

    fn validate(&self, _: &DomainReader<'_, SyndicDomain>) -> Result<(), Self::Error> {
        Ok(())
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        match self.0 {
            Replacement::Build(_, _) => reservation
                .reserve_records::<DraftComposerBuildsCodec>(1)
                .unwrap(),
            Replacement::Mapping(_, _) => reservation
                .reserve_records::<DraftComposerMaterializationsCodec>(1)
                .unwrap(),
            Replacement::Manifest(_, _) => reservation
                .reserve_records::<ContentManifestsCodec>(1)
                .unwrap(),
            Replacement::Chunk(_, _) => reservation
                .reserve_records::<ContentChunksCodec>(1)
                .unwrap(),
            Replacement::DeleteSource(_) => reservation
                .reserve_records::<DraftPieceRootsCodec>(1)
                .unwrap(),
            Replacement::DeleteBuild(_) => reservation
                .reserve_records::<DraftComposerBuildsCodec>(1)
                .unwrap(),
            Replacement::DeleteByteSpan(_) => reservation
                .reserve_records::<ContentByteSpansCodec>(1)
                .unwrap(),
            Replacement::DeleteTextSpan(_) => reservation
                .reserve_records::<ContentTextSpansCodec>(1)
                .unwrap(),
            Replacement::DeletePiece(_) => reservation
                .reserve_records::<ContentPiecesCodec>(1)
                .unwrap(),
        }
        Ok(())
    }

    fn contribute(
        &self,
        _: &DomainReader<'_, SyndicDomain>,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        match &self.0 {
            Replacement::Build(key, value) => mutations
                .put::<DraftComposerBuildsCodec>(key, value)
                .unwrap(),
            Replacement::Mapping(key, value) => mutations
                .put::<DraftComposerMaterializationsCodec>(key, value)
                .unwrap(),
            Replacement::Manifest(key, value) => {
                mutations.put::<ContentManifestsCodec>(key, value).unwrap()
            }
            Replacement::Chunk(key, value) => {
                mutations.put::<ContentChunksCodec>(key, value).unwrap()
            }
            Replacement::DeleteSource(key) => {
                mutations.delete::<DraftPieceRootsCodec>(key).unwrap()
            }
            Replacement::DeleteBuild(key) => {
                mutations.delete::<DraftComposerBuildsCodec>(key).unwrap()
            }
            Replacement::DeleteByteSpan(key) => {
                mutations.delete::<ContentByteSpansCodec>(key).unwrap()
            }
            Replacement::DeleteTextSpan(key) => {
                mutations.delete::<ContentTextSpansCodec>(key).unwrap()
            }
            Replacement::DeletePiece(key) => mutations.delete::<ContentPiecesCodec>(key).unwrap(),
        }
        Ok(())
    }
}
