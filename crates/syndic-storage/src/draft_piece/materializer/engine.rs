use std::convert::Infallible;

use beryl_home_store::{
    DomainMutation, DomainReader, HomeStore, MutationBuilder, MutationContribution,
    ReconciliationReservation,
};
use beryl_model::{ContentRevision, DomainRevision, SyndicContentDigest, SyndicContentId};
use sha2::{Digest, Sha256};

use crate::{
    ComposerAtomOrdinal, ContentByteSpanRecord, ContentChunkOrdinal, ContentChunkRecord,
    ContentEncoding, ContentLifecycle, ContentManifestRecord, ContentPieceOrdinal,
    ContentPieceRecord, ContentReference, ContentSummary, ContentTextSpanRecord,
    DraftPieceLeafValueV1, InputMarkerOrdinal, SyndicMutationError, SyndicPointReadLimit,
    SyndicStorage, advance_content_chain, content_chain_seed,
    draft_piece::read_materialization_page,
};
use crate::{
    codec::{
        ContentByteSpanKey, ContentByteSpansCodec, ContentByteSpansFamily, ContentChunkKey,
        ContentChunksCodec, ContentChunksFamily, ContentManifestsCodec, ContentManifestsFamily,
        ContentPieceKey, ContentPiecesCodec, ContentPiecesFamily, ContentTextSpanKey,
        ContentTextSpansCodec, ContentTextSpansFamily, ExactCodec, Family, family_point_limit,
    },
    domain::SyndicDomain,
};

use super::{codec::*, model::*};

#[derive(Clone)]
pub struct PreparedDraftComposerStepV1 {
    expected: DraftComposerBuildRecordV1,
    next: DraftComposerBuildRecordV1,
    expected_manifest: Option<ContentManifestRecord>,
    next_manifest: Option<ContentManifestRecord>,
    chunk: Option<ContentChunkRecord>,
    byte_span: Option<ContentByteSpanRecord>,
    text_span: Option<ContentTextSpanRecord>,
    piece: Option<ContentPieceRecord>,
    mapping: Option<DraftComposerMaterializationRecordV1>,
    records_read: u64,
    input_payload_bytes: usize,
    resident_bytes: usize,
}

impl PreparedDraftComposerStepV1 {
    #[must_use]
    pub const fn next_phase(&self) -> Option<DraftComposerBuildPhaseV1> {
        match self.next.lifecycle() {
            DraftComposerBuildLifecycleV1::Open(phase) => Some(*phase),
            _ => None,
        }
    }

    #[must_use]
    pub const fn records_read(&self) -> u64 {
        self.records_read
    }

    #[must_use]
    pub const fn input_payload_bytes(&self) -> usize {
        self.input_payload_bytes
    }

    #[must_use]
    pub const fn resident_bytes(&self) -> usize {
        self.resident_bytes
    }

    #[must_use]
    pub fn written_record_count(&self) -> usize {
        1 + usize::from(self.next_manifest.is_some())
            + usize::from(self.chunk.is_some())
            + usize::from(self.byte_span.is_some())
            + usize::from(self.text_span.is_some())
            + usize::from(self.piece.is_some())
            + usize::from(self.mapping.is_some())
    }

    #[cfg(feature = "test-faults")]
    pub(crate) fn fault_chunk(&self) -> Option<ContentChunkRecord> {
        self.chunk.clone()
    }
}

#[derive(Clone)]
struct BeginMutation {
    initial: DraftComposerBuildRecordV1,
}

#[derive(Clone)]
struct StepMutation {
    prepared: PreparedDraftComposerStepV1,
}

#[derive(Clone, Copy)]
enum TerminalKind {
    Cancel,
    Fail,
    Supersede(DraftComposerMaterializationOperationIdV1),
}

#[derive(Clone)]
struct TerminalMutation {
    key: DraftComposerBuildKeyV1,
    kind: TerminalKind,
}

struct EncoderWork {
    cursor: DraftComposerSourceCursorV1,
    source_piece_count: u64,
    encoded_bytes: u64,
    logical_utf8_bytes: u64,
    chunk_count: u64,
    piece_count: u64,
    marker_count: u64,
    marker_digest: [u8; 32],
    maximum_image_label: Option<crate::ImageLabelOrdinal>,
    chain_digest: SyndicContentDigest,
    carry: Vec<u8>,
    break_before: bool,
    active_encoded_start: Option<u64>,
    active_logical_start: Option<u64>,
}

impl EncoderWork {
    fn initial(atom_count: u64) -> Self {
        let mut carry = Vec::with_capacity(DRAFT_COMPOSER_CARRY_MAX_BYTES);
        carry.push(1);
        carry.extend_from_slice(&atom_count.to_be_bytes());
        Self {
            cursor: DraftComposerSourceCursorV1::new(0, 0),
            source_piece_count: 0,
            encoded_bytes: 9,
            logical_utf8_bytes: 0,
            chunk_count: 0,
            piece_count: 0,
            marker_count: 0,
            marker_digest: beryl_model::sequential_marker_digest_seed(),
            maximum_image_label: None,
            chain_digest: content_chain_seed(ContentEncoding::ComposerV1),
            carry,
            break_before: false,
            active_encoded_start: None,
            active_logical_start: None,
        }
    }

    fn from_state(state: &DraftComposerEncoderStateV1) -> Self {
        Self {
            cursor: state.cursor(),
            source_piece_count: state.source_piece_count(),
            encoded_bytes: state.encoded_bytes(),
            logical_utf8_bytes: state.logical_utf8_bytes(),
            chunk_count: state.chunk_count(),
            piece_count: state.piece_count(),
            marker_count: state.marker_count(),
            marker_digest: state.marker_digest(),
            maximum_image_label: state.maximum_image_label(),
            chain_digest: state.chain_digest(),
            carry: state.carry().to_vec(),
            break_before: state.break_before(),
            active_encoded_start: state.active_text_span_encoded_start(),
            active_logical_start: state.active_text_span_logical_start(),
        }
    }

    fn state(self) -> DraftComposerEncoderStateV1 {
        DraftComposerEncoderStateV1::new(
            self.cursor,
            self.source_piece_count,
            self.encoded_bytes,
            self.logical_utf8_bytes,
            self.chunk_count,
            self.piece_count,
            self.marker_count,
            self.marker_digest,
            self.maximum_image_label,
            self.chain_digest,
            self.carry,
            self.break_before,
            self.active_encoded_start,
            self.active_logical_start,
        )
    }

    fn finalize_text_span(&mut self) -> Result<(), DraftComposerMaterializationErrorV1> {
        if self.active_encoded_start.take().is_some() {
            self.active_logical_start.take();
            self.piece_count = checked_next(self.piece_count)?;
        }
        Ok(())
    }

