use std::io::{self, Read};

use beryl_home_store::{DomainReader, PointReadLimit, ReadError};
use beryl_model::{SyndicContentDigest, SyndicContentId};

use crate::{
    ContentByteSpanRecord, ContentChunkOrdinal, ContentChunkRecord, ContentSummary,
    ProviderFrameStructuralValidationV1, ProviderFrameTextSpanSinkV1, ProviderFrameTextSpanV1,
    ProviderItemBuildLifecycle, ProviderItemBuildRecord, ProviderLogicalTextRoleV1,
    ProviderNarrativeReference, ProviderNarrativeSpanRecord, SealedProviderFrameReference,
    advance_content_chain, codec::*, content_chain_seed, domain::SyndicDomain,
    validate_streaming_provider_item_frame_v1,
};

mod completion;
mod prefix;
mod published;
mod source;

pub(crate) use completion::{
    advance_provider_completion_comparison, validate_provider_completion_comparison,
};
pub(crate) use prefix::validate_staged_provider_narrative;
pub(crate) use published::{
    validate_published_narrative_completion, validate_published_provider_frame,
};
use source::validate_source_range;

const CHUNK_POINT_BYTES: usize = 128 * 1024;
const INDEX_POINT_BYTES: usize = 1024;

#[derive(Debug)]
pub(crate) enum ProviderFrameStorageValidationError {
    Read(ReadError),
    Invariant(&'static str),
}

pub(crate) fn validate_staged_provider_frame(
    reader: &DomainReader<'_, SyndicDomain>,
    build: &ProviderItemBuildRecord,
) -> Result<ProviderFrameStructuralValidationV1, ProviderFrameStorageValidationError> {
    if build.lifecycle() != ProviderItemBuildLifecycle::Sealed && !build.frame_staged() {
        return Err(ProviderFrameStorageValidationError::Invariant(
            "provider frame structural validation requires a fully staged frame",
        ));
    }
    let mut spans = FrameSpanVerifier::new(reader, build)?;
    let validation =
        validate_sealed_provider_frame(reader, build.prior(), build.target(), &mut spans)?;
    spans.finish()?;
    Ok(validation)
}

fn validate_sealed_provider_frame<S>(
    reader: &DomainReader<'_, SyndicDomain>,
    prior: Option<&SealedProviderFrameReference>,
    target: &SealedProviderFrameReference,
    spans: &mut S,
) -> Result<ProviderFrameStructuralValidationV1, ProviderFrameStorageValidationError>
where
    S: ProviderFrameTextSpanSinkV1<Error = FrameSpanError>,
{
    let frame = target.frame();
    let first_chunk = prior
        .map(|value| value.content().summary().chunk_count())
        .unwrap_or(0)
        .checked_add(1)
        .ok_or(ProviderFrameStorageValidationError::Invariant(
            "provider frame first chunk overflowed",
        ))?;
    let initial_chain = prior
        .map(|value| value.content().summary().digest())
        .unwrap_or_else(|| content_chain_seed(crate::ContentEncoding::ProviderItemV1));
    let mut bytes = FrameChunkReader::new(
        reader,
        target.content().id(),
        first_chunk,
        target.content().summary().chunk_count(),
        frame.encoded_start(),
        initial_chain,
    );
    let result = validate_streaming_provider_item_frame_v1(
        &mut bytes,
        frame.encoded_start(),
        frame.encoded_len(),
        frame.encoded_digest(),
        spans,
    );
    if let Some(error) = bytes.take_read_error() {
        return Err(ProviderFrameStorageValidationError::Read(error));
    }
    let validation = result.map_err(|error| match error {
        crate::ProviderFrameStreamError::Span(FrameSpanError::Read(source)) => {
            ProviderFrameStorageValidationError::Read(source)
        }
        crate::ProviderFrameStreamError::Span(FrameSpanError::Invariant(message)) => {
            ProviderFrameStorageValidationError::Invariant(message)
        }
        crate::ProviderFrameStreamError::Decode(_) | crate::ProviderFrameStreamError::Read(_) => {
            ProviderFrameStorageValidationError::Invariant(
                "staged provider frame failed structural validation",
            )
        }
    })?;
    bytes.finish(target.content().summary())?;
    if validation.reference() != frame || validation.observation() != target.observation() {
        return Err(ProviderFrameStorageValidationError::Invariant(
            "staged provider frame reference or observation disagrees",
        ));
    }
    Ok(validation)
}

struct FrameChunkReader<'a, 'r> {
    reader: &'a DomainReader<'r, SyndicDomain>,
    content_id: SyndicContentId,
    next_ordinal: u64,
    final_ordinal: u64,
    expected_start: u64,
    chain: SyndicContentDigest,
    current: Option<ContentChunkRecord>,
    current_offset: usize,
    read_error: Option<ReadError>,
    invalid: bool,
}

impl<'a, 'r> FrameChunkReader<'a, 'r> {
    const fn new(
        reader: &'a DomainReader<'r, SyndicDomain>,
        content_id: SyndicContentId,
        first_ordinal: u64,
        final_ordinal: u64,
        expected_start: u64,
        chain: SyndicContentDigest,
    ) -> Self {
        Self {
            reader,
            content_id,
            next_ordinal: first_ordinal,
            final_ordinal,
            expected_start,
            chain,
            current: None,
            current_offset: 0,
            read_error: None,
            invalid: false,
        }
    }

