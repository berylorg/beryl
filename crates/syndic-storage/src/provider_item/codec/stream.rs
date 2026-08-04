mod item;
mod structured;
mod text;
mod utf8;

use std::io::Read;

use beryl_model::{CasItemId, CasThreadId};
use sha2::{Digest, Sha256};

use super::{
    ProviderFrameDecodeError, ProviderFrameObservationSummaryV1, ProviderFrameReferenceV1,
    ProviderFrameStreamError, ProviderFrameStructuralValidationV1, ProviderFrameTextSpanSinkV1,
    tags,
};
use crate::provider_item::*;
use item::StreamItemSummary;

pub fn validate_streaming_provider_item_frame_v1<R, S>(
    reader: &mut R,
    encoded_start: u64,
    encoded_bytes: u64,
    expected_digest: [u8; 32],
    spans: &mut S,
) -> Result<ProviderFrameStructuralValidationV1, ProviderFrameStreamError<S::Error>>
where
    R: Read,
    S: ProviderFrameTextSpanSinkV1,
{
    let mut decoder = StreamDecoder::new(reader, spans, encoded_start, encoded_bytes);
    if decoder.fixed::<4>()? != tags::MAGIC {
        return Err(ProviderFrameDecodeError::InvalidTag {
            kind: "magic/version",
            tag: 0,
        }
        .into());
    }
    let ordinal = ProviderFrameOrdinalV1::new(decoder.u64()?)?;
    decoder.frame_ordinal = Some(ordinal);
    let item_id = decoder.cas_item_id()?;
    let (observation, item) = match decoder.u8()? {
        tags::OBSERVATION_STARTED => {
            let timestamp = ProviderLifecycleTimestampMsV1::new(decoder.u64()?);
            let item = decoder.item()?;
            if item.kind.permits_completion_only() {
                return Err(ProviderItemValidationError::CompletionOnlyItemStarted.into());
            }
            (ProviderFrameObservationSummaryV1::Started(timestamp), item)
        }
        tags::OBSERVATION_DELTA => (
            ProviderFrameObservationSummaryV1::Delta,
            StreamItemSummary::supported(decoder.delta()?),
        ),
        tags::OBSERVATION_COMPLETED => {
            let timestamp = ProviderLifecycleTimestampMsV1::new(decoder.u64()?);
            let item = decoder.item()?;
            (
                ProviderFrameObservationSummaryV1::Completed(timestamp),
                item,
            )
        }
        tag => {
            return Err(ProviderFrameDecodeError::InvalidTag {
                kind: "observation",
                tag,
            }
            .into());
        }
    };
    if matches!(observation, ProviderFrameObservationSummaryV1::Completed(_)) && item.in_progress {
        return Err(ProviderItemValidationError::CompletionStatusInProgress.into());
    }
    let (digest, logical_utf8_bytes, text_span_count) = decoder.finish()?;
    if digest != expected_digest {
        return Err(ProviderFrameDecodeError::DigestMismatch.into());
    }
    let encoded_end = encoded_start
        .checked_add(encoded_bytes)
        .ok_or(ProviderItemValidationError::FrameLengthOverflow)?;
    let reference = ProviderFrameReferenceV1::new(
        item_id,
        item.kind,
        ordinal,
        encoded_start,
        encoded_end,
        digest,
        logical_utf8_bytes,
        text_span_count,
    )?;
    Ok(ProviderFrameStructuralValidationV1 {
        reference,
        observation,
        history_support: item.history_support,
        message_phase: item.message_phase,
        submitted_content: item.submitted_content,
    })
}

pub(super) struct StreamDecoder<'a, R: Read, S: ProviderFrameTextSpanSinkV1> {
    reader: &'a mut R,
    spans: &'a mut S,
    remaining: u64,
    encoded_start: u64,
    consumed: u64,
    frame_hasher: Sha256,
    frame_ordinal: Option<ProviderFrameOrdinalV1>,
    logical_frontier: u64,
    text_span_count: u64,
}