    fn flush(
        &mut self,
        content_id: SyndicContentId,
    ) -> Result<
        Option<(ContentChunkRecord, ContentByteSpanRecord)>,
        DraftComposerMaterializationErrorV1,
    > {
        if self.carry.is_empty() {
            return Ok(None);
        }
        self.finalize_text_span()?;
        let chunk_count = checked_next(self.chunk_count)?;
        let ordinal = ContentChunkOrdinal::new(chunk_count)
            .map_err(|_| DraftComposerMaterializationErrorV1::LengthOverflow)?;
        let bytes = std::mem::take(&mut self.carry);
        let chunk = ContentChunkRecord::new(content_id, ordinal, bytes)
            .map_err(|_| DraftComposerMaterializationErrorV1::InvalidOutput)?;
        let start = self
            .encoded_bytes
            .checked_sub(chunk.bytes().len() as u64)
            .ok_or(DraftComposerMaterializationErrorV1::InvalidBuild)?;
        let span = ContentByteSpanRecord::for_chunk(&chunk, start)
            .map_err(|_| DraftComposerMaterializationErrorV1::InvalidOutput)?;
        self.chunk_count = chunk_count;
        self.chain_digest = advance_content_chain(self.chain_digest, &chunk);
        Ok(Some((chunk, span)))
    }

    fn summary(
        &self,
        atom_count: u64,
    ) -> Result<ContentSummary, DraftComposerMaterializationErrorV1> {
        ContentSummary::new(
            self.chunk_count,
            self.piece_count,
            self.encoded_bytes,
            self.logical_utf8_bytes,
            atom_count,
            self.marker_count,
            self.marker_digest,
            self.maximum_image_label,
            self.chain_digest,
        )
        .map_err(|_| DraftComposerMaterializationErrorV1::InvalidOutput)
    }
}

fn checked_next(value: u64) -> Result<u64, DraftComposerMaterializationErrorV1> {
    value
        .checked_add(1)
        .ok_or(DraftComposerMaterializationErrorV1::LengthOverflow)
}

fn content_id_for(
    key: DraftComposerBuildKeyV1,
) -> Result<SyndicContentId, DraftComposerMaterializationErrorV1> {
    let encoded = DraftComposerBuildsFamily::encode_key(&key)
        .map_err(|_| DraftComposerMaterializationErrorV1::InvalidBuild)?;
    let mut digest = Sha256::new();
    digest.update(b"syndic-draft-composer-output-v1\0");
    digest.update(encoded);
    Ok(SyndicContentId::from_digest(digest.finalize().into()))
}

fn point<F: Family>(
    reader: &DomainReader<'_, SyndicDomain>,
    key: &F::Key,
) -> Result<Option<F::Value>, SyndicMutationError> {
    reader
        .point::<ExactCodec<F>>(key, family_point_limit::<F>())
        .map_err(Into::into)
}

fn storage_point_limit<F: Family>() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(family_point_limit::<F>().max_bytes())
        .expect("materializer point-read limit is nonzero")
}

fn empty_record_frontier() -> DraftComposerRecordFrontierV1 {
    DraftComposerRecordFrontierV1::new(
        DraftComposerSourceCursorV1::new(0, 0),
        9,
        0,
        0,
        0,
        beryl_model::sequential_marker_digest_seed(),
        None,
        0,
        1,
        false,
    )
}

fn initial_build(key: DraftComposerBuildKeyV1) -> DraftComposerBuildRecordV1 {
    DraftComposerBuildRecordV1::new(
        key,
        EncoderWork::initial(key.source().summary().piece_count()).state(),
        empty_record_frontier(),
        None,
        None,
        0,
        0,
        content_chain_seed(ContentEncoding::ComposerV1),
        DraftComposerBuildLifecycleV1::Open(DraftComposerBuildPhaseV1::Planning),
    )
}

impl SyndicStorage {
    pub fn begin_draft_composer_materialization(
        &self,
        expected_domain_revision: DomainRevision,
        key: DraftComposerBuildKeyV1,
    ) -> MutationContribution {
        self.handle.contribution(
            expected_domain_revision,
            BeginMutation {
                initial: initial_build(key),
            },
        )
    }

    pub fn advance_draft_composer_materialization(
        &self,
        expected_domain_revision: DomainRevision,
        prepared: PreparedDraftComposerStepV1,
    ) -> MutationContribution {
        self.handle
            .contribution(expected_domain_revision, StepMutation { prepared })
    }

    pub fn cancel_draft_composer_materialization(
        &self,
        expected_domain_revision: DomainRevision,
        key: DraftComposerBuildKeyV1,
    ) -> MutationContribution {
        self.handle.contribution(
            expected_domain_revision,
            TerminalMutation {
                key,
                kind: TerminalKind::Cancel,
            },
        )
    }

    pub fn fail_draft_composer_materialization(
        &self,
        expected_domain_revision: DomainRevision,
        key: DraftComposerBuildKeyV1,
    ) -> MutationContribution {
        self.handle.contribution(
            expected_domain_revision,
            TerminalMutation {
                key,
                kind: TerminalKind::Fail,
            },
        )
    }

    pub fn supersede_draft_composer_materialization(
        &self,
        expected_domain_revision: DomainRevision,
        key: DraftComposerBuildKeyV1,
        successor: DraftComposerMaterializationOperationIdV1,
    ) -> MutationContribution {
        self.handle.contribution(
            expected_domain_revision,
            TerminalMutation {
                key,
                kind: TerminalKind::Supersede(successor),
            },
        )
    }

    pub fn draft_composer_materialization_status(
        &self,
        store: &HomeStore,
        key: DraftComposerBuildKeyV1,
    ) -> Result<DraftComposerMaterializationStatusV1, DraftComposerMaterializationErrorV1> {
        let mapping_key = DraftComposerMaterializationKeyV1::new(key.source(), key.format());
        if let Some(mapping) = self.point::<DraftComposerMaterializationsFamily>(
            store,
            mapping_key,
            storage_point_limit::<DraftComposerMaterializationsFamily>(),
        )? {
            validate_sealed_mapping_closure(self, store, mapping_key, mapping)?;
            return Ok(DraftComposerMaterializationStatusV1::Sealed(mapping));
        }
        let Some(build) = self.point::<DraftComposerBuildsFamily>(
            store,
            key,
            storage_point_limit::<DraftComposerBuildsFamily>(),
        )?
        else {
            return Ok(DraftComposerMaterializationStatusV1::Absent);
        };
        validate_build_identity(key, &build)?;
        validate_output_frontier_records(self, store, &build)?;
        if matches!(build.lifecycle(), DraftComposerBuildLifecycleV1::Open(_))
            && let Some(output) = build.output()
        {
            let manifest = self
                .point::<ContentManifestsFamily>(
                    store,
                    output.id(),
                    storage_point_limit::<ContentManifestsFamily>(),
                )?
                .ok_or(DraftComposerMaterializationErrorV1::InvalidOutput)?;
            validate_build_manifest(&build, &manifest)?;
        }
        Ok(match build.lifecycle() {
            DraftComposerBuildLifecycleV1::Open(_) => {
                let DraftComposerBuildLifecycleV1::Open(phase) = build.lifecycle() else {
                    unreachable!()
                };
                DraftComposerMaterializationStatusV1::Building(*phase)
            }
            DraftComposerBuildLifecycleV1::Cancelled => {
                DraftComposerMaterializationStatusV1::Cancelled
            }
            DraftComposerBuildLifecycleV1::Failed(reason) => {
                DraftComposerMaterializationStatusV1::Failed(*reason)
            }
            DraftComposerBuildLifecycleV1::Superseded(successor) => {
                DraftComposerMaterializationStatusV1::Superseded(*successor)
            }
            DraftComposerBuildLifecycleV1::Sealed(reference) => {
                let _ = reference;
                return Err(DraftComposerMaterializationErrorV1::InvalidBuild);
            }
        })
    }