    fn load_next(&mut self) -> io::Result<bool> {
        if self.next_ordinal > self.final_ordinal {
            return Ok(false);
        }
        let ordinal = ContentChunkOrdinal::new(self.next_ordinal)
            .map_err(|_| invalid_io("provider frame chunk ordinal is invalid"))?;
        let key = ContentChunkKey {
            owner: self.content_id,
            ordinal,
        };
        let chunk = match self.reader.point::<ContentChunksCodec>(
            &key,
            PointReadLimit::new(CHUNK_POINT_BYTES).expect("chunk point bound is nonzero"),
        ) {
            Ok(Some(chunk)) => chunk,
            Ok(None) => return self.fail("provider frame chunk is missing"),
            Err(error) => return self.read_fail(error),
        };
        let span_key = ContentByteSpanKey {
            owner: self.content_id,
            start: self.expected_start,
        };
        let span = match self.reader.point::<ContentByteSpansCodec>(
            &span_key,
            PointReadLimit::new(INDEX_POINT_BYTES).expect("span point bound is nonzero"),
        ) {
            Ok(Some(span)) => span,
            Ok(None) => return self.fail("provider frame content byte span is missing"),
            Err(error) => return self.read_fail(error),
        };
        if !chunk_and_span_agree(&chunk, span, self.expected_start) {
            return self.fail("provider frame chunk and byte span disagree");
        }
        self.chain = advance_content_chain(self.chain, &chunk);
        self.expected_start = span.end();
        self.next_ordinal = self
            .next_ordinal
            .checked_add(1)
            .ok_or_else(|| invalid_io("provider frame chunk frontier overflowed"))?;
        self.current = Some(chunk);
        self.current_offset = 0;
        Ok(true)
    }

    fn fail<T>(&mut self, message: &'static str) -> io::Result<T> {
        self.invalid = true;
        Err(invalid_io(message))
    }

    fn read_fail<T>(&mut self, error: ReadError) -> io::Result<T> {
        self.read_error = Some(error);
        Err(invalid_io("provider frame bounded read failed"))
    }

    fn take_read_error(&mut self) -> Option<ReadError> {
        self.read_error.take()
    }

    fn finish(&self, summary: ContentSummary) -> Result<(), ProviderFrameStorageValidationError> {
        if self.invalid
            || self.current.is_some()
            || self.next_ordinal != self.final_ordinal.saturating_add(1)
            || self.expected_start != summary.encoded_bytes()
            || self.chain != summary.digest()
        {
            return Err(ProviderFrameStorageValidationError::Invariant(
                "provider frame staged chunk frontier disagrees",
            ));
        }
        Ok(())
    }
}

impl Read for FrameChunkReader<'_, '_> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if output.is_empty() {
            return Ok(0);
        }
        let mut written = 0;
        while written < output.len() {
            if self.current.is_none() && !self.load_next()? {
                break;
            }
            let chunk = self
                .current
                .as_ref()
                .expect("load_next installed a provider frame chunk");
            let remaining = &chunk.bytes()[self.current_offset..];
            let take = remaining.len().min(output.len() - written);
            output[written..written + take].copy_from_slice(&remaining[..take]);
            written += take;
            self.current_offset += take;
            if self.current_offset == chunk.bytes().len() {
                self.current = None;
                self.current_offset = 0;
            }
        }
        Ok(written)
    }
}

fn chunk_and_span_agree(
    chunk: &ContentChunkRecord,
    span: ContentByteSpanRecord,
    expected_start: u64,
) -> bool {
    chunk.content_id() == span.content_id()
        && chunk.ordinal() == span.ordinal()
        && span.start() == expected_start
        && span.len() == u64::try_from(chunk.bytes().len()).unwrap_or(u64::MAX)
        && *chunk.digest() == span.chunk_digest()
}

#[derive(Debug)]
pub(super) enum FrameSpanError {
    Read(ReadError),
    Invariant(&'static str),
}

struct FrameSpanVerifier<'a, 'r, 'b> {
    reader: &'a DomainReader<'r, SyndicDomain>,
    build: &'b ProviderItemBuildRecord,
    logical_base: u64,
    narrative: Option<ProviderNarrativeReference>,
    completion_span: Option<ProviderFrameTextSpanV1>,
}

impl<'a, 'r, 'b> FrameSpanVerifier<'a, 'r, 'b> {
    fn new(
        reader: &'a DomainReader<'r, SyndicDomain>,
        build: &'b ProviderItemBuildRecord,
    ) -> Result<Self, ProviderFrameStorageValidationError> {
        let narrative = narrative_seed(build)?;
        Ok(Self {
            reader,
            build,
            logical_base: narrative.map_or(0, |value| value.logical_utf8_bytes()),
            narrative,
            completion_span: None,
        })
    }

