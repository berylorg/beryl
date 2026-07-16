use std::io::Read;

use sha2::{Digest, Sha256};

use super::{StreamDecoder, utf8::Utf8State};
use crate::provider_item::*;

type Utf8ScanResult<const N: usize> = ([u8; 32], [bool; N], bool);

impl<R: Read, S: ProviderFrameTextSpanSinkV1> StreamDecoder<'_, R, S> {
    pub(super) fn scan_utf8<const N: usize>(
        &mut self,
        length: u64,
        kind: &'static str,
        exact: [&[u8]; N],
        ascii_prefix: Option<&[u8]>,
    ) -> Result<Utf8ScanResult<N>, ProviderFrameStreamError<S::Error>> {
        if length > self.remaining {
            return Err(ProviderFrameDecodeError::Truncated.into());
        }
        let mut utf8 = Utf8State::default();
        let mut digest = Sha256::new();
        let mut matches = std::array::from_fn(|index| {
            u64::try_from(exact[index].len()).expect("literal length fits u64") == length
        });
        let mut prefix_matches = ascii_prefix.is_some_and(|prefix| {
            u64::try_from(prefix.len()).expect("literal length fits u64") <= length
        });
        let mut offset = 0_u64;
        let mut left = length;
        let mut buffer = [0_u8; 4_096];
        while left != 0 {
            let take = usize::try_from(left.min(buffer.len() as u64))
                .expect("bounded scan length fits usize");
            let bytes = &mut buffer[..take];
            self.read_into(bytes)?;
            utf8.push(bytes, kind)?;
            digest.update(&*bytes);
            for (local, byte) in bytes.iter().copied().enumerate() {
                let position = usize::try_from(offset)
                    .ok()
                    .and_then(|base| base.checked_add(local));
                for (index, candidate) in exact.iter().enumerate() {
                    if matches[index]
                        && position
                            .and_then(|position| candidate.get(position))
                            .copied()
                            != Some(byte)
                    {
                        matches[index] = false;
                    }
                }
                if prefix_matches
                    && let (Some(prefix), Some(position)) = (ascii_prefix, position)
                    && let Some(expected) = prefix.get(position)
                    && !byte.eq_ignore_ascii_case(expected)
                {
                    prefix_matches = false;
                }
            }
            let take_u64 = u64::try_from(take).expect("bounded scan length fits u64");
            offset += take_u64;
            left -= take_u64;
        }
        utf8.finish(kind)?;
        Ok((digest.finalize().into(), matches, prefix_matches))
    }

    pub(super) fn raw_text_matches<const N: usize>(
        &mut self,
        kind: &'static str,
        exact: [&[u8]; N],
    ) -> Result<[bool; N], ProviderFrameStreamError<S::Error>> {
        let length = self.u64()?;
        self.scan_utf8(length, kind, exact, None)
            .map(|(_, matches, _)| matches)
    }

    pub(super) fn raw_text_validate_image_locator(
        &mut self,
        kind: &'static str,
    ) -> Result<(), ProviderFrameStreamError<S::Error>> {
        let length = self.u64()?;
        if length > self.remaining {
            return Err(ProviderFrameDecodeError::Truncated.into());
        }
        let mut utf8 = Utf8State::default();
        let mut locator = ProviderImageLocatorValidatorV1::new();
        let mut left = length;
        let mut buffer = [0_u8; 4_096];
        while left != 0 {
            let take = usize::try_from(left.min(buffer.len() as u64))
                .expect("bounded scan length fits usize");
            let bytes = &mut buffer[..take];
            self.read_into(bytes)?;
            utf8.push(bytes, kind)?;
            locator.push(bytes);
            left -= u64::try_from(take).expect("bounded scan length fits u64");
        }
        utf8.finish(kind)?;
        locator.finish().map_err(Into::into)
    }

    pub(super) fn text(
        &mut self,
        kind: &'static str,
        role: Option<ProviderLogicalTextRoleV1>,
    ) -> Result<(), ProviderFrameStreamError<S::Error>> {
        let (source_start, source_end, digest) = match self.u8()? {
            super::super::tags::TEXT_INLINE => {
                let length = self.u64()?;
                let start = self.position()?;
                let (digest, _, _) = self.scan_utf8(length, kind, [], None)?;
                let end = self.position()?;
                (start, end, digest)
            }
            super::super::tags::TEXT_REUSED => {
                let start = self.u64()?;
                let end = self.u64()?;
                let digest = self.fixed::<32>()?;
                let reference = ProviderTextReferenceV1::new(start, end, digest)?;
                if reference.end() > self.encoded_start {
                    return Err(ProviderItemValidationError::TextReferenceBeyondFrontier {
                        start,
                        end,
                        frontier: self.encoded_start,
                    }
                    .into());
                }
                (start, end, digest)
            }
            tag => return Err(ProviderFrameDecodeError::InvalidTag { kind, tag }.into()),
        };
        self.write_logical_span(role, source_start, source_end, digest)
    }

    fn write_logical_span(
        &mut self,
        role: Option<ProviderLogicalTextRoleV1>,
        source_start: u64,
        source_end: u64,
        digest: [u8; 32],
    ) -> Result<(), ProviderFrameStreamError<S::Error>> {
        let Some(role) = role else { return Ok(()) };
        let length = source_end - source_start;
        if length == 0 {
            return Ok(());
        }
        let logical_end = self
            .logical_frontier
            .checked_add(length)
            .ok_or(ProviderItemValidationError::FrameLengthOverflow)?;
        let span = ProviderFrameTextSpanV1::new(
            self.frame_ordinal
                .expect("frame ordinal parsed before body"),
            self.logical_frontier,
            logical_end,
            source_start,
            source_end,
            digest,
            role,
        )?;
        self.spans
            .write_text_span(span)
            .map_err(ProviderFrameStreamError::Span)?;
        self.logical_frontier = logical_end;
        self.text_span_count = self
            .text_span_count
            .checked_add(1)
            .ok_or(ProviderItemValidationError::FrameLengthOverflow)?;
        Ok(())
    }

    pub(super) fn asset(&mut self) -> Result<(), ProviderFrameStreamError<S::Error>> {
        let tag = self.u8()?;
        if tag != 1 {
            return Err(ProviderFrameDecodeError::InvalidTag {
                kind: "asset identity version",
                tag,
            }
            .into());
        }
        self.fixed::<32>()?;
        if self.u64()? == 0 {
            return Err(ProviderFrameDecodeError::InvalidIdentity { kind: "asset id" }.into());
        }
        Ok(())
    }
}