    pub fn prepare_draft_composer_materialization_step(
        &self,
        store: &HomeStore,
        key: DraftComposerBuildKeyV1,
    ) -> Result<Option<PreparedDraftComposerStepV1>, DraftComposerMaterializationErrorV1> {
        let mapping_key = DraftComposerMaterializationKeyV1::new(key.source(), key.format());
        if let Some(mapping) = self.point::<DraftComposerMaterializationsFamily>(
            store,
            mapping_key,
            storage_point_limit::<DraftComposerMaterializationsFamily>(),
        )? {
            validate_sealed_mapping_closure(self, store, mapping_key, mapping)?;
            return Ok(None);
        }
        let build = self
            .point::<DraftComposerBuildsFamily>(
                store,
                key,
                storage_point_limit::<DraftComposerBuildsFamily>(),
            )?
            .ok_or(DraftComposerMaterializationErrorV1::MissingBuild)?;
        validate_build_identity(key, &build)?;
        validate_output_frontier_records(self, store, &build)?;
        let DraftComposerBuildLifecycleV1::Open(phase) = *build.lifecycle() else {
            return Ok(None);
        };
        let prepared = match phase {
            DraftComposerBuildPhaseV1::Planning => self.prepare_plan_step(store, build)?,
            DraftComposerBuildPhaseV1::Writing => self.prepare_write_step(store, build)?,
            DraftComposerBuildPhaseV1::Draining { final_chunk } => {
                self.prepare_drain_step(store, build, final_chunk)?
            }
            DraftComposerBuildPhaseV1::ReadyToSeal => self.prepare_seal_step(store, build)?,
        };
        Ok(Some(prepared))
    }

    fn prepare_plan_step(
        &self,
        store: &HomeStore,
        build: DraftComposerBuildRecordV1,
    ) -> Result<PreparedDraftComposerStepV1, DraftComposerMaterializationErrorV1> {
        let source = build.key().source();
        let cursor = build.encoder().cursor();
        let page = read_materialization_page(
            self,
            store,
            source,
            cursor.piece_index(),
            DRAFT_COMPOSER_INPUT_MAX_RECORDS,
            DRAFT_COMPOSER_INPUT_MAX_BYTES,
        )?;
        let mut work = EncoderWork::from_state(build.encoder());
        let flushed = advance_encoder(
            &mut work,
            page.pieces(),
            source.summary().piece_count(),
            SyndicContentId::from_bytes([0; 16]),
        )?;
        let at_eof = work.cursor.piece_index() == source.summary().piece_count()
            && work.cursor.atom_encoded_offset() == 0;
        let mut next_manifest = None;
        let lifecycle;
        let output;
        let output_revision;
        if at_eof {
            if flushed.is_none() {
                let _ = work.flush(SyndicContentId::from_bytes([0; 16]))?;
            }
            let summary = work.summary(source.summary().piece_count())?;
            if summary.logical_utf8_bytes() != source.summary().logical_utf8_bytes()
                || summary.image_marker_count() != source.summary().marker_count()
            {
                return Err(DraftComposerMaterializationErrorV1::InvalidBuild);
            }
            let content_id = content_id_for(build.key())?;
            let revision = ContentRevision::new(1)
                .map_err(|_| DraftComposerMaterializationErrorV1::LengthOverflow)?;
            let reference =
                ContentReference::new(content_id, revision, ContentEncoding::ComposerV1, summary);
            let existing = self.point::<ContentManifestsFamily>(
                store,
                content_id,
                storage_point_limit::<ContentManifestsFamily>(),
            )?;
            if let Some(existing) = existing {
                if existing.encoding() != ContentEncoding::ComposerV1
                    || existing.expected() != summary
                    || existing.owner().is_some()
                {
                    return Err(DraftComposerMaterializationErrorV1::InvalidOutput);
                }
                if existing.lifecycle() == ContentLifecycle::Sealed {
                    return Err(DraftComposerMaterializationErrorV1::InvalidOutput);
                }
            } else {
                next_manifest = Some(ContentManifestRecord::new(
                    content_id,
                    revision,
                    ContentEncoding::ComposerV1,
                    ContentLifecycle::Building,
                    0,
                    0,
                    content_chain_seed(ContentEncoding::ComposerV1),
                    summary,
                ));
            }
            work = EncoderWork::initial(source.summary().piece_count());
            lifecycle = DraftComposerBuildLifecycleV1::Open(DraftComposerBuildPhaseV1::Writing);
            output = Some(reference);
            output_revision = Some(revision);
        } else {
            lifecycle = DraftComposerBuildLifecycleV1::Open(DraftComposerBuildPhaseV1::Planning);
            output = None;
            output_revision = None;
        }
        let next = DraftComposerBuildRecordV1::new(
            build.key(),
            work.state(),
            build.records(),
            output,
            output_revision,
            0,
            0,
            content_chain_seed(ContentEncoding::ComposerV1),
            lifecycle,
        );
        prepared(
            build,
            next,
            None,
            next_manifest,
            None,
            None,
            None,
            None,
            None,
            page.records_read(),
            page.payload_bytes(),
        )
    }

    fn prepare_write_step(
        &self,
        store: &HomeStore,
        build: DraftComposerBuildRecordV1,
    ) -> Result<PreparedDraftComposerStepV1, DraftComposerMaterializationErrorV1> {
        let reference = build
            .output()
            .ok_or(DraftComposerMaterializationErrorV1::InvalidBuild)?;
        let manifest = self
            .point::<ContentManifestsFamily>(
                store,
                reference.id(),
                storage_point_limit::<ContentManifestsFamily>(),
            )?
            .ok_or(DraftComposerMaterializationErrorV1::InvalidOutput)?;
        validate_build_manifest(&build, &manifest)?;
        let cursor = build.encoder().cursor();
        let page = read_materialization_page(
            self,
            store,
            build.key().source(),
            cursor.piece_index(),
            DRAFT_COMPOSER_INPUT_MAX_RECORDS,
            DRAFT_COMPOSER_INPUT_MAX_BYTES,
        )?;
        let mut work = EncoderWork::from_state(build.encoder());
        let mut emitted = advance_encoder(
            &mut work,
            page.pieces(),
            build.key().source().summary().piece_count(),
            reference.id(),
        )?;
        let at_eof = work.cursor.piece_index() == build.key().source().summary().piece_count()
            && work.cursor.atom_encoded_offset() == 0;
        let final_chunk = at_eof;
        if emitted.is_none() && at_eof {
            emitted = work.flush(reference.id())?;
        }
        let Some((chunk, byte_span)) = emitted else {
            let next = copy_build(
                &build,
                work.state(),
                build.records(),
                build.output(),
                build.output_revision(),
                build.output_chunk_count(),
                build.output_encoded_bytes(),
                build.output_chain_digest(),
                DraftComposerBuildLifecycleV1::Open(DraftComposerBuildPhaseV1::Writing),
            );
            return prepared(
                build,
                next,
                Some(manifest),
                None,
                None,
                None,
                None,
                None,
                None,
                page.records_read(),
                page.payload_bytes(),
            );
        };
        let chunk_end = byte_span.end();
        let next_revision = manifest
            .revision()
            .checked_next()
            .map_err(|_| DraftComposerMaterializationErrorV1::LengthOverflow)?;
        let next_manifest = ContentManifestRecord::new(
            manifest.id(),
            next_revision,
            manifest.encoding(),
            ContentLifecycle::Building,
            work.chunk_count,
            chunk_end,
            work.chain_digest,
            manifest.expected(),
        );
        let next_reference = ContentReference::new(
            reference.id(),
            next_revision,
            ContentEncoding::ComposerV1,
            reference.summary(),
        );
        let next = copy_build(
            &build,
            work.state(),
            build.records(),
            Some(next_reference),
            Some(next_revision),
            chunk.ordinal().get(),
            chunk_end,
            next_manifest.chain_digest(),
            DraftComposerBuildLifecycleV1::Open(DraftComposerBuildPhaseV1::Draining {
                final_chunk,
            }),
        );
        prepared(
            build,
            next,
            Some(manifest),
            Some(next_manifest),
            Some(chunk),
            Some(byte_span),
            None,
            None,
            None,
            page.records_read(),
            page.payload_bytes(),
        )
    }