impl<R: Read, S: ProviderFrameTextSpanSinkV1> StreamDecoder<'_, R, S> {
    fn new<'a>(
        reader: &'a mut R,
        spans: &'a mut S,
        encoded_start: u64,
        encoded_bytes: u64,
    ) -> StreamDecoder<'a, R, S> {
        StreamDecoder {
            reader,
            spans,
            remaining: encoded_bytes,
            encoded_start,
            consumed: 0,
            frame_hasher: Sha256::new(),
            frame_ordinal: None,
            logical_frontier: 0,
            text_span_count: 0,
        }
    }

    fn finish(self) -> Result<([u8; 32], u64, u64), ProviderFrameStreamError<S::Error>> {
        if self.remaining != 0 {
            return Err(ProviderFrameDecodeError::TrailingBytes.into());
        }
        Ok((
            self.frame_hasher.finalize().into(),
            self.logical_frontier,
            self.text_span_count,
        ))
    }

    fn read_into(&mut self, bytes: &mut [u8]) -> Result<(), ProviderFrameStreamError<S::Error>> {
        let length = u64::try_from(bytes.len()).expect("usize buffer length fits u64");
        if length > self.remaining {
            return Err(ProviderFrameDecodeError::Truncated.into());
        }
        if let Err(error) = self.reader.read_exact(bytes) {
            return if error.kind() == std::io::ErrorKind::UnexpectedEof {
                Err(ProviderFrameDecodeError::Truncated.into())
            } else {
                Err(ProviderFrameStreamError::Read(error))
            };
        }
        self.frame_hasher.update(&*bytes);
        self.remaining -= length;
        self.consumed = self
            .consumed
            .checked_add(length)
            .ok_or(ProviderItemValidationError::FrameLengthOverflow)?;
        Ok(())
    }

    fn fixed<const N: usize>(&mut self) -> Result<[u8; N], ProviderFrameStreamError<S::Error>> {
        let mut bytes = [0; N];
        self.read_into(&mut bytes)?;
        Ok(bytes)
    }

    pub(super) fn u8(&mut self) -> Result<u8, ProviderFrameStreamError<S::Error>> {
        Ok(self.fixed::<1>()?[0])
    }

    pub(super) fn u32(&mut self) -> Result<u32, ProviderFrameStreamError<S::Error>> {
        Ok(u32::from_be_bytes(self.fixed()?))
    }

    pub(super) fn u64(&mut self) -> Result<u64, ProviderFrameStreamError<S::Error>> {
        Ok(u64::from_be_bytes(self.fixed()?))
    }

    pub(super) fn i32(&mut self) -> Result<i32, ProviderFrameStreamError<S::Error>> {
        Ok(i32::from_be_bytes(self.fixed()?))
    }

    pub(super) fn i64(&mut self) -> Result<i64, ProviderFrameStreamError<S::Error>> {
        Ok(i64::from_be_bytes(self.fixed()?))
    }

    pub(super) fn count(
        &mut self,
        kind: &'static str,
    ) -> Result<u64, ProviderFrameStreamError<S::Error>> {
        let count = self.u64()?;
        if count > self.remaining {
            return Err(ProviderFrameDecodeError::InvalidLength { kind }.into());
        }
        Ok(count)
    }

    fn position(&self) -> Result<u64, ProviderItemValidationError> {
        self.encoded_start
            .checked_add(self.consumed)
            .ok_or(ProviderItemValidationError::FrameLengthOverflow)
    }

    fn bounded_identity(
        &mut self,
        kind: &'static str,
    ) -> Result<String, ProviderFrameStreamError<S::Error>> {
        let length = usize::try_from(self.u32()?)
            .map_err(|_| ProviderFrameDecodeError::InvalidLength { kind })?;
        if length > 256 {
            return Err(ProviderFrameDecodeError::InvalidLength { kind }.into());
        }
        let mut bytes = [0_u8; 256];
        self.read_into(&mut bytes[..length])?;
        std::str::from_utf8(&bytes[..length])
            .map(str::to_owned)
            .map_err(|_| ProviderFrameDecodeError::InvalidUtf8 { kind }.into())
    }

    fn cas_item_id(&mut self) -> Result<CasItemId, ProviderFrameStreamError<S::Error>> {
        CasItemId::new(self.bounded_identity("CAS item id")?)
            .map_err(|_| ProviderFrameDecodeError::InvalidIdentity {
                kind: "CAS item id",
            })
            .map_err(Into::into)
    }

    pub(super) fn cas_thread_id(&mut self) -> Result<(), ProviderFrameStreamError<S::Error>> {
        CasThreadId::new(self.bounded_identity("CAS thread id")?)
            .map(|_| ())
            .map_err(|_| ProviderFrameDecodeError::InvalidIdentity {
                kind: "CAS thread id",
            })
            .map_err(Into::into)
    }

    pub(super) fn option(
        &mut self,
        kind: &'static str,
        parse: impl FnOnce(&mut Self) -> Result<(), ProviderFrameStreamError<S::Error>>,
    ) -> Result<(), ProviderFrameStreamError<S::Error>> {
        self.option_value(kind, parse).map(|_| ())
    }

    pub(super) fn option_value<T>(
        &mut self,
        kind: &'static str,
        parse: impl FnOnce(&mut Self) -> Result<T, ProviderFrameStreamError<S::Error>>,
    ) -> Result<Option<T>, ProviderFrameStreamError<S::Error>> {
        match self.u8()? {
            tags::OPTION_NONE => Ok(None),
            tags::OPTION_SOME => parse(self).map(Some),
            tag => Err(ProviderFrameDecodeError::InvalidTag { kind, tag }.into()),
        }
    }

    pub(super) fn boolean(
        &mut self,
        kind: &'static str,
    ) -> Result<(), ProviderFrameStreamError<S::Error>> {
        match self.u8()? {
            0 | 1 => Ok(()),
            tag => Err(ProviderFrameDecodeError::InvalidTag { kind, tag }.into()),
        }
    }

    pub(super) fn enum_tag(
        &mut self,
        kind: &'static str,
        variants: u8,
    ) -> Result<u8, ProviderFrameStreamError<S::Error>> {
        let tag = self.u8()?;
        if tag < variants {
            Ok(tag)
        } else {
            Err(ProviderFrameDecodeError::InvalidTag { kind, tag }.into())
        }
    }
}
