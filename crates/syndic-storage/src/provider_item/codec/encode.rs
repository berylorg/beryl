mod item;

use beryl_model::{AssetId, AssetIdentityVersion, CasItemId, CasThreadId};
use sha2::{Digest, Sha256};

use super::{
    PROVIDER_FRAME_CHUNK_MAX_BYTES, ProviderFrameEncodeError, ProviderFrameReferenceV1,
    ProviderFrameSinkV1, tags,
};
use crate::provider_item::*;

pub fn encode_provider_item_frame_v1<S: ProviderFrameSinkV1>(
    frame: &ProviderItemFrameV1,
    prior_content_frontier: u64,
    sink: &mut S,
) -> Result<ProviderFrameReferenceV1, ProviderFrameEncodeError<S::Error>> {
    frame.validate(prior_content_frontier)?;
    let mut encoder = Encoder::new(sink, prior_content_frontier, frame.ordinal());
    encoder.bytes(&tags::MAGIC)?;
    encoder.u64(frame.ordinal().get())?;
    encoder.cas_item_id(frame.item_id())?;
    match frame.observation() {
        ProviderItemObservationV1::Started { observed_at, item } => {
            encoder.u8(tags::OBSERVATION_STARTED)?;
            encoder.u64(observed_at.get())?;
            encoder.item(item)?;
        }
        ProviderItemObservationV1::Delta(delta) => {
            encoder.u8(tags::OBSERVATION_DELTA)?;
            encoder.delta(delta)?;
        }
        ProviderItemObservationV1::Completed { observed_at, item } => {
            encoder.u8(tags::OBSERVATION_COMPLETED)?;
            encoder.u64(observed_at.get())?;
            encoder.item(item)?;
        }
    }
    encoder.finish(frame)
}

pub(super) struct Encoder<'a, S: ProviderFrameSinkV1> {
    sink: &'a mut S,
    chunk: Vec<u8>,
    hasher: Sha256,
    encoded_start: u64,
    encoded_bytes: u64,
    frame_ordinal: ProviderFrameOrdinalV1,
    logical_frontier: u64,
    text_span_count: u64,
}

impl<'a, S: ProviderFrameSinkV1> Encoder<'a, S> {
    fn new(sink: &'a mut S, encoded_start: u64, frame_ordinal: ProviderFrameOrdinalV1) -> Self {
        Self {
            sink,
            chunk: Vec::with_capacity(PROVIDER_FRAME_CHUNK_MAX_BYTES),
            hasher: Sha256::new(),
            encoded_start,
            encoded_bytes: 0,
            frame_ordinal,
            logical_frontier: 0,
            text_span_count: 0,
        }
    }

    fn finish(
        mut self,
        frame: &ProviderItemFrameV1,
    ) -> Result<ProviderFrameReferenceV1, ProviderFrameEncodeError<S::Error>> {
        self.flush()?;
        let encoded_end = self
            .encoded_start
            .checked_add(self.encoded_bytes)
            .ok_or(ProviderItemValidationError::FrameLengthOverflow)?;
        let digest: [u8; 32] = self.hasher.finalize().into();
        ProviderFrameReferenceV1::new(
            frame.item_id().clone(),
            frame.kind(),
            frame.ordinal(),
            self.encoded_start,
            encoded_end,
            digest,
            self.logical_frontier,
            self.text_span_count,
        )
        .map_err(ProviderFrameEncodeError::Validation)
    }

    fn flush(&mut self) -> Result<(), ProviderFrameEncodeError<S::Error>> {
        if !self.chunk.is_empty() {
            self.sink
                .write_chunk(&self.chunk)
                .map_err(ProviderFrameEncodeError::Sink)?;
            self.chunk.clear();
        }
        Ok(())
    }

