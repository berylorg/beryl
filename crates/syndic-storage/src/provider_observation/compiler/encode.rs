mod item;
mod structured;

use beryl_model::{CasItemId, CasThreadId};
use sha2::{Digest, Sha256};

use crate::{
    PROVIDER_FRAME_CHUNK_MAX_BYTES, ProviderFrameOrdinalV1, ProviderFrameReferenceV1,
    ProviderFrameSinkV1, ProviderFrameTextSpanV1, ProviderItemKind, ProviderItemValidationError,
    ProviderLogicalTextRoleV1,
};

use super::ProviderObservationFrameSemanticError;
use super::replay::{
    FieldSelector, ObservationReplayReader, ReplayError, ReplayWriteError, TextSelector,
};

const MAGIC: &[u8; 4] = b"PIV1";
const TEXT_INLINE: u8 = 0;

pub(super) enum ObservationEncodeError<E> {
    Replay(ReplayError),
    Validation(ProviderItemValidationError),
    Sink(E),
}

impl<E> From<ReplayError> for ObservationEncodeError<E> {
    fn from(value: ReplayError) -> Self {
        Self::Replay(value)
    }
}

impl<E> From<ProviderItemValidationError> for ObservationEncodeError<E> {
    fn from(value: ProviderItemValidationError) -> Self {
        Self::Validation(value)
    }
}

pub(super) fn encode_observation<S: ProviderFrameSinkV1>(
    reader: &ObservationReplayReader<'_>,
    item_id: &CasItemId,
    ordinal: ProviderFrameOrdinalV1,
    kind: ProviderItemKind,
    prior_content_frontier: u64,
    sink: &mut S,
) -> Result<ProviderFrameReferenceV1, ObservationEncodeError<S::Error>> {
    compare_item_identity(reader, item_id)?;
    let mut encoder = Encoder::new(reader, sink, prior_content_frontier, ordinal, item_id, kind);
    encoder.bytes(MAGIC)?;
    encoder.u64(ordinal.get())?;
    encoder.bounded_identity_bytes(item_id.as_str().as_bytes())?;
    encoder.observation()?;
    encoder.finish()
}

fn compare_item_identity<E>(
    reader: &ObservationReplayReader<'_>,
    item_id: &CasItemId,
) -> Result<(), ObservationEncodeError<E>> {
    let selector = TextSelector::Field(FieldSelector::top(super::super::ProviderField::ItemId));
    let expected = item_id.as_str().as_bytes();
    let summary = reader.text_summary(selector)?;
    if summary.bytes != u64::try_from(expected.len()).expect("bounded identity length fits u64") {
        return Err(ObservationEncodeError::Replay(ReplayError::Semantic(
            ProviderObservationFrameSemanticError::ItemIdentityMismatch,
        )));
    }
    let mut offset = 0_usize;
    reader
        .write_text(selector, |fragment| {
            let end = offset.checked_add(fragment.len()).ok_or(())?;
            if expected.get(offset..end) != Some(fragment) {
                return Err(());
            }
            offset = end;
            Ok(())
        })
        .map_err(|error| match error {
            ReplayWriteError::Replay(error) => ObservationEncodeError::Replay(error),
            ReplayWriteError::Output(()) => ObservationEncodeError::Replay(ReplayError::Semantic(
                ProviderObservationFrameSemanticError::ItemIdentityMismatch,
            )),
        })?;
    if offset != expected.len() {
        return Err(ObservationEncodeError::Replay(ReplayError::Semantic(
            ProviderObservationFrameSemanticError::ItemIdentityMismatch,
        )));
    }
    Ok(())
}

pub(super) struct Encoder<'a, 's, S: ProviderFrameSinkV1> {
    reader: &'a ObservationReplayReader<'a>,
    sink: &'s mut S,
    chunk: Vec<u8>,
    hasher: Sha256,
    encoded_start: u64,
    encoded_bytes: u64,
    frame_ordinal: ProviderFrameOrdinalV1,
    logical_frontier: u64,
    text_span_count: u64,
    item_id: &'a CasItemId,
    kind: ProviderItemKind,
}

impl<'a, 's, S: ProviderFrameSinkV1> Encoder<'a, 's, S> {
    fn new(
        reader: &'a ObservationReplayReader<'a>,
        sink: &'s mut S,
        encoded_start: u64,
        frame_ordinal: ProviderFrameOrdinalV1,
        item_id: &'a CasItemId,
        kind: ProviderItemKind,
    ) -> Self {
        Self {
            reader,
            sink,
            chunk: Vec::with_capacity(PROVIDER_FRAME_CHUNK_MAX_BYTES),
            hasher: Sha256::new(),
            encoded_start,
            encoded_bytes: 0,
            frame_ordinal,
            logical_frontier: 0,
            text_span_count: 0,
            item_id,
            kind,
        }
    }

    fn finish(mut self) -> Result<ProviderFrameReferenceV1, ObservationEncodeError<S::Error>> {
        self.flush()?;
        let encoded_end = self
            .encoded_start
            .checked_add(self.encoded_bytes)
            .ok_or(ProviderItemValidationError::FrameLengthOverflow)?;
        ProviderFrameReferenceV1::new(
            self.item_id.clone(),
            self.kind,
            self.frame_ordinal,
            self.encoded_start,
            encoded_end,
            self.hasher.finalize().into(),
            self.logical_frontier,
            self.text_span_count,
        )
        .map_err(Into::into)
    }

    fn flush(&mut self) -> Result<(), ObservationEncodeError<S::Error>> {
        if !self.chunk.is_empty() {
            self.sink
                .write_chunk(&self.chunk)
                .map_err(ObservationEncodeError::Sink)?;
            self.chunk.clear();
        }
        Ok(())
    }