    fn prepare_drain_step(
        &self,
        store: &HomeStore,
        build: DraftComposerBuildRecordV1,
        final_chunk: bool,
    ) -> Result<PreparedDraftComposerStepV1, DraftComposerMaterializationErrorV1> {
        let source = build.key().source();
        let frontier = build.records();
        if frontier.cursor().piece_index() == source.summary().piece_count() {
            if frontier.encoded_bytes() != build.output_encoded_bytes() {
                return Err(DraftComposerMaterializationErrorV1::InvalidBuild);
            }
            let phase = if final_chunk {
                DraftComposerBuildPhaseV1::ReadyToSeal
            } else {
                DraftComposerBuildPhaseV1::Writing
            };
            let next_frontier = if final_chunk {
                frontier
            } else {
                DraftComposerRecordFrontierV1::new(
                    frontier.cursor(),
                    frontier.encoded_bytes(),
                    frontier.logical_utf8_bytes(),
                    frontier.piece_count(),
                    frontier.marker_count(),
                    frontier.marker_digest(),
                    frontier.maximum_image_label(),
                    build.output_encoded_bytes(),
                    checked_next(frontier.chunk_ordinal())?,
                    frontier.break_before(),
                )
            };
            let next = copy_build(
                &build,
                build.encoder().clone(),
                next_frontier,
                build.output(),
                build.output_revision(),
                build.output_chunk_count(),
                build.output_encoded_bytes(),
                build.output_chain_digest(),
                DraftComposerBuildLifecycleV1::Open(phase),
            );
            return prepared(build, next, None, None, None, None, None, None, None, 0, 0);
        }
        let page = read_materialization_page(
            self,
            store,
            source,
            frontier.cursor().piece_index(),
            1,
            DRAFT_COMPOSER_INPUT_MAX_BYTES,
        )?;
        let piece = page
            .pieces()
            .first()
            .ok_or(DraftComposerMaterializationErrorV1::InvalidBuild)?;
        let drained = drain_record(&build, piece)?;
        let Some((next_frontier, text_span, output_piece)) = drained else {
            let next_frontier = DraftComposerRecordFrontierV1::new(
                frontier.cursor(),
                frontier.encoded_bytes(),
                frontier.logical_utf8_bytes(),
                frontier.piece_count(),
                frontier.marker_count(),
                frontier.marker_digest(),
                frontier.maximum_image_label(),
                build.output_encoded_bytes(),
                checked_next(frontier.chunk_ordinal())?,
                frontier.break_before(),
            );
            let next = copy_build(
                &build,
                build.encoder().clone(),
                next_frontier,
                build.output(),
                build.output_revision(),
                build.output_chunk_count(),
                build.output_encoded_bytes(),
                build.output_chain_digest(),
                DraftComposerBuildLifecycleV1::Open(DraftComposerBuildPhaseV1::Writing),
            );
            return prepared(
                build,
                next,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                page.records_read(),
                page.payload_bytes(),
            );
        };
        let drained_all = next_frontier.encoded_bytes() == build.output_encoded_bytes();
        let phase = if drained_all {
            if final_chunk && next_frontier.cursor().piece_index() == source.summary().piece_count()
            {
                DraftComposerBuildPhaseV1::ReadyToSeal
            } else {
                DraftComposerBuildPhaseV1::Writing
            }
        } else {
            DraftComposerBuildPhaseV1::Draining { final_chunk }
        };
        let next_frontier = if drained_all && !final_chunk {
            DraftComposerRecordFrontierV1::new(
                next_frontier.cursor(),
                next_frontier.encoded_bytes(),
                next_frontier.logical_utf8_bytes(),
                next_frontier.piece_count(),
                next_frontier.marker_count(),
                next_frontier.marker_digest(),
                next_frontier.maximum_image_label(),
                build.output_encoded_bytes(),
                checked_next(next_frontier.chunk_ordinal())?,
                next_frontier.break_before(),
            )
        } else {
            next_frontier
        };
        let next = copy_build(
            &build,
            build.encoder().clone(),
            next_frontier,
            build.output(),
            build.output_revision(),
            build.output_chunk_count(),
            build.output_encoded_bytes(),
            build.output_chain_digest(),
            DraftComposerBuildLifecycleV1::Open(phase),
        );
        prepared(
            build,
            next,
            None,
            None,
            None,
            None,
            text_span,
            Some(output_piece),
            None,
            page.records_read(),
            page.payload_bytes(),
        )
    }

    fn prepare_seal_step(
        &self,
        store: &HomeStore,
        build: DraftComposerBuildRecordV1,
    ) -> Result<PreparedDraftComposerStepV1, DraftComposerMaterializationErrorV1> {
        let output = build
            .output()
            .ok_or(DraftComposerMaterializationErrorV1::InvalidBuild)?;
        let manifest = self
            .point::<ContentManifestsFamily>(
                store,
                output.id(),
                storage_point_limit::<ContentManifestsFamily>(),
            )?
            .ok_or(DraftComposerMaterializationErrorV1::InvalidOutput)?;
        validate_build_manifest(&build, &manifest)?;
        let eof = read_materialization_page(
            self,
            store,
            build.key().source(),
            build.key().source().summary().piece_count(),
            1,
            1,
        )?;
        if !eof.pieces().is_empty()
            || build.encoder().cursor().piece_index()
                != build.key().source().summary().piece_count()
            || build.encoder().cursor().atom_encoded_offset() != 0
            || !build.encoder().carry().is_empty()
            || build.records().cursor().piece_index()
                != build.key().source().summary().piece_count()
            || build.records().encoded_bytes() != output.summary().encoded_bytes()
            || build.records().logical_utf8_bytes() != output.summary().logical_utf8_bytes()
            || build.records().piece_count() != output.summary().piece_count()
            || build.records().marker_count() != output.summary().image_marker_count()
            || build.records().marker_digest() != output.summary().marker_digest()
            || build.records().maximum_image_label() != output.summary().maximum_image_label()
            || manifest.chunk_count() != output.summary().chunk_count()
            || manifest.encoded_bytes() != output.summary().encoded_bytes()
            || manifest.chain_digest() != output.summary().digest()
        {
            return Err(DraftComposerMaterializationErrorV1::InvalidBuild);
        }
        let revision = manifest
            .revision()
            .checked_next()
            .map_err(|_| DraftComposerMaterializationErrorV1::LengthOverflow)?;
        let sealed_manifest = ContentManifestRecord::new(
            manifest.id(),
            revision,
            ContentEncoding::ComposerV1,
            ContentLifecycle::Sealed,
            manifest.chunk_count(),
            manifest.encoded_bytes(),
            manifest.chain_digest(),
            manifest.expected(),
        );
        let reference = sealed_manifest
            .sealed_reference()
            .ok_or(DraftComposerMaterializationErrorV1::InvalidOutput)?;
        let mapping = DraftComposerMaterializationRecordV1::new(
            DraftComposerMaterializationKeyV1::new(build.key().source(), build.key().format()),
            build.key().operation(),
            reference,
        );
        let next = copy_build(
            &build,
            build.encoder().clone(),
            build.records(),
            Some(reference),
            Some(revision),
            build.output_chunk_count(),
            build.output_encoded_bytes(),
            build.output_chain_digest(),
            DraftComposerBuildLifecycleV1::Sealed(reference),
        );
        prepared(
            build,
            next,
            Some(manifest),
            Some(sealed_manifest),
            None,
            None,
            None,
            None,
            Some(mapping),
            eof.records_read(),
            0,
        )
    }
}

