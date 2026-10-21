use super::source::invalid;
use crate::read::access::ReadAccess;
use crate::{
    SealedProviderFrameReference, SyndicReadError, SyndicStorage, codec::*, domain::SyndicDomain,
};
use beryl_home_store::{CursorDirection, CursorRange, CursorReadLimits};
use beryl_model::{GeneratedMediaUnavailableReason as Reason, SyndicContentId};
use sha2::{Digest, Sha256};

struct Extractor<'a> {
    storage: &'a SyndicStorage,
    store: ReadAccess<'a>,
    content: SyndicContentId,
    position: u64,
    end: u64,
    prior: u64,
}

pub(super) fn saved_path(
    storage: &SyndicStorage,
    store: ReadAccess<'_>,
    frame: &SealedProviderFrameReference,
) -> Result<Result<Box<str>, Reason>, SyndicReadError> {
    let mut parser = Extractor {
        storage,
        store,
        content: frame.content().id(),
        position: frame.frame().encoded_start(),
        end: frame.frame().encoded_end(),
        prior: frame.frame().encoded_start(),
    };
    if parser.take(4)? != b"PIV1" || parser.u64()? != frame.frame().ordinal().get() {
        return Err(invalid());
    }
    let id_length = u32::from_be_bytes(parser.take(4)?.try_into().map_err(|_| invalid())?) as usize;
    if id_length != frame.frame().item_id().as_str().len()
        || parser.take(id_length)? != frame.frame().item_id().as_str().as_bytes()
    {
        return Err(invalid());
    }
    if parser.byte()? != 2 {
        return Err(invalid());
    }
    let observed_at = parser.u64()?;
    if !matches!(frame.observation(), crate::ProviderFrameObservationSummaryV1::Completed(time) if time.get() == observed_at)
        || parser.byte()? != 14
    {
        return Err(invalid());
    }
    let status = parser.byte()?;
    if status > 2 {
        return Err(invalid());
    }
    match parser.byte()? {
        0 => {}
        1 => {
            parser.text(false)?;
        }
        _ => return Err(invalid()),
    }
    let path = match parser.byte()? {
        0 => Err(Reason::Missing),
        1 => parser.text(true)?.ok_or(Reason::MalformedOutput),
        _ => return Err(invalid()),
    };
    if parser.position != parser.end {
        return Err(invalid());
    }
    if status != 2 {
        return Ok(Err(Reason::ProducerFailed));
    }
    Ok(match path {
        Ok(bytes) => match String::from_utf8(bytes) {
            Ok(path) if !path.is_empty() && !path.contains('\0') => Ok(path.into_boxed_str()),
            Ok(path) if path.is_empty() => Err(Reason::Missing),
            _ => Err(Reason::MalformedOutput),
        },
        Err(reason) => Err(reason),
    })
}

impl Extractor<'_> {
    fn byte(&mut self) -> Result<u8, SyndicReadError> {
        Ok(self.take(1)?[0])
    }
    fn u64(&mut self) -> Result<u64, SyndicReadError> {
        Ok(u64::from_be_bytes(
            self.take(8)?.try_into().map_err(|_| invalid())?,
        ))
    }
    fn take(&mut self, length: usize) -> Result<Vec<u8>, SyndicReadError> {
        let end = self
            .position
            .checked_add(length as u64)
            .filter(|end| *end <= self.end)
            .ok_or_else(invalid)?;
        let bytes = self.range(self.position, end)?;
        self.position = end;
        Ok(bytes)
    }
    fn text(&mut self, retain: bool) -> Result<Option<Vec<u8>>, SyndicReadError> {
        match self.byte()? {
            0 => {
                let length = self.u64()?;
                let end = self
                    .position
                    .checked_add(length)
                    .filter(|end| *end <= self.end)
                    .ok_or_else(invalid)?;
                let bytes = if retain && length <= 4096 {
                    Some(self.range(self.position, end)?)
                } else {
                    None
                };
                self.position = end;
                Ok(bytes)
            }
            1 => {
                let start = self.u64()?;
                let end = self.u64()?;
                let digest = self.take(32)?;
                if start >= end || end > self.prior {
                    return Err(invalid());
                }
                if !retain || end - start > 4096 {
                    return Ok(None);
                }
                let bytes = self.range(start, end)?;
                if Sha256::digest(&bytes).as_slice() != digest {
                    return Err(invalid());
                }
                Ok(Some(bytes))
            }
            _ => Err(invalid()),
        }
    }
    fn range(&self, mut start: u64, end: u64) -> Result<Vec<u8>, SyndicReadError> {
        if start > end || end - start > 4096 {
            return Err(invalid());
        }
        let mut output = Vec::with_capacity((end - start) as usize);
        while start < end {
            let page = self
                .store
                .read_cursor::<SyndicDomain, ContentByteSpansCodec>(
                    &self.storage.handle,
                    &CursorRange::closed(
                        ContentByteSpanKey {
                            owner: self.content,
                            start: 0,
                        },
                        ContentByteSpanKey {
                            owner: self.content,
                            start,
                        },
                    ),
                    CursorDirection::Reverse,
                    CursorReadLimits::new(1, 512).expect("byte-span predecessor bound"),
                )?;
            let record = page.records().first().ok_or_else(invalid)?;
            let span = record.value();
            if record.key().owner != self.content
                || record.key().start != span.start()
                || span.content_id() != self.content
                || span.start() > start
                || span.end() <= start
            {
                return Err(invalid());
            }
            let chunk = self
                .storage
                .generated_point_with_access::<ContentChunksFamily>(
                    self.store,
                    ContentChunkKey {
                        owner: self.content,
                        ordinal: span.ordinal(),
                    },
                )?
                .ok_or_else(invalid)?;
            let selected_end = end.min(span.end());
            let first = usize::try_from(start - span.start()).map_err(|_| invalid())?;
            let last = usize::try_from(selected_end - span.start()).map_err(|_| invalid())?;
            if chunk.content_id() != self.content
                || chunk.ordinal() != span.ordinal()
                || chunk.bytes().len() as u64 != span.end() - span.start()
            {
                return Err(invalid());
            }
            output.extend_from_slice(chunk.bytes().get(first..last).ok_or_else(invalid)?);
            start = selected_end;
        }
        Ok(output)
    }
}
