use super::*;
use gpui_text_input::{
    ByteOffset, InlineObjectFact, InlineObjectId, InlineObjectOrder, InlineObjectPresentation,
    ObjectCursor,
};
use syndic_storage::{
    DraftCompositeSearchKeyV1, DraftPieceMarkerAtV1, DraftPieceMarkerDemandV1,
    DraftPieceMarkerDirectionV1, DraftPieceMarkerScopeV1, DraftPieceTextDemandV1,
};

pub(super) enum ReplayItem {
    Text {
        offset: u64,
        text: Box<str>,
    },
    Marker {
        offset: u64,
        marker: syndic_storage::DraftPieceMarkerV1,
    },
}

#[derive(Debug, thiserror::Error)]
pub(super) enum PrivateReplayError {
    #[error("private clipboard text or closure correlation changed")]
    Correlation,
    #[error("{0}")]
    Unavailable(String),
}

impl From<&str> for PrivateReplayError {
    fn from(error: &str) -> Self {
        Self::Unavailable(error.into())
    }
}

impl From<String> for PrivateReplayError {
    fn from(error: String) -> Self {
        Self::Unavailable(error)
    }
}

pub(super) struct PrivateReplay {
    descriptor: super::super::source::MainWindowPrivateClipboardDescriptor,
    text_cursor: u64,
    marker_cursor: Option<DraftCompositeSearchKeyV1>,
    pending_marker: Option<DraftPieceMarkerAtV1>,
    marker_complete: bool,
    output_cursor: usize,
    provenance: gpui_text_input::ClipboardProvenanceReplay,
    finished: bool,
}

impl PrivateReplay {
    pub(super) fn new(
        descriptor: super::super::source::MainWindowPrivateClipboardDescriptor,
    ) -> Self {
        Self {
            descriptor,
            text_cursor: descriptor
                .selection
                .range()
                .expect("copied range")
                .start()
                .byte_offset
                .get(),
            marker_cursor: None,
            pending_marker: None,
            marker_complete: false,
            output_cursor: 0,
            provenance: gpui_text_input::ClipboardProvenanceReplay::new(
                descriptor.provenance_limits,
            ),
            finished: false,
        }
    }

