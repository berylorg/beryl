use beryl_home_store::HomeStore;
use syndic_storage::{SyndicCaptureItem, SyndicPointReadLimit, SyndicStorage};

use syndic_storage::TurnIncompleteReason;

use super::{ItemText, LiveCapture};
use crate::cas_projection::ordinary::OrdinaryTurnExecutionError;

pub(super) const COALESCED_DELTA_MAX_BYTES: usize = 64 * 1024;

impl LiveCapture {
    pub(super) fn append_authoritative_text(
        &mut self,
        store: &HomeStore,
        storage: SyndicStorage,
        item: &SyndicCaptureItem,
        text: ItemText<'_>,
        limit: SyndicPointReadLimit,
    ) -> Result<bool, OrdinaryTurnExecutionError> {
        let source = item
            .item()
            .cas_source()
            .ok_or(OrdinaryTurnExecutionError::Invariant(
                "canonical text item has no exact CAS source",
            ))?;
        let cas_item_id = source.item_id().clone();
        let expected_kind = item.item().provider_kind();
        let Some(content) = item.item().payload().content() else {
            self.note_incomplete(TurnIncompleteReason::CompletionMismatch);
            return Ok(false);
        };
        let durable_bytes =
            usize::try_from(content.summary().logical_utf8_bytes()).map_err(|_| {
                OrdinaryTurnExecutionError::Invariant(
                    "durable live text byte frontier exceeds the process address space",
                )
            })?;
        let total_bytes = total_bytes(text.clone()).ok_or(
            OrdinaryTurnExecutionError::Invariant("completed item text byte length overflowed"),
        )?;
        if durable_bytes > total_bytes {
            self.note_incomplete(TurnIncompleteReason::CompletionMismatch);
            return Ok(false);
        }

        let mut cursor = FragmentCursor::new(text);
        if !validate_prefix(store, storage, item, durable_bytes, limit, &mut cursor)? {
            self.note_incomplete(TurnIncompleteReason::CompletionMismatch);
            return Ok(false);
        }
        while let Some(fragment) = cursor.next_remaining() {
            self.queue_text(
                store,
                storage,
                cas_item_id.clone(),
                expected_kind,
                fragment,
                limit,
            )?;
        }
        self.flush_delta(store, storage, limit)?;
        Ok(true)
    }
}

fn total_bytes(mut text: ItemText<'_>) -> Option<usize> {
    text.try_fold(0_usize, |total, fragment| total.checked_add(fragment.len()))
}

fn validate_prefix(
    store: &HomeStore,
    storage: SyndicStorage,
    item: &SyndicCaptureItem,
    durable_bytes: usize,
    limit: SyndicPointReadLimit,
    cursor: &mut FragmentCursor<'_>,
) -> Result<bool, OrdinaryTurnExecutionError> {
    let mut offset = 0_usize;
    while offset < durable_bytes {
        let remaining = durable_bytes - offset;
        let page = storage.capture_item_text_range(
            store,
            item,
            u64::try_from(offset).map_err(|_| {
                OrdinaryTurnExecutionError::Invariant("durable text comparison offset exceeds u64")
            })?,
            remaining.min(COALESCED_DELTA_MAX_BYTES),
            limit,
        )?;
        if page.text().is_empty() || !cursor.matches(page.text().as_bytes()) {
            return Ok(false);
        }
        let end =
            offset
                .checked_add(page.text().len())
                .ok_or(OrdinaryTurnExecutionError::Invariant(
                    "durable text comparison frontier overflowed",
                ))?;
        let expected_next = (end < durable_bytes).then_some(u64::try_from(end).map_err(|_| {
            OrdinaryTurnExecutionError::Invariant("durable text continuation exceeds u64")
        })?);
        if end > durable_bytes || page.next_offset() != expected_next {
            return Err(OrdinaryTurnExecutionError::Invariant(
                "durable text page continuation disagrees with its content frontier",
            ));
        }
        offset = end;
    }
    Ok(true)
}

struct FragmentCursor<'a> {
    fragments: ItemText<'a>,
    current: &'a str,
    offset: usize,
}

impl<'a> FragmentCursor<'a> {
    fn new(fragments: ItemText<'a>) -> Self {
        Self {
            fragments,
            current: "",
            offset: 0,
        }
    }

    fn matches(&mut self, mut expected: &[u8]) -> bool {
        while !expected.is_empty() {
            if !self.ensure_current() {
                return false;
            }
            let available = &self.current.as_bytes()[self.offset..];
            let take = available.len().min(expected.len());
            if available[..take] != expected[..take] {
                return false;
            }
            self.offset += take;
            expected = &expected[take..];
        }
        true
    }

    fn next_remaining(&mut self) -> Option<&'a str> {
        if self.ensure_current() {
            let remaining = self.current.get(self.offset..)?;
            self.offset = self.current.len();
            return Some(remaining);
        }
        None
    }

    fn ensure_current(&mut self) -> bool {
        while self.offset == self.current.len() {
            let Some(next) = self.fragments.next() else {
                return false;
            };
            self.current = next;
            self.offset = 0;
        }
        true
    }
}

pub(super) fn bounded_utf8_parts(text: &str) -> BoundedUtf8Parts<'_> {
    BoundedUtf8Parts { remaining: text }
}

pub(super) struct BoundedUtf8Parts<'a> {
    remaining: &'a str,
}

impl<'a> Iterator for BoundedUtf8Parts<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining.is_empty() {
            return None;
        }
        let mut end = self.remaining.len().min(COALESCED_DELTA_MAX_BYTES);
        while !self.remaining.is_char_boundary(end) {
            end -= 1;
        }
        if end == 0 {
            end = self
                .remaining
                .char_indices()
                .nth(1)
                .map_or(self.remaining.len(), |(index, _)| index);
        }
        let (part, remainder) = self.remaining.split_at(end);
        self.remaining = remainder;
        Some(part)
    }
}