fn advance_encoder(
    work: &mut EncoderWork,
    pieces: &[DraftPieceLeafValueV1],
    total_pieces: u64,
    content_id: SyndicContentId,
) -> Result<Option<(ContentChunkRecord, ContentByteSpanRecord)>, DraftComposerMaterializationErrorV1>
{
    let first = work.cursor.piece_index();
    for (page_index, piece) in pieces.iter().enumerate() {
        let expected = first
            .checked_add(page_index as u64)
            .ok_or(DraftComposerMaterializationErrorV1::LengthOverflow)?;
        if work.cursor.piece_index() != expected {
            break;
        }
        loop {
            if work.carry.len() == crate::CONTENT_CHUNK_MAX_BYTES {
                return work.flush(content_id);
            }
            let before_cursor = work.cursor;
            let before_carry = work.carry.len();
            let offset = usize::try_from(work.cursor.atom_encoded_offset())
                .map_err(|_| DraftComposerMaterializationErrorV1::LengthOverflow)?;
            let complete = match piece {
                DraftPieceLeafValueV1::Text(text) => {
                    if text.as_bytes().contains(&0) {
                        return Err(DraftComposerMaterializationErrorV1::InvalidBuild);
                    }
                    advance_text_atom(work, text, offset)?
                }
                DraftPieceLeafValueV1::Marker(marker) => {
                    advance_marker_atom(work, *marker, offset)?
                }
            };
            if work.carry.len() == crate::CONTENT_CHUNK_MAX_BYTES {
                return work.flush(content_id);
            }
            if complete {
                let next = checked_next(expected)?;
                work.cursor = DraftComposerSourceCursorV1::new(next, 0);
                work.source_piece_count = next;
                break;
            }
            if work.cursor == before_cursor && work.carry.len() == before_carry {
                return work.flush(content_id);
            }
            if work.carry.len() >= DRAFT_COMPOSER_CARRY_MAX_BYTES {
                return Ok(None);
            }
        }
    }
    if work.cursor.piece_index() > total_pieces {
        return Err(DraftComposerMaterializationErrorV1::InvalidBuild);
    }
    Ok(None)
}

fn append_raw(
    work: &mut EncoderWork,
    bytes: &[u8],
    offset: usize,
) -> Result<(usize, bool), DraftComposerMaterializationErrorV1> {
    let available = crate::CONTENT_CHUNK_MAX_BYTES - work.carry.len();
    let soft = DRAFT_COMPOSER_CARRY_MAX_BYTES.saturating_sub(work.carry.len());
    let take = available.min(soft).min(bytes.len() - offset);
    work.carry.extend_from_slice(&bytes[offset..offset + take]);
    work.encoded_bytes = work
        .encoded_bytes
        .checked_add(take as u64)
        .ok_or(DraftComposerMaterializationErrorV1::LengthOverflow)?;
    Ok((offset + take, offset + take == bytes.len()))
}

fn advance_text_atom(
    work: &mut EncoderWork,
    text: &str,
    offset: usize,
) -> Result<bool, DraftComposerMaterializationErrorV1> {
    let mut header = [0_u8; 9];
    header[1..].copy_from_slice(&(text.len() as u64).to_be_bytes());
    if offset < header.len() {
        let (next, complete_header) = append_raw(work, &header, offset)?;
        work.cursor = DraftComposerSourceCursorV1::new(work.cursor.piece_index(), next as u64);
        if !complete_header {
            return Ok(false);
        }
    }
    let payload_offset = offset.max(header.len()) - header.len();
    if payload_offset > text.len() || !text.is_char_boundary(payload_offset) {
        return Err(DraftComposerMaterializationErrorV1::InvalidBuild);
    }
    if payload_offset == text.len() {
        work.finalize_text_span()?;
        return Ok(true);
    }
    let available = crate::CONTENT_CHUNK_MAX_BYTES - work.carry.len();
    let soft = DRAFT_COMPOSER_CARRY_MAX_BYTES.saturating_sub(work.carry.len());
    let mut take = available.min(soft).min(text.len() - payload_offset);
    while take != 0 && !text.is_char_boundary(payload_offset + take) {
        take -= 1;
    }
    if take == 0 {
        return Ok(false);
    }
    if work.active_encoded_start.is_none() {
        work.active_encoded_start = Some(work.encoded_bytes);
        work.active_logical_start = Some(work.logical_utf8_bytes);
        work.break_before = false;
    }
    work.carry
        .extend_from_slice(&text.as_bytes()[payload_offset..payload_offset + take]);
    work.encoded_bytes = work
        .encoded_bytes
        .checked_add(take as u64)
        .ok_or(DraftComposerMaterializationErrorV1::LengthOverflow)?;
    work.logical_utf8_bytes = work
        .logical_utf8_bytes
        .checked_add(take as u64)
        .ok_or(DraftComposerMaterializationErrorV1::LengthOverflow)?;
    let next = payload_offset + take;
    work.cursor =
        DraftComposerSourceCursorV1::new(work.cursor.piece_index(), (header.len() + next) as u64);
    if next == text.len() {
        work.finalize_text_span()?;
        Ok(true)
    } else {
        Ok(false)
    }
}

fn advance_marker_atom(
    work: &mut EncoderWork,
    marker: crate::DraftPieceMarkerV1,
    offset: usize,
) -> Result<bool, DraftComposerMaterializationErrorV1> {
    let mut encoded = [0_u8; 25];
    encoded[0] = 1;
    encoded[1..17].copy_from_slice(marker.marker_id().as_bytes());
    encoded[17..].copy_from_slice(&marker.label().get().to_be_bytes());
    if offset > encoded.len() {
        return Err(DraftComposerMaterializationErrorV1::InvalidBuild);
    }
    let (next, complete) = append_raw(work, &encoded, offset)?;
    work.cursor = DraftComposerSourceCursorV1::new(work.cursor.piece_index(), next as u64);
    if complete {
        work.marker_count = checked_next(work.marker_count)?;
        work.piece_count = checked_next(work.piece_count)?;
        work.marker_digest = beryl_model::advance_sequential_marker_digest(
            work.marker_digest,
            marker.marker_id(),
            marker.label(),
        );
        work.maximum_image_label = Some(
            work.maximum_image_label
                .map_or(marker.label(), |current| current.max(marker.label())),
        );
        work.break_before = true;
    }
    Ok(complete)
}