    pub(super) fn bytes(
        &mut self,
        mut bytes: &[u8],
    ) -> Result<(), ProviderFrameEncodeError<S::Error>> {
        while !bytes.is_empty() {
            let available = PROVIDER_FRAME_CHUNK_MAX_BYTES - self.chunk.len();
            let take = available.min(bytes.len());
            let (head, tail) = bytes.split_at(take);
            self.chunk.extend_from_slice(head);
            self.hasher.update(head);
            self.encoded_bytes = self
                .encoded_bytes
                .checked_add(u64::try_from(take).expect("usize chunk length fits u64"))
                .ok_or(ProviderItemValidationError::FrameLengthOverflow)?;
            bytes = tail;
            if self.chunk.len() == PROVIDER_FRAME_CHUNK_MAX_BYTES {
                self.flush()?;
            }
        }
        Ok(())
    }

    pub(super) fn u8(&mut self, value: u8) -> Result<(), ProviderFrameEncodeError<S::Error>> {
        self.bytes(&[value])
    }

    pub(super) fn u32(&mut self, value: u32) -> Result<(), ProviderFrameEncodeError<S::Error>> {
        self.bytes(&value.to_be_bytes())
    }

    pub(super) fn i32(&mut self, value: i32) -> Result<(), ProviderFrameEncodeError<S::Error>> {
        self.bytes(&value.to_be_bytes())
    }

    pub(super) fn u64(&mut self, value: u64) -> Result<(), ProviderFrameEncodeError<S::Error>> {
        self.bytes(&value.to_be_bytes())
    }

    pub(super) fn i64(&mut self, value: i64) -> Result<(), ProviderFrameEncodeError<S::Error>> {
        self.bytes(&value.to_be_bytes())
    }

    fn position(&self) -> Result<u64, ProviderItemValidationError> {
        self.encoded_start
            .checked_add(self.encoded_bytes)
            .ok_or(ProviderItemValidationError::FrameLengthOverflow)
    }

    pub(super) fn raw_text(
        &mut self,
        value: &str,
    ) -> Result<(), ProviderFrameEncodeError<S::Error>> {
        self.u64(u64::try_from(value.len()).expect("usize text length fits u64"))?;
        self.bytes(value.as_bytes())
    }

    pub(super) fn text(
        &mut self,
        value: &ProviderTextV1,
        role: Option<ProviderLogicalTextRoleV1>,
    ) -> Result<(), ProviderFrameEncodeError<S::Error>> {
        let (source_start, source_end, digest) = match value {
            ProviderTextV1::Inline(value) => {
                self.u8(tags::TEXT_INLINE)?;
                self.u64(u64::try_from(value.len()).expect("usize text length fits u64"))?;
                let start = self.position()?;
                self.bytes(value.as_bytes())?;
                let end = self.position()?;
                let digest: [u8; 32] = Sha256::digest(value.as_bytes()).into();
                (start, end, digest)
            }
            ProviderTextV1::Reused(reference) => {
                self.u8(tags::TEXT_REUSED)?;
                self.u64(reference.start())?;
                self.u64(reference.end())?;
                self.bytes(&reference.digest())?;
                (reference.start(), reference.end(), reference.digest())
            }
        };
        if let Some(role) = role {
            let length = source_end
                .checked_sub(source_start)
                .ok_or(ProviderItemValidationError::FrameLengthOverflow)?;
            if length != 0 {
                let logical_end = self
                    .logical_frontier
                    .checked_add(length)
                    .ok_or(ProviderItemValidationError::FrameLengthOverflow)?;
                let span = ProviderFrameTextSpanV1::new(
                    self.frame_ordinal,
                    self.logical_frontier,
                    logical_end,
                    source_start,
                    source_end,
                    digest,
                    role,
                )?;
                self.sink
                    .write_text_span(span)
                    .map_err(ProviderFrameEncodeError::Sink)?;
                self.logical_frontier = logical_end;
                self.text_span_count = self
                    .text_span_count
                    .checked_add(1)
                    .ok_or(ProviderItemValidationError::FrameLengthOverflow)?;
            }
        }
        Ok(())
    }

