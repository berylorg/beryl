use super::super::STREAMED_TEXT_MAX_PAGE_BYTES;
use super::{
    contract::{
        StreamedInputDescriptor, StreamedInputDescriptorKind, StreamedInputSource,
        StreamedInputSourceError, StreamedTextDescriptor, StreamedTextPage,
    },
    digest::{StreamedInputHeader, StreamedInputSequenceDigestAccumulator},
};

pub(crate) struct StreamedInputPass {
    expected: StreamedInputHeader,
    accumulator: Option<StreamedInputSequenceDigestAccumulator>,
    observed: u64,
    finished: bool,
}

impl StreamedInputPass {
    pub(crate) fn begin(
        expected: StreamedInputHeader,
        source: &mut dyn StreamedInputSource,
    ) -> Result<Self, StreamedInputSourceError> {
        let actual = source.begin_pass()?;
        if actual.source_identity() != expected.source_identity() {
            return Err(StreamedInputSourceError::SourceIdentityMismatch {
                expected: expected.source_identity(),
                actual: actual.source_identity(),
            });
        }
        if actual.source_revision() != expected.source_revision() {
            return Err(StreamedInputSourceError::RevisionDrift {
                expected: expected.source_revision(),
                actual: actual.source_revision(),
            });
        }
        if actual.item_count() != expected.item_count() {
            return Err(StreamedInputSourceError::DeclaredItemCountMismatch {
                expected: expected.item_count(),
                actual: actual.item_count(),
            });
        }
        if actual.sequence_digest() != expected.sequence_digest() {
            return Err(StreamedInputSourceError::DeclaredSequenceDigestMismatch {
                expected: expected.sequence_digest(),
                actual: actual.sequence_digest(),
            });
        }
        Ok(Self {
            expected,
            accumulator: Some(StreamedInputSequenceDigestAccumulator::new(
                expected.item_count(),
            )),
            observed: 0,
            finished: false,
        })
    }

    pub(crate) const fn observed_count(&self) -> u64 {
        self.observed
    }

    pub(crate) fn next_descriptor(
        &mut self,
        source: &mut dyn StreamedInputSource,
    ) -> Result<Option<StreamedInputDescriptorKind>, StreamedInputSourceError> {
        if self.finished {
            return Ok(None);
        }
        if self.observed == self.expected.item_count() {
            return match source.next_descriptor()? {
                Some(descriptor) => Err(StreamedInputSourceError::UnexpectedDescriptor {
                    declared: self.expected.item_count(),
                    actual_ordinal: descriptor.item_ordinal(),
                }),
                None => {
                    self.finish_digest()?;
                    self.finished = true;
                    Ok(None)
                }
            };
        }

        let Some(descriptor) = source.next_descriptor()? else {
            return Err(StreamedInputSourceError::DescriptorCountMismatch {
                expected: self.expected.item_count(),
                actual: self.observed,
            });
        };
        self.validate_descriptor_header(&descriptor)?;
        let ordinal = descriptor.item_ordinal();
        match descriptor.kind() {
            StreamedInputDescriptorKind::Text(text) => {
                if text.utf8_len() == 0 {
                    return Err(StreamedInputSourceError::MalformedTextSegmentation {
                        item_ordinal: ordinal,
                    });
                }
                self.accumulator
                    .as_mut()
                    .expect("unfinished pass owns its digest")
                    .push_text(ordinal, text.proof(), text.utf8_len())?;
            }
            StreamedInputDescriptorKind::LocalImage(image) => {
                self.accumulator
                    .as_mut()
                    .expect("unfinished pass owns its digest")
                    .push_local_image(ordinal, image.detail(), image.path())?;
            }
        }
        self.observed += 1;
        Ok(Some(descriptor.into_kind()))
    }

    pub(crate) fn finish(
        &mut self,
        source: &mut dyn StreamedInputSource,
    ) -> Result<(), StreamedInputSourceError> {
        if self.observed != self.expected.item_count() {
            return Err(StreamedInputSourceError::DescriptorCountMismatch {
                expected: self.expected.item_count(),
                actual: self.observed,
            });
        }
        match self.next_descriptor(source)? {
            None => Ok(()),
            Some(_) => unreachable!("terminal next_descriptor rejects trailing input"),
        }
    }

