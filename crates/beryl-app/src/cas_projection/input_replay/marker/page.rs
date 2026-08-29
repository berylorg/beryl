//! One bounded marker-aware text descriptor page cursor.

use beryl_backend::{
    STREAMED_TEXT_MAX_PAGE_BYTES, StreamedInputSourceError, StreamedTextPage, StreamedTextSourceId,
};
use beryl_home_store::HomeStore;
use beryl_state::AssetLabelDisposition;
use syndic_storage::{SyndicContentTextSegment, SyndicContentTextSegmentBoundary, SyndicStorage};

use super::{
    identity::{GeneratedLabelKind, TextRunBlueprint, generated_label},
    source::MarkerSource,
};
use crate::cas_projection::ProjectionCancellationToken;

struct GeneratedFragment {
    boundary: SyndicContentTextSegmentBoundary,
    text: Box<str>,
    offset: usize,
    terminates_run: bool,
}

pub(super) struct TextPageState {
    run: TextRunBlueprint,
    expected_source_offset: u64,
    after_marker: Option<SyndicContentTextSegmentBoundary>,
    segment: Option<SyndicContentTextSegment>,
    segment_offset: u64,
    generated: Option<GeneratedFragment>,
    finished: bool,
}

impl TextPageState {
    pub(super) fn new(run: TextRunBlueprint) -> Self {
        let after_marker = run.start_boundary;
        let segment_offset = after_marker.map_or(0, |marker| marker.logical_offset());
        Self {
            run,
            expected_source_offset: 0,
            after_marker,
            segment: None,
            segment_offset,
            generated: None,
            finished: false,
        }
    }

    pub(super) const fn is_complete(&self) -> bool {
        self.finished && self.expected_source_offset == self.run.utf8_len
    }

