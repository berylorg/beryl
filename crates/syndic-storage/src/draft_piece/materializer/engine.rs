use std::convert::Infallible;

use beryl_home_store::{
    DomainMutation, DomainReader, HomeStore, MutationBuilder, MutationContribution,
    ReconciliationReservation,
};
use beryl_model::{ContentRevision, DomainRevision, SyndicContentDigest, SyndicContentId};

use crate::{
    ContentByteSpanRecord, ContentChunkOrdinal, ContentChunkRecord, ContentEncoding,
    ContentLifecycle, ContentManifestRecord, ContentPieceOrdinal, ContentPieceRecord,
    ContentReference, ContentSummary, ContentTextSpanRecord, DraftPieceLeafValueV1,
    SyndicMutationError, SyndicPointReadLimit, SyndicStorage, advance_content_chain,
    content_chain_seed, draft_piece::read_materialization_page,
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

mod drain;
mod preparation;
mod step;
mod validation;

use drain::drain_record;
use validation::*;

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

fn content_id_for_summary(summary: ContentSummary) -> SyndicContentId {
    SyndicContentId::from_digest(*summary.digest().as_bytes())
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

impl DomainMutation<SyndicDomain> for BeginMutation {
    type Error = SyndicMutationError;
    type Prepared = Option<DraftComposerBuildRecordV1>;

    fn prepare(
        self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        let key = self.initial.key();
        let mapping_key = DraftComposerMaterializationKeyV1::new(key.source(), key.format());
        if let Some(mapping) = point::<DraftComposerMaterializationsFamily>(reader, &mapping_key)? {
            validate_mapping_for_mutation(reader, mapping_key, mapping)?;
            return Ok(None);
        }
        if let Some(build) = point::<DraftComposerBuildsFamily>(reader, &key)? {
            return if build.key() == key {
                Ok(None)
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
        Ok(Some(self.initial))
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<DraftComposerBuildsCodec>(1)?;
        Ok(())
    }

    fn contribute(
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        if let Some(prepared) = prepared {
            mutations.put::<DraftComposerBuildsCodec>(&prepared.key(), &prepared)?;
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
        || content_id_for_summary(mapping.content().summary()) != mapping.content().id()
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

impl DomainMutation<SyndicDomain> for TerminalMutation {
    type Error = SyndicMutationError;
    type Prepared = Option<(DraftComposerBuildKeyV1, DraftComposerBuildRecordV1)>;

    fn prepare(
        self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
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
            DraftComposerBuildLifecycleV1::Open(_) => {
                let lifecycle = match self.kind {
                    TerminalKind::Cancel => DraftComposerBuildLifecycleV1::Cancelled,
                    TerminalKind::Fail => DraftComposerBuildLifecycleV1::Failed(
                        DraftComposerFailureReasonV1::Operational,
                    ),
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
                Ok(Some((self.key, next)))
            }
            DraftComposerBuildLifecycleV1::Cancelled
                if matches!(self.kind, TerminalKind::Cancel) =>
            {
                Ok(None)
            }
            DraftComposerBuildLifecycleV1::Failed(_) if matches!(self.kind, TerminalKind::Fail) => {
                Ok(None)
            }
            DraftComposerBuildLifecycleV1::Superseded(existing) if matches!(self.kind, TerminalKind::Supersede(value) if value == *existing) => {
                Ok(None)
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
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        if let Some((key, build)) = prepared {
            mutations.put::<DraftComposerBuildsCodec>(&key, &build)?;
        }
        Ok(())
    }
}

impl From<Infallible> for DraftComposerMaterializationErrorV1 {
    fn from(value: Infallible) -> Self {
        match value {}
    }
}