type DrainedRecord = (
    DraftComposerRecordFrontierV1,
    Option<ContentTextSpanRecord>,
    ContentPieceRecord,
);

fn drain_record(
    build: &DraftComposerBuildRecordV1,
    source: &DraftPieceLeafValueV1,
) -> Result<Option<DrainedRecord>, DraftComposerMaterializationErrorV1> {
    let content = build
        .output()
        .ok_or(DraftComposerMaterializationErrorV1::InvalidBuild)?;
    let frontier = build.records();
    let rank = frontier.cursor().piece_index();
    let atom_ordinal = ComposerAtomOrdinal::new(checked_next(rank)?)
        .map_err(|_| DraftComposerMaterializationErrorV1::LengthOverflow)?;
    match source {
        DraftPieceLeafValueV1::Text(text) => {
            let offset = usize::try_from(frontier.cursor().atom_encoded_offset())
                .map_err(|_| DraftComposerMaterializationErrorV1::LengthOverflow)?;
            if offset > text.len() || !text.is_char_boundary(offset) {
                return Err(DraftComposerMaterializationErrorV1::InvalidBuild);
            }
            let encoded_start = if offset == 0 {
                frontier
                    .encoded_bytes()
                    .checked_add(9)
                    .ok_or(DraftComposerMaterializationErrorV1::LengthOverflow)?
            } else {
                frontier.encoded_bytes()
            };
            if encoded_start >= build.output_encoded_bytes() {
                return Ok(None);
            }
            let available = build.output_encoded_bytes() - encoded_start;
            let mut take = usize::try_from(available)
                .unwrap_or(usize::MAX)
                .min(text.len() - offset);
            while take != 0 && !text.is_char_boundary(offset + take) {
                take -= 1;
            }
            if take == 0 {
                return Ok(None);
            }
            let logical_end = frontier
                .logical_utf8_bytes()
                .checked_add(take as u64)
                .ok_or(DraftComposerMaterializationErrorV1::LengthOverflow)?;
            let encoded_end = encoded_start
                .checked_add(take as u64)
                .ok_or(DraftComposerMaterializationErrorV1::LengthOverflow)?;
            let piece_count = checked_next(frontier.piece_count())?;
            let piece_ordinal = ContentPieceOrdinal::new(piece_count)
                .map_err(|_| DraftComposerMaterializationErrorV1::LengthOverflow)?;
            let chunk_ordinal = ContentChunkOrdinal::new(frontier.chunk_ordinal())
                .map_err(|_| DraftComposerMaterializationErrorV1::LengthOverflow)?;
            let digest: [u8; 32] = Sha256::digest(&text.as_bytes()[offset..offset + take]).into();
            let span = ContentTextSpanRecord::new(
                content.id(),
                piece_ordinal,
                chunk_ordinal,
                frontier.chunk_start(),
                frontier.logical_utf8_bytes(),
                logical_end,
                encoded_start,
                encoded_end,
                frontier.break_before(),
                digest,
            )
            .map_err(|_| DraftComposerMaterializationErrorV1::InvalidOutput)?;
            let complete = offset + take == text.len();
            let cursor = if complete {
                DraftComposerSourceCursorV1::new(checked_next(rank)?, 0)
            } else {
                DraftComposerSourceCursorV1::new(rank, (offset + take) as u64)
            };
            let next = DraftComposerRecordFrontierV1::new(
                cursor,
                encoded_end,
                logical_end,
                piece_count,
                frontier.marker_count(),
                frontier.marker_digest(),
                frontier.maximum_image_label(),
                frontier.chunk_start(),
                frontier.chunk_ordinal(),
                false,
            );
            Ok(Some((next, Some(span), ContentPieceRecord::text(span))))
        }
        DraftPieceLeafValueV1::Marker(marker) => {
            let encoded_start = frontier.encoded_bytes();
            let encoded_end = encoded_start
                .checked_add(25)
                .ok_or(DraftComposerMaterializationErrorV1::LengthOverflow)?;
            if encoded_end > build.output_encoded_bytes() {
                return Ok(None);
            }
            let marker_count = checked_next(frontier.marker_count())?;
            let marker_ordinal = InputMarkerOrdinal::new(marker_count)
                .map_err(|_| DraftComposerMaterializationErrorV1::LengthOverflow)?;
            let piece_count = checked_next(frontier.piece_count())?;
            let piece_ordinal = ContentPieceOrdinal::new(piece_count)
                .map_err(|_| DraftComposerMaterializationErrorV1::LengthOverflow)?;
            let mut encoded = [0_u8; 25];
            encoded[0] = 1;
            encoded[1..17].copy_from_slice(marker.marker_id().as_bytes());
            encoded[17..].copy_from_slice(&marker.label().get().to_be_bytes());
            let piece = ContentPieceRecord::image_marker(
                content.id(),
                piece_ordinal,
                atom_ordinal,
                marker_ordinal,
                frontier.logical_utf8_bytes(),
                encoded_start,
                encoded_end,
                marker.marker_id(),
                marker.label(),
                Sha256::digest(encoded).into(),
            )
            .map_err(|_| DraftComposerMaterializationErrorV1::InvalidOutput)?;
            let digest = beryl_model::advance_sequential_marker_digest(
                frontier.marker_digest(),
                marker.marker_id(),
                marker.label(),
            );
            let maximum = Some(
                frontier
                    .maximum_image_label()
                    .map_or(marker.label(), |current| current.max(marker.label())),
            );
            let next = DraftComposerRecordFrontierV1::new(
                DraftComposerSourceCursorV1::new(checked_next(rank)?, 0),
                encoded_end,
                frontier.logical_utf8_bytes(),
                piece_count,
                marker_count,
                digest,
                maximum,
                frontier.chunk_start(),
                frontier.chunk_ordinal(),
                true,
            );
            Ok(Some((next, None, piece)))
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn copy_build(
    source: &DraftComposerBuildRecordV1,
    encoder: DraftComposerEncoderStateV1,
    records: DraftComposerRecordFrontierV1,
    output: Option<ContentReference>,
    output_revision: Option<ContentRevision>,
    output_chunk_count: u64,
    output_encoded_bytes: u64,
    output_chain_digest: SyndicContentDigest,
    lifecycle: DraftComposerBuildLifecycleV1,
) -> DraftComposerBuildRecordV1 {
    DraftComposerBuildRecordV1::new(
        source.key(),
        encoder,
        records,
        output,
        output_revision,
        output_chunk_count,
        output_encoded_bytes,
        output_chain_digest,
        lifecycle,
    )
}

#[allow(clippy::too_many_arguments)]
fn prepared(
    expected: DraftComposerBuildRecordV1,
    next: DraftComposerBuildRecordV1,
    expected_manifest: Option<ContentManifestRecord>,
    next_manifest: Option<ContentManifestRecord>,
    chunk: Option<ContentChunkRecord>,
    byte_span: Option<ContentByteSpanRecord>,
    text_span: Option<ContentTextSpanRecord>,
    piece: Option<ContentPieceRecord>,
    mapping: Option<DraftComposerMaterializationRecordV1>,
    records_read: u64,
    input_payload_bytes: usize,
) -> Result<PreparedDraftComposerStepV1, DraftComposerMaterializationErrorV1> {
    let resident_bytes = expected
        .encoder()
        .carry()
        .len()
        .checked_add(next.encoder().carry().len())
        .and_then(|value| value.checked_add(input_payload_bytes))
        .and_then(|value| value.checked_add(chunk.as_ref().map_or(0, |value| value.bytes().len())))
        .ok_or(DraftComposerMaterializationErrorV1::LengthOverflow)?;
    let written_records = 1
        + usize::from(next_manifest.is_some())
        + usize::from(chunk.is_some())
        + usize::from(byte_span.is_some())
        + usize::from(text_span.is_some())
        + usize::from(piece.is_some())
        + usize::from(mapping.is_some());
    if resident_bytes > DRAFT_COMPOSER_RESIDENT_MAX_BYTES
        || records_read > DRAFT_COMPOSER_READ_MAX_RECORDS
        || written_records > DRAFT_COMPOSER_WRITE_MAX_RECORDS
    {
        return Err(DraftComposerMaterializationErrorV1::InvalidBuild);
    }
    Ok(PreparedDraftComposerStepV1 {
        expected,
        next,
        expected_manifest,
        next_manifest,
        chunk,
        byte_span,
        text_span,
        piece,
        mapping,
        records_read,
        input_payload_bytes,
        resident_bytes,
    })
}

fn validate_build_identity(
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
            if output.id() != content_id_for(key)?
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

fn validate_mapping(
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

fn validate_sealed_mapping_closure(
    storage: &SyndicStorage,
    store: &HomeStore,
    key: DraftComposerMaterializationKeyV1,
    mapping: DraftComposerMaterializationRecordV1,
) -> Result<(), DraftComposerMaterializationErrorV1> {
    validate_mapping(key, mapping)?;
    let source = storage
        .point::<super::super::codec::DraftPieceRootsFamily>(
            store,
            key.source().key(),
            storage_point_limit::<super::super::codec::DraftPieceRootsFamily>(),
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
        || mapping.content().id() != content_id_for(origin_key)?
    {
        return Err(DraftComposerMaterializationErrorV1::InvalidBuild);
    }
    validate_sealed_content(storage, store, mapping.content())
}

fn validate_build_manifest(
    build: &DraftComposerBuildRecordV1,
    manifest: &ContentManifestRecord,
) -> Result<(), DraftComposerMaterializationErrorV1> {
    let reference = build
        .output()
        .ok_or(DraftComposerMaterializationErrorV1::InvalidBuild)?;
    if manifest.id() != reference.id()
        || manifest.owner().is_some()
        || manifest.encoding() != ContentEncoding::ComposerV1
        || manifest.lifecycle() != ContentLifecycle::Building
        || manifest.expected() != reference.summary()
        || manifest.revision()
            != build
                .output_revision()
                .ok_or(DraftComposerMaterializationErrorV1::InvalidBuild)?
        || manifest.chunk_count() != build.output_chunk_count()
        || manifest.encoded_bytes() != build.output_encoded_bytes()
        || manifest.chain_digest() != build.output_chain_digest()
    {
        return Err(DraftComposerMaterializationErrorV1::InvalidOutput);
    }
    Ok(())
}

fn validate_sealed_content(
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

fn validate_output_frontier_records(
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

impl DomainMutation<SyndicDomain> for BeginMutation {
    type Error = SyndicMutationError;

    fn validate(&self, reader: &DomainReader<'_, SyndicDomain>) -> Result<(), Self::Error> {
        let key = self.initial.key();
        let mapping_key = DraftComposerMaterializationKeyV1::new(key.source(), key.format());
        if let Some(mapping) = point::<DraftComposerMaterializationsFamily>(reader, &mapping_key)? {
            validate_mapping_for_mutation(reader, mapping_key, mapping)?;
            return Ok(());
        }
        if let Some(build) = point::<DraftComposerBuildsFamily>(reader, &key)? {
            return if build.key() == key {
                Ok(())
            } else {
                Err(SyndicMutationError::IdentityCollision)
            };
        }
        let Some(source) =
            point::<super::super::codec::DraftPieceRootsFamily>(reader, &key.source().key())?
        else {
            return Err(SyndicMutationError::RequiredRecordMissing {
                family: "draft-piece-roots",
            });
        };
        if source.reference() != key.source() {
            return Err(SyndicMutationError::IdentityCollision);
        }
        Ok(())
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<DraftComposerBuildsCodec>(1)?;
        Ok(())
    }

    fn contribute(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        if point::<DraftComposerBuildsFamily>(reader, &self.initial.key())?.is_none()
            && point::<DraftComposerMaterializationsFamily>(
                reader,
                &DraftComposerMaterializationKeyV1::new(
                    self.initial.key().source(),
                    self.initial.key().format(),
                ),
            )?
            .is_none()
        {
            mutations.put::<DraftComposerBuildsCodec>(&self.initial.key(), &self.initial)?;
        }
        Ok(())
    }
}

fn validate_mapping_for_mutation(
    reader: &DomainReader<'_, SyndicDomain>,
    key: DraftComposerMaterializationKeyV1,
    mapping: DraftComposerMaterializationRecordV1,
) -> Result<(), SyndicMutationError> {
    if validate_mapping(key, mapping).is_err() {
        return Err(SyndicMutationError::IdentityCollision);
    }
    let source = point::<super::super::codec::DraftPieceRootsFamily>(reader, &key.source().key())?
        .ok_or(SyndicMutationError::RequiredRecordMissing {
            family: "draft-piece-roots",
        })?;
    if source.reference() != key.source() {
        return Err(SyndicMutationError::IdentityCollision);
    }
    let origin_key =
        DraftComposerBuildKeyV1::new(key.source(), key.format(), mapping.sealing_operation());
    let origin = point::<DraftComposerBuildsFamily>(reader, &origin_key)?.ok_or(
        SyndicMutationError::RequiredRecordMissing {
            family: "draft-composer-builds",
        },
    )?;
    if validate_build_identity(origin_key, &origin).is_err()
        || origin.lifecycle() != &DraftComposerBuildLifecycleV1::Sealed(mapping.content())
        || origin.output() != Some(mapping.content())
        || content_id_for(origin_key).ok() != Some(mapping.content().id())
    {
        return Err(SyndicMutationError::IdentityCollision);
    }
    let manifest = point::<ContentManifestsFamily>(reader, &mapping.content().id())?.ok_or(
        SyndicMutationError::RequiredRecordMissing {
            family: "content-manifests",
        },
    )?;
    if manifest.sealed_reference() != Some(mapping.content()) || manifest.owner().is_some() {
        return Err(SyndicMutationError::IdentityCollision);
    }
    Ok(())
}

impl DomainMutation<SyndicDomain> for StepMutation {
    type Error = SyndicMutationError;

    fn validate(&self, reader: &DomainReader<'_, SyndicDomain>) -> Result<(), Self::Error> {
        let prepared = &self.prepared;
        if point::<DraftComposerBuildsFamily>(reader, &prepared.expected.key())?
            != Some(prepared.expected.clone())
        {
            return Err(SyndicMutationError::IdentityCollision);
        }
        match (&prepared.expected_manifest, &prepared.next_manifest) {
            (Some(expected), Some(_)) => {
                if point::<ContentManifestsFamily>(reader, &expected.id())?
                    != Some(expected.clone())
                {
                    return Err(SyndicMutationError::ContentManifestConflict);
                }
            }
            (None, Some(next)) => {
                if point::<ContentManifestsFamily>(reader, &next.id())?.is_some() {
                    return Err(SyndicMutationError::ContentIdentityCollision);
                }
            }
            (_, None) => {}
        }
        validate_exact_or_absent_records(reader, prepared)?;
        if let Some(mapping) = prepared.mapping {
            if let Some(existing) =
                point::<DraftComposerMaterializationsFamily>(reader, &mapping.key())?
                && existing != mapping
            {
                return Err(SyndicMutationError::IdentityCollision);
            }
        }
        Ok(())
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<DraftComposerBuildsCodec>(1)?;
        if self.prepared.next_manifest.is_some() {
            reservation.reserve_records::<ContentManifestsCodec>(1)?;
        }
        if self.prepared.chunk.is_some() {
            reservation.reserve_records::<ContentChunksCodec>(1)?;
        }
        if self.prepared.byte_span.is_some() {
            reservation.reserve_records::<ContentByteSpansCodec>(1)?;
        }
        if self.prepared.text_span.is_some() {
            reservation.reserve_records::<ContentTextSpansCodec>(1)?;
        }
        if self.prepared.piece.is_some() {
            reservation.reserve_records::<ContentPiecesCodec>(1)?;
        }
        if self.prepared.mapping.is_some() {
            reservation.reserve_records::<DraftComposerMaterializationsCodec>(1)?;
        }
        Ok(())
    }

    fn contribute(
        &self,
        _reader: &DomainReader<'_, SyndicDomain>,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        let p = &self.prepared;
        if let Some(manifest) = &p.next_manifest {
            mutations.put::<ContentManifestsCodec>(&manifest.id(), manifest)?;
        }
        if let Some(chunk) = &p.chunk {
            mutations.put::<ContentChunksCodec>(
                &ContentChunkKey {
                    owner: chunk.content_id(),
                    ordinal: chunk.ordinal(),
                },
                chunk,
            )?;
        }
        if let Some(span) = &p.byte_span {
            mutations.put::<ContentByteSpansCodec>(
                &ContentByteSpanKey {
                    owner: span.content_id(),
                    start: span.start(),
                },
                span,
            )?;
        }
        if let Some(span) = &p.text_span {
            mutations.put::<ContentTextSpansCodec>(
                &ContentTextSpanKey {
                    owner: span.content_id(),
                    logical_start: span.logical_start(),
                },
                span,
            )?;
        }
        if let Some(piece) = &p.piece {
            mutations.put::<ContentPiecesCodec>(
                &ContentPieceKey {
                    owner: piece.content_id(),
                    ordinal: piece.ordinal(),
                },
                piece,
            )?;
        }
        if let Some(mapping) = &p.mapping {
            mutations.put::<DraftComposerMaterializationsCodec>(&mapping.key(), mapping)?;
        }
        mutations.put::<DraftComposerBuildsCodec>(&p.next.key(), &p.next)?;
        Ok(())
    }
}

fn validate_exact_or_absent_records(
    reader: &DomainReader<'_, SyndicDomain>,
    p: &PreparedDraftComposerStepV1,
) -> Result<(), SyndicMutationError> {
    if let Some(chunk) = &p.chunk {
        let existing = point::<ContentChunksFamily>(
            reader,
            &ContentChunkKey {
                owner: chunk.content_id(),
                ordinal: chunk.ordinal(),
            },
        )?;
        if existing.is_some() && existing.as_ref() != Some(chunk) {
            return Err(SyndicMutationError::ContentChunkConflict);
        }
    }
    if let Some(span) = &p.byte_span {
        let existing = point::<ContentByteSpansFamily>(
            reader,
            &ContentByteSpanKey {
                owner: span.content_id(),
                start: span.start(),
            },
        )?;
        if existing.is_some() && existing.as_ref() != Some(span) {
            return Err(SyndicMutationError::ContentChunkConflict);
        }
    }
    if let Some(span) = &p.text_span {
        let existing = point::<ContentTextSpansFamily>(
            reader,
            &ContentTextSpanKey {
                owner: span.content_id(),
                logical_start: span.logical_start(),
            },
        )?;
        if existing.is_some() && existing.as_ref() != Some(span) {
            return Err(SyndicMutationError::ContentChunkConflict);
        }
    }
    if let Some(piece) = &p.piece {
        let existing = point::<ContentPiecesFamily>(
            reader,
            &ContentPieceKey {
                owner: piece.content_id(),
                ordinal: piece.ordinal(),
            },
        )?;
        if existing.is_some() && existing.as_ref() != Some(piece) {
            return Err(SyndicMutationError::ContentChunkConflict);
        }
    }
    Ok(())
}

impl DomainMutation<SyndicDomain> for TerminalMutation {
    type Error = SyndicMutationError;

    fn validate(&self, reader: &DomainReader<'_, SyndicDomain>) -> Result<(), Self::Error> {
        if matches!(self.kind, TerminalKind::Supersede(successor) if successor == self.key.operation())
        {
            return Err(SyndicMutationError::IdentityCollision);
        }
        let build = point::<DraftComposerBuildsFamily>(reader, &self.key)?.ok_or(
            SyndicMutationError::RequiredRecordMissing {
                family: "draft-composer-builds",
            },
        )?;
        match build.lifecycle() {
            DraftComposerBuildLifecycleV1::Open(_) => Ok(()),
            DraftComposerBuildLifecycleV1::Cancelled
                if matches!(self.kind, TerminalKind::Cancel) =>
            {
                Ok(())
            }
            DraftComposerBuildLifecycleV1::Failed(_) if matches!(self.kind, TerminalKind::Fail) => {
                Ok(())
            }
            DraftComposerBuildLifecycleV1::Superseded(existing) if matches!(self.kind, TerminalKind::Supersede(value) if value == *existing) => {
                Ok(())
            }
            _ => Err(SyndicMutationError::IdentityCollision),
        }
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<DraftComposerBuildsCodec>(1)?;
        Ok(())
    }

    fn contribute(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        let build = point::<DraftComposerBuildsFamily>(reader, &self.key)?.ok_or(
            SyndicMutationError::RequiredRecordMissing {
                family: "draft-composer-builds",
            },
        )?;
        if matches!(build.lifecycle(), DraftComposerBuildLifecycleV1::Open(_)) {
            let lifecycle = match self.kind {
                TerminalKind::Cancel => DraftComposerBuildLifecycleV1::Cancelled,
                TerminalKind::Fail => {
                    DraftComposerBuildLifecycleV1::Failed(DraftComposerFailureReasonV1::Operational)
                }
                TerminalKind::Supersede(successor) => {
                    DraftComposerBuildLifecycleV1::Superseded(successor)
                }
            };
            let next = copy_build(
                &build,
                build.encoder().clone(),
                build.records(),
                build.output(),
                build.output_revision(),
                build.output_chunk_count(),
                build.output_encoded_bytes(),
                build.output_chain_digest(),
                lifecycle,
            );
            mutations.put::<DraftComposerBuildsCodec>(&self.key, &next)?;
        }
        Ok(())
    }
}

impl From<Infallible> for DraftComposerMaterializationErrorV1 {
    fn from(value: Infallible) -> Self {
        match value {}
    }
}