    pub(super) fn descriptor(&self) -> beryl_backend::StreamedTextDescriptor {
        beryl_backend::StreamedTextDescriptor::new(
            self.run.source_id,
            self.run.proof,
            self.run.utf8_len,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn read(
        &mut self,
        source: &MarkerSource,
        store: &HomeStore,
        storage: &SyndicStorage,
        cancellation: &ProjectionCancellationToken,
        source_id: StreamedTextSourceId,
        start: u64,
        maximum: usize,
    ) -> Result<StreamedTextPage, StreamedInputSourceError> {
        source
            .check_authority(store, storage, cancellation)
            .map_err(|error| error.into_source())?;
        if source_id != self.run.source_id {
            return Err(StreamedInputSourceError::TextSourceIdMismatch {
                item_ordinal: self.run.descriptor_ordinal,
            });
        }
        if start != self.expected_source_offset {
            return Err(StreamedInputSourceError::PageStartMismatch {
                expected: self.expected_source_offset,
                actual: start,
            });
        }
        if maximum == 0 || maximum > STREAMED_TEXT_MAX_PAGE_BYTES {
            return Err(StreamedInputSourceError::InvalidSource);
        }
        if start >= self.run.utf8_len || self.finished {
            return Err(StreamedInputSourceError::PagePastEnd {
                end: start,
                utf8_len: self.run.utf8_len,
            });
        }

        let mut output = String::with_capacity(maximum);
        while output.len() < maximum && !self.finished {
            if self.generated.is_some() {
                self.append_generated(&mut output, maximum)?;
                continue;
            }
            if self.segment.is_none() {
                self.open_segment(source, store, storage, cancellation)?;
            }
            if self.segment_offset
                < self
                    .segment
                    .as_ref()
                    .expect("opened page state owns a segment")
                    .end()
            {
                // A storage page must be able to return one complete UTF-8 scalar. Preserve the
                // already assembled page when fewer than four bytes remain instead of turning a
                // valid boundary-crossing scalar into a read-limit failure.
                if !output.is_empty() && maximum - output.len() < 4 {
                    break;
                }
                self.append_authored(source, store, storage, cancellation, &mut output, maximum)?;
                continue;
            }
            self.finish_segment(source, store, storage, cancellation)?;
        }

        if output.is_empty() {
            return Err(StreamedInputSourceError::EmptyPage { start });
        }
        let page_bytes = output.len();
        let page_bytes_u64 = u64::try_from(page_bytes)
            .map_err(|_| StreamedInputSourceError::PageEndOverflow { start, page_bytes })?;
        let end = start
            .checked_add(page_bytes_u64)
            .ok_or(StreamedInputSourceError::PageEndOverflow { start, page_bytes })?;
        if end == self.run.utf8_len && !self.finished {
            self.finish_at_declared_end(source, store, storage, cancellation)?;
        }
        if end > self.run.utf8_len || self.finished != (end == self.run.utf8_len) {
            return Err(StreamedInputSourceError::InvalidSource);
        }
        self.expected_source_offset = end;
        let next_offset = (!self.finished).then_some(end);
        let page = streamed_text_page(
            source,
            self.run.source_id,
            self.run.proof,
            start,
            output,
            next_offset,
        );
        Ok(page)
    }

    fn finish_at_declared_end(
        &mut self,
        source: &MarkerSource,
        store: &HomeStore,
        storage: &SyndicStorage,
        cancellation: &ProjectionCancellationToken,
    ) -> Result<(), StreamedInputSourceError> {
        while !self.finished {
            if self.generated.is_some() {
                return Err(StreamedInputSourceError::InvalidSource);
            }
            if self.segment.is_none() {
                self.open_segment(source, store, storage, cancellation)?;
            }
            if self.segment_offset
                != self
                    .segment
                    .as_ref()
                    .expect("terminal page state owns a segment")
                    .end()
            {
                return Err(StreamedInputSourceError::InvalidSource);
            }
            self.finish_segment(source, store, storage, cancellation)?;
        }
        Ok(())
    }

    fn open_segment(
        &mut self,
        source: &MarkerSource,
        store: &HomeStore,
        storage: &SyndicStorage,
        cancellation: &ProjectionCancellationToken,
    ) -> Result<(), StreamedInputSourceError> {
        let segment = source
            .prove_segment(store, storage, cancellation, self.after_marker)
            .map_err(|error| error.into_source())?;
        let expected_start = self
            .after_marker
            .map_or(0, |marker| marker.logical_offset());
        let run_end = self
            .run
            .end_boundary
            .map_or(source.content().summary().logical_utf8_bytes(), |marker| {
                marker.logical_offset()
            });
        if segment.start() != expected_start
            || segment.end() < segment.start()
            || segment.end() > run_end
        {
            return Err(StreamedInputSourceError::MalformedTextSegmentation {
                item_ordinal: self.run.descriptor_ordinal,
            });
        }
        self.segment_offset = segment.start();
        self.segment = Some(segment);
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn append_authored(
        &mut self,
        source: &MarkerSource,
        store: &HomeStore,
        storage: &SyndicStorage,
        cancellation: &ProjectionCancellationToken,
        output: &mut String,
        maximum: usize,
    ) -> Result<(), StreamedInputSourceError> {
        let available = maximum - output.len();
        let segment = self
            .segment
            .as_ref()
            .expect("authored page state owns a segment");
        let page = source
            .read_segment_range(
                store,
                storage,
                cancellation,
                segment,
                self.segment_offset,
                available,
            )
            .map_err(|error| error.into_source())?;
        let page_bytes = u64::try_from(page.text().len())
            .map_err(|_| StreamedInputSourceError::InvalidSource)?;
        let end = self
            .segment_offset
            .checked_add(page_bytes)
            .ok_or(StreamedInputSourceError::InvalidSource)?;
        match (end < segment.end(), page.next_offset()) {
            (true, Some(next)) if next == end => {}
            (false, None) if end == segment.end() => {}
            _ => return Err(StreamedInputSourceError::InvalidSource),
        }
        output.push_str(&page.into_text());
        self.segment_offset = end;
        Ok(())
    }

    fn finish_segment(
        &mut self,
        source: &MarkerSource,
        store: &HomeStore,
        storage: &SyndicStorage,
        cancellation: &ProjectionCancellationToken,
    ) -> Result<(), StreamedInputSourceError> {
        let segment = self
            .segment
            .take()
            .expect("finished page segment is present");
        let Some(boundary) = segment.following_marker() else {
            if self.run.end_boundary.is_some() {
                return Err(StreamedInputSourceError::MalformedTextSegmentation {
                    item_ordinal: self.run.descriptor_ordinal,
                });
            }
            source
                .require_entry_eof(store, cancellation, self.after_marker)
                .map_err(|error| error.into_source())?;
            self.finished = true;
            return Ok(());
        };
        let entry = source
            .marker_entry(store, cancellation, boundary)
            .map_err(|error| error.into_source())?;
        source
            .validate_marker_entry(store, storage, cancellation, &entry)
            .map_err(|error| error.into_source())?;
        let kind = GeneratedLabelKind::from_disposition(entry.label_disposition());
        let terminates_run = entry.label_disposition() == AssetLabelDisposition::First;
        if terminates_run != (self.run.end_boundary == Some(boundary)) {
            return Err(StreamedInputSourceError::MalformedTextSegmentation {
                item_ordinal: self.run.descriptor_ordinal,
            });
        }
        self.generated = Some(GeneratedFragment {
            boundary,
            text: generated_label(kind, entry.label()),
            offset: 0,
            terminates_run,
        });
        Ok(())
    }

    fn append_generated(
        &mut self,
        output: &mut String,
        maximum: usize,
    ) -> Result<(), StreamedInputSourceError> {
        let generated = self
            .generated
            .as_mut()
            .expect("generated page fragment is present");
        let available = maximum - output.len();
        let remaining = &generated.text.as_bytes()[generated.offset..];
        let copied = available.min(remaining.len());
        let bytes = &remaining[..copied];
        output.push_str(
            std::str::from_utf8(bytes).map_err(|_| StreamedInputSourceError::InvalidSource)?,
        );
        generated.offset += copied;
        if generated.offset != generated.text.len() {
            return Ok(());
        }
        let generated = self
            .generated
            .take()
            .expect("completed generated fragment is present");
        if generated.terminates_run {
            self.finished = true;
        } else {
            self.after_marker = Some(generated.boundary);
            self.segment_offset = generated.boundary.logical_offset();
        }
        Ok(())
    }
}

fn streamed_text_page(
    source: &MarkerSource,
    source_id: StreamedTextSourceId,
    proof: beryl_backend::TextSourceProof,
    start: u64,
    text: String,
    next_offset: Option<u64>,
) -> StreamedTextPage {
    StreamedTextPage::new(
        source.source_identity(),
        source.source_revision(),
        source_id,
        proof,
        start,
        text,
        next_offset,
    )
}