    pub(super) fn bytes(
        &mut self,
        mut bytes: &[u8],
    ) -> Result<(), ObservationEncodeError<S::Error>> {
        while !bytes.is_empty() {
            let available = PROVIDER_FRAME_CHUNK_MAX_BYTES - self.chunk.len();
            let take = available.min(bytes.len());
            let (head, tail) = bytes.split_at(take);
            self.chunk.extend_from_slice(head);
            self.hasher.update(head);
            self.encoded_bytes = self
                .encoded_bytes
                .checked_add(u64::try_from(take).expect("bounded chunk length fits u64"))
                .ok_or(ProviderItemValidationError::FrameLengthOverflow)?;
            bytes = tail;
            if self.chunk.len() == PROVIDER_FRAME_CHUNK_MAX_BYTES {
                self.flush()?;
            }
        }
        Ok(())
    }

    pub(super) fn u8(&mut self, value: u8) -> Result<(), ObservationEncodeError<S::Error>> {
        self.bytes(&[value])
    }

    pub(super) fn u32(&mut self, value: u32) -> Result<(), ObservationEncodeError<S::Error>> {
        self.bytes(&value.to_be_bytes())
    }

    pub(super) fn i32(&mut self, value: i32) -> Result<(), ObservationEncodeError<S::Error>> {
        self.bytes(&value.to_be_bytes())
    }

    pub(super) fn u64(&mut self, value: u64) -> Result<(), ObservationEncodeError<S::Error>> {
        self.bytes(&value.to_be_bytes())
    }

    pub(super) fn i64(&mut self, value: i64) -> Result<(), ObservationEncodeError<S::Error>> {
        self.bytes(&value.to_be_bytes())
    }

    fn position(&self) -> Result<u64, ObservationEncodeError<S::Error>> {
        self.encoded_start
            .checked_add(self.encoded_bytes)
            .ok_or(ProviderItemValidationError::FrameLengthOverflow.into())
    }

    pub(super) fn option(
        &mut self,
        present: bool,
        encode: impl FnOnce(&mut Self) -> Result<(), ObservationEncodeError<S::Error>>,
    ) -> Result<(), ObservationEncodeError<S::Error>> {
        if present {
            self.u8(1)?;
            encode(self)
        } else {
            self.u8(0)
        }
    }

    pub(super) fn bounded_identity(
        &mut self,
        selector: TextSelector,
    ) -> Result<(), ObservationEncodeError<S::Error>> {
        let summary = self.reader.text_summary(selector)?;
        let length = u32::try_from(summary.bytes)
            .map_err(|_| ProviderItemValidationError::FrameLengthOverflow)?;
        self.u32(length)?;
        self.replay_text_bytes(selector)
    }

    fn bounded_identity_bytes(
        &mut self,
        bytes: &[u8],
    ) -> Result<(), ObservationEncodeError<S::Error>> {
        let length = u32::try_from(bytes.len())
            .map_err(|_| ProviderItemValidationError::FrameLengthOverflow)?;
        self.u32(length)?;
        self.bytes(bytes)
    }

    pub(super) fn cas_thread_id(
        &mut self,
        selector: TextSelector,
    ) -> Result<(), ObservationEncodeError<S::Error>> {
        self.bounded_identity(selector)
    }

    #[allow(dead_code)]
    fn cas_thread_id_value(
        &mut self,
        value: &CasThreadId,
    ) -> Result<(), ObservationEncodeError<S::Error>> {
        self.bounded_identity_bytes(value.as_str().as_bytes())
    }

    pub(super) fn raw_text(
        &mut self,
        selector: TextSelector,
    ) -> Result<(), ObservationEncodeError<S::Error>> {
        let summary = self.reader.text_summary(selector)?;
        self.u64(summary.bytes)?;
        self.replay_text_bytes(selector)
    }

    pub(super) fn text(
        &mut self,
        selector: TextSelector,
        role: Option<ProviderLogicalTextRoleV1>,
    ) -> Result<(), ObservationEncodeError<S::Error>> {
        let summary = self.reader.text_summary(selector)?;
        self.u8(TEXT_INLINE)?;
        self.u64(summary.bytes)?;
        let source_start = self.position()?;
        self.replay_text_bytes(selector)?;
        let source_end = self.position()?;
        if let Some(role) = role
            && summary.bytes != 0
        {
            let logical_end = self
                .logical_frontier
                .checked_add(summary.bytes)
                .ok_or(ProviderItemValidationError::FrameLengthOverflow)?;
            let span = ProviderFrameTextSpanV1::new(
                self.frame_ordinal,
                self.logical_frontier,
                logical_end,
                source_start,
                source_end,
                summary.digest,
                role,
            )?;
            self.sink
                .write_text_span(span)
                .map_err(ObservationEncodeError::Sink)?;
            self.logical_frontier = logical_end;
            self.text_span_count = self
                .text_span_count
                .checked_add(1)
                .ok_or(ProviderItemValidationError::FrameLengthOverflow)?;
        }
        Ok(())
    }

    fn replay_text_bytes(
        &mut self,
        selector: TextSelector,
    ) -> Result<(), ObservationEncodeError<S::Error>> {
        let reader = self.reader;
        reader
            .write_text(selector, |fragment| self.bytes(fragment))
            .map_err(|error| match error {
                ReplayWriteError::Replay(error) => ObservationEncodeError::Replay(error),
                ReplayWriteError::Output(error) => error,
            })
    }
}