    pub(super) fn next(
        &mut self,
        storage: &syndic_storage::SyndicStorage,
        store: &beryl_home_store::HomeStore,
        fallback: &str,
        page_bytes: usize,
    ) -> Result<Option<ReplayItem>, PrivateReplayError> {
        if self.finished {
            return Ok(None);
        }
        let range = self
            .descriptor
            .selection
            .range()
            .map_err(|_| "private clipboard range changed")?;
        while self.pending_marker.is_none() && !self.marker_complete {
            let page = storage
                .draft_piece_marker_demand(
                    store,
                    self.descriptor.source.content_root(),
                    DraftPieceMarkerDemandV1::new(
                        DraftPieceMarkerScopeV1::InclusiveRange {
                            start: range.start().byte_offset.get(),
                            end: range.end().byte_offset.get(),
                        },
                        DraftPieceMarkerDirectionV1::Forward,
                        self.marker_cursor,
                        1,
                        self.descriptor.provenance_limits.max_page_retained_bytes(),
                    ),
                )
                .map_err(|_| "private clipboard marker source is unavailable")?;
            self.marker_complete = page.requested_side_complete();
            if let Some(at) = page.markers().first() {
                let marker = at.marker();
                let cursor = ObjectCursor::new(
                    ByteOffset::new(at.anchor()),
                    InlineObjectOrder::new(u128::from(marker.order_key())),
                    InlineObjectId::new(u128::from_be_bytes(*marker.marker_id().as_bytes())),
                );
                let next = DraftCompositeSearchKeyV1::Marker {
                    anchor: at.anchor(),
                    order_key: marker.order_key(),
                    marker_id: marker.marker_id(),
                };
                if self.marker_cursor == Some(next) {
                    return Err("private clipboard marker source made no progress".into());
                }
                self.marker_cursor = Some(next);
                if super::super::paging::object_follows_start(cursor, range.start())
                    && super::super::paging::object_precedes_end(cursor, range.end())
                {
                    self.pending_marker = Some(at.clone());
                }
            } else if !self.marker_complete {
                return Err("private clipboard marker source made no progress".into());
            }
        }
        let next_anchor = self
            .pending_marker
            .as_ref()
            .map_or(range.end().byte_offset.get(), |at| at.anchor());
        if self.text_cursor < next_anchor {
            let page = storage
                .draft_piece_text_demand(
                    store,
                    self.descriptor.source.content_root(),
                    DraftPieceTextDemandV1::Forward(self.text_cursor),
                    page_bytes,
                )
                .map_err(|_| "private clipboard text source is unavailable")?;
            if page.start() != self.text_cursor || page.end() <= self.text_cursor {
                return Err("private clipboard text source made no progress".into());
            }
            let bytes = usize::try_from(next_anchor.min(page.end()) - self.text_cursor)
                .map_err(|_| "private clipboard text extent is unavailable")?;
            let text = std::str::from_utf8(&page.bytes()[..bytes])
                .map_err(|_| "private clipboard text is malformed")?;
            self.compare(fallback, text)?;
            let offset = self.text_cursor - range.start().byte_offset.get();
            self.text_cursor += bytes as u64;
            return Ok(Some(ReplayItem::Text {
                offset,
                text: text.into(),
            }));
        }
        if let Some(at) = self.pending_marker.take() {
            let marker = at.marker();
            let label = marker.label().to_string();
            let copy = format!("[Image {label}]");
            let start = self.output_cursor;
            self.compare(fallback, &copy)?;
            let presentation = InlineObjectPresentation::new(
                self.descriptor
                    .content_origin
                    .binding()
                    .presentation_generation()
                    .get(),
                format!("[{label}]"),
                gpui::px(18.0 + label.len() as f32 * 8.0),
                gpui::px(22.0),
                gpui::px(17.0),
                None,
                marker.label().get(),
                true,
            )
            .map_err(|_| "private clipboard marker presentation is unavailable")?;
            let fact = InlineObjectFact::new(
                InlineObjectId::new(u128::from_be_bytes(*marker.marker_id().as_bytes())),
                ByteOffset::new(at.anchor()),
                InlineObjectOrder::new(u128::from(marker.order_key())),
                copy,
                presentation,
            );
            let full = self
                .provenance
                .push(&fact, start, self.output_cursor)
                .map_err(|_| "private clipboard provenance exceeds its page allowance")?;
            if full {
                self.acknowledge_page()?;
            }
            return Ok(Some(ReplayItem::Marker {
                offset: at.anchor() - range.start().byte_offset.get(),
                marker,
            }));
        }
        if self.provenance.has_items() {
            self.acknowledge_page()?;
        }
        let closure = self
            .provenance
            .closure(self.descriptor.clipboard_key, fallback)
            .map_err(|_| "private clipboard closure is unavailable")?;
        if closure != self.descriptor.closure || self.output_cursor != fallback.len() {
            return Err(PrivateReplayError::Correlation);
        }
        self.finished = true;
        Ok(None)
    }

    fn acknowledge_page(&mut self) -> Result<(), String> {
        let page = self
            .provenance
            .emit(self.descriptor.clipboard_key)
            .map_err(|_| "private clipboard provenance page is unavailable")?;
        self.provenance
            .acknowledge(page)
            .map_err(|_| "private clipboard provenance acknowledgement changed".into())
    }

    fn compare(&mut self, fallback: &str, text: &str) -> Result<(), PrivateReplayError> {
        let end = self
            .output_cursor
            .checked_add(text.len())
            .ok_or("private clipboard fallback extent overflowed")?;
        if fallback.get(self.output_cursor..end) != Some(text) {
            return Err(PrivateReplayError::Correlation);
        }
        self.output_cursor = end;
        Ok(())
    }
}

pub(super) fn count_line_breaks(
    storage: &syndic_storage::SyndicStorage,
    store: &beryl_home_store::HomeStore,
    root: syndic_storage::DraftPieceRootReferenceV1,
    range: SourceRange,
    page_bytes: usize,
    cancellation: &CommandCancellation,
) -> Result<u64, String> {
    let mut cursor = range.start().byte_offset.get();
    let end = range.end().byte_offset.get();
    let mut breaks = 0u64;
    while cursor < end {
        if cancellation.is_cancelled() {
            return Err("paste cancelled".into());
        }
        let page = storage
            .draft_piece_text_demand(
                store,
                root,
                DraftPieceTextDemandV1::Forward(cursor),
                page_bytes,
            )
            .map_err(|_| "paste replacement text source unavailable")?;
        if page.start() != cursor || page.end() <= cursor {
            return Err("paste replacement text made no progress".into());
        }
        let count = usize::try_from(end.min(page.end()) - cursor)
            .map_err(|_| "paste replacement extent unavailable")?;
        breaks = breaks
            .checked_add(
                page.bytes()[..count]
                    .iter()
                    .filter(|byte| **byte == b'\n')
                    .count() as u64,
            )
            .ok_or("paste replacement line count overflowed")?;
        cursor += count as u64;
    }
    Ok(breaks)
}