    pub(crate) fn read_text_page(
        &mut self,
        source: &mut dyn StreamedInputSource,
        item_ordinal: u64,
        descriptor: &StreamedTextDescriptor,
        start: u64,
    ) -> Result<StreamedTextPage, StreamedInputSourceError> {
        if STREAMED_TEXT_MAX_PAGE_BYTES == 0 {
            unreachable!("streamed text page bound is nonzero");
        }
        let page =
            source.read_text_page(descriptor.source_id(), start, STREAMED_TEXT_MAX_PAGE_BYTES)?;
        self.validate_text_page(item_ordinal, descriptor, start, &page)?;
        Ok(page)
    }

    fn validate_descriptor_header(
        &self,
        descriptor: &StreamedInputDescriptor,
    ) -> Result<(), StreamedInputSourceError> {
        if descriptor.source_identity() != self.expected.source_identity() {
            return Err(StreamedInputSourceError::SourceIdentityMismatch {
                expected: self.expected.source_identity(),
                actual: descriptor.source_identity(),
            });
        }
        if descriptor.source_revision() != self.expected.source_revision() {
            return Err(StreamedInputSourceError::RevisionDrift {
                expected: self.expected.source_revision(),
                actual: descriptor.source_revision(),
            });
        }
        let expected = self.observed + 1;
        if descriptor.item_ordinal() != expected {
            return Err(StreamedInputSourceError::DescriptorOrdinalMismatch {
                expected,
                actual: descriptor.item_ordinal(),
            });
        }
        Ok(())
    }

    fn validate_text_page(
        &self,
        item_ordinal: u64,
        descriptor: &StreamedTextDescriptor,
        requested_start: u64,
        page: &StreamedTextPage,
    ) -> Result<(), StreamedInputSourceError> {
        if page.source_identity() != self.expected.source_identity() {
            return Err(StreamedInputSourceError::SourceIdentityMismatch {
                expected: self.expected.source_identity(),
                actual: page.source_identity(),
            });
        }
        if page.source_revision() != self.expected.source_revision() {
            return Err(StreamedInputSourceError::RevisionDrift {
                expected: self.expected.source_revision(),
                actual: page.source_revision(),
            });
        }
        if page.source_id() != descriptor.source_id() {
            return Err(StreamedInputSourceError::TextSourceIdMismatch { item_ordinal });
        }
        if page.proof() != descriptor.proof() {
            return Err(StreamedInputSourceError::TextProofMismatch { item_ordinal });
        }
        if page.start() != requested_start {
            return Err(StreamedInputSourceError::PageStartMismatch {
                expected: requested_start,
                actual: page.start(),
            });
        }
        let page_bytes = page.text().len();
        if page_bytes > STREAMED_TEXT_MAX_PAGE_BYTES {
            return Err(StreamedInputSourceError::PageTooLarge {
                maximum: STREAMED_TEXT_MAX_PAGE_BYTES,
                actual: page_bytes,
            });
        }
        if page_bytes == 0 {
            return Err(StreamedInputSourceError::EmptyPage {
                start: requested_start,
            });
        }
        let page_bytes =
            u64::try_from(page_bytes).map_err(|_| StreamedInputSourceError::PageEndOverflow {
                start: requested_start,
                page_bytes: page.text().len(),
            })?;
        let end = requested_start.checked_add(page_bytes).ok_or(
            StreamedInputSourceError::PageEndOverflow {
                start: requested_start,
                page_bytes: page.text().len(),
            },
        )?;
        if end > descriptor.utf8_len() {
            return Err(StreamedInputSourceError::PagePastEnd {
                end,
                utf8_len: descriptor.utf8_len(),
            });
        }
        match page.next_offset() {
            None if end != descriptor.utf8_len() => Err(StreamedInputSourceError::PrematureEof {
                end,
                utf8_len: descriptor.utf8_len(),
            }),
            Some(next_offset) if next_offset != end || end == descriptor.utf8_len() => {
                Err(StreamedInputSourceError::InvalidNextOffset {
                    end,
                    next_offset,
                    utf8_len: descriptor.utf8_len(),
                })
            }
            _ => Ok(()),
        }
    }

    fn finish_digest(&mut self) -> Result<(), StreamedInputSourceError> {
        let actual = self
            .accumulator
            .take()
            .expect("unfinished pass owns its digest")
            .finish()?;
        if actual != self.expected.sequence_digest() {
            return Err(StreamedInputSourceError::SequenceDigestMismatch {
                expected: self.expected.sequence_digest(),
                actual,
            });
        }
        Ok(())
    }
}