    pub(super) fn option<T>(
        &mut self,
        value: &Option<T>,
        encode: impl FnOnce(&mut Self, &T) -> Result<(), ProviderFrameEncodeError<S::Error>>,
    ) -> Result<(), ProviderFrameEncodeError<S::Error>> {
        match value {
            None => self.u8(tags::OPTION_NONE),
            Some(value) => {
                self.u8(tags::OPTION_SOME)?;
                encode(self, value)
            }
        }
    }

    pub(super) fn count(&mut self, count: usize) -> Result<(), ProviderFrameEncodeError<S::Error>> {
        self.u64(u64::try_from(count).expect("usize collection length fits u64"))
    }

    fn bounded_identity(&mut self, value: &str) -> Result<(), ProviderFrameEncodeError<S::Error>> {
        let length = u32::try_from(value.len())
            .map_err(|_| ProviderItemValidationError::FrameLengthOverflow)?;
        self.u32(length)?;
        self.bytes(value.as_bytes())
    }

    fn cas_item_id(&mut self, value: &CasItemId) -> Result<(), ProviderFrameEncodeError<S::Error>> {
        self.bounded_identity(value.as_str())
    }

    pub(super) fn cas_thread_id(
        &mut self,
        value: &CasThreadId,
    ) -> Result<(), ProviderFrameEncodeError<S::Error>> {
        self.bounded_identity(value.as_str())
    }

    pub(super) fn asset(
        &mut self,
        asset: AssetId,
    ) -> Result<(), ProviderFrameEncodeError<S::Error>> {
        match asset.version() {
            AssetIdentityVersion::Sha256V1 => self.u8(1)?,
        }
        self.bytes(&asset.digest())?;
        self.u64(asset.length().get())
    }

    pub(super) fn structured(
        &mut self,
        value: &ProviderStructuredValueV1,
    ) -> Result<(), ProviderFrameEncodeError<S::Error>> {
        match value {
            ProviderStructuredValueV1::Null => self.u8(0),
            ProviderStructuredValueV1::Boolean(false) => self.u8(1),
            ProviderStructuredValueV1::Boolean(true) => self.u8(2),
            ProviderStructuredValueV1::Number(ProviderNumberV1::Signed(value)) => {
                self.u8(3)?;
                self.i64(*value)
            }
            ProviderStructuredValueV1::Number(ProviderNumberV1::Unsigned(value)) => {
                self.u8(4)?;
                self.u64(*value)
            }
            ProviderStructuredValueV1::Number(ProviderNumberV1::FiniteFloat(value)) => {
                self.u8(5)?;
                self.u64(value.bits())
            }
            ProviderStructuredValueV1::String(value) => {
                self.u8(6)?;
                self.text(value, None)
            }
            ProviderStructuredValueV1::List(values) => {
                self.u8(7)?;
                self.count(values.len())?;
                for value in values {
                    self.structured(value)?;
                }
                Ok(())
            }
            ProviderStructuredValueV1::Object(entries) => {
                self.u8(8)?;
                self.object_entries(entries)
            }
        }
    }

    pub(super) fn object_entries(
        &mut self,
        entries: &[ProviderObjectEntryV1],
    ) -> Result<(), ProviderFrameEncodeError<S::Error>> {
        self.count(entries.len())?;
        for entry in entries {
            self.raw_text(&entry.key)?;
            self.structured(&entry.value)?;
        }
        Ok(())
    }

    pub(super) fn mcp_content(
        &mut self,
        value: &ProviderMcpContentV1,
    ) -> Result<(), ProviderFrameEncodeError<S::Error>> {
        match value.view() {
            ProviderMcpContentViewV1::Structured(value) => {
                self.u8(0)?;
                self.structured(value)
            }
            ProviderMcpContentViewV1::InlineImage(value) => {
                self.u8(1)?;
                self.asset(value.asset().asset_id())?;
                self.object_entries(value.metadata())
            }
        }
    }
}