    fn finish(&self) -> Result<(), ProviderFrameStorageValidationError> {
        if self.narrative != self.build.target().narrative()
            || self.completion_span
                != self
                    .build
                    .completion_check()
                    .and_then(|check| check.source())
        {
            return Err(ProviderFrameStorageValidationError::Invariant(
                "provider frame staged narrative frontier disagrees",
            ));
        }
        Ok(())
    }
}

impl ProviderFrameTextSpanSinkV1 for FrameSpanVerifier<'_, '_, '_> {
    type Error = FrameSpanError;

    fn write_text_span(&mut self, span: ProviderFrameTextSpanV1) -> Result<(), Self::Error> {
        validate_source_range(
            self.reader,
            self.build.target().content(),
            self.build.staged_encoded_bytes(),
            span.source_start(),
            span.source_end(),
            span.source_digest(),
        )?;
        if let Some(check) = self.build.completion_check() {
            if check.source() != Some(span) || self.completion_span.replace(span).is_some() {
                return Err(FrameSpanError::Invariant(
                    "provider completion narrative source disagrees",
                ));
            }
            return Ok(());
        }
        let Some(previous) = self.narrative else {
            return Ok(());
        };
        if span.role() != ProviderLogicalTextRoleV1::Narrative {
            return Err(FrameSpanError::Invariant(
                "narrative provider frame emitted a nonnarrative span",
            ));
        }
        let logical_start = self.logical_base.checked_add(span.logical_start()).ok_or(
            FrameSpanError::Invariant("provider narrative logical frontier overflowed"),
        )?;
        let logical_end =
            self.logical_base
                .checked_add(span.logical_end())
                .ok_or(FrameSpanError::Invariant(
                    "provider narrative logical frontier overflowed",
                ))?;
        if logical_start != previous.logical_utf8_bytes() {
            return Err(FrameSpanError::Invariant(
                "provider narrative logical frontier is not contiguous",
            ));
        }
        let frame = self.build.target().frame();
        let expected = ProviderNarrativeSpanRecord::new(
            self.build.target().content().id(),
            previous.generation(),
            logical_start,
            logical_end,
            span.frame_ordinal(),
            frame.encoded_digest(),
            span.source_start(),
            span.source_end(),
            span.source_digest(),
            previous.chain_digest(),
        )
        .map_err(|_| FrameSpanError::Invariant("provider narrative span is invalid"))?;
        let key = ProviderNarrativeSpanKey::new(
            expected.content_id(),
            expected.generation(),
            expected.logical_start(),
        );
        let stored = self
            .reader
            .point::<ProviderNarrativeSpansCodec>(
                &key,
                PointReadLimit::new(INDEX_POINT_BYTES).expect("span point bound is nonzero"),
            )
            .map_err(FrameSpanError::Read)?
            .ok_or(FrameSpanError::Invariant(
                "provider narrative span is missing",
            ))?;
        ProviderNarrativeSpansFamily::validate_key_value(&key, &stored).map_err(|_| {
            FrameSpanError::Invariant("provider narrative-span key and value disagree")
        })?;
        if stored != expected {
            return Err(FrameSpanError::Invariant(
                "provider frame regenerated narrative span disagrees",
            ));
        }
        self.narrative = Some(advance_narrative(previous, stored)?);
        Ok(())
    }
}

fn narrative_seed(
    build: &ProviderItemBuildRecord,
) -> Result<Option<ProviderNarrativeReference>, ProviderFrameStorageValidationError> {
    let Some(target) = build.target().narrative() else {
        return Ok(None);
    };
    if matches!(
        build.target().observation(),
        crate::ProviderFrameObservationSummaryV1::Delta
            | crate::ProviderFrameObservationSummaryV1::Completed(_)
    ) {
        return build
            .prior()
            .and_then(crate::SealedProviderFrameReference::narrative)
            .map(Some)
            .ok_or(ProviderFrameStorageValidationError::Invariant(
                "provider delta omitted its prior narrative frontier",
            ));
    }
    Ok(Some(ProviderNarrativeReference::empty(
        target.content_id(),
        target.generation(),
    )))
}

fn advance_narrative(
    previous: ProviderNarrativeReference,
    record: ProviderNarrativeSpanRecord,
) -> Result<ProviderNarrativeReference, FrameSpanError> {
    let count = previous
        .span_count()
        .checked_add(1)
        .ok_or(FrameSpanError::Invariant(
            "provider narrative span frontier overflowed",
        ))?;
    ProviderNarrativeReference::new(
        previous.content_id(),
        previous.generation(),
        count,
        record.logical_end(),
        record.resulting_chain_digest(),
    )
    .map_err(|_| FrameSpanError::Invariant("provider narrative frontier is invalid"))
}

fn invalid_io(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
