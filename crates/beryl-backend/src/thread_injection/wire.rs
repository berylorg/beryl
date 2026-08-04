use std::{cell::RefCell, fmt};

use beryl_model::{CasThreadId, RecoveryItemSequenceAccumulator};
use serde::{
    Serialize, Serializer,
    ser::{SerializeSeq, SerializeStruct},
};

use crate::session::outbound::{
    JsonMessageWriter, OutboundWriteMetrics, SourceAwareWriteFailure, SourceFailureSlot,
    write_source_aware_json,
};

use super::{
    THREAD_INJECTION_MAX_PAGE_BYTES, ThreadInjectionPreflight, ThreadInjectionRole,
    ThreadInjectionSource, ThreadInjectionSourceError, ThreadInjectionSourcePage,
};

const SOURCE_FAILURE_SENTINEL: &str =
    "__BERYL_PRIVATE_THREAD_INJECTION_SOURCE_FAILURE_7B62B6AE1954__";

pub(crate) type ThreadInjectionSourceFailureSlot = SourceFailureSlot<ThreadInjectionSourceError>;
pub(crate) type ThreadInjectionWriteFailure<E> =
    SourceAwareWriteFailure<ThreadInjectionSourceError, E>;

pub(crate) fn write_injection_source_json<T, W>(
    writer: &mut W,
    message: &T,
    source_failure: &ThreadInjectionSourceFailureSlot,
) -> Result<OutboundWriteMetrics, ThreadInjectionWriteFailure<W::TransportError>>
where
    T: Serialize + ?Sized,
    W: JsonMessageWriter,
{
    write_source_aware_json(writer, message, source_failure)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ThreadInjectItemsParams<'a> {
    thread_id: &'a CasThreadId,
    items: ThreadInjectionItemsWire<'a>,
}

impl<'a> ThreadInjectItemsParams<'a> {
    pub(crate) fn new(
        thread_id: &'a CasThreadId,
        preflight: &'a ThreadInjectionPreflight,
        source: &'a mut dyn ThreadInjectionSource,
        failure: &'a ThreadInjectionSourceFailureSlot,
    ) -> Self {
        Self {
            thread_id,
            items: ThreadInjectionItemsWire {
                replay: RefCell::new(ThreadInjectionReplay::new(preflight, source)),
                failure,
            },
        }
    }
}

struct ThreadInjectionItemsWire<'a> {
    replay: RefCell<ThreadInjectionReplay<'a>>,
    failure: &'a ThreadInjectionSourceFailureSlot,
}

impl ThreadInjectionItemsWire<'_> {
    fn fail<T, E>(&self, source: ThreadInjectionSourceError) -> Result<T, E>
    where
        E: serde::ser::Error,
    {
        self.failure.record(source);
        Err(E::custom(SOURCE_FAILURE_SENTINEL))
    }
}

impl Serialize for ThreadInjectionItemsWire<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let item_count = self.replay.borrow().preflight.item_count();
        let sequence_length = usize::try_from(item_count)
            .expect("validated injection item limit fits every supported usize");
        let mut sequence = serializer.serialize_seq(Some(sequence_length))?;
        for item_ordinal in 1..=item_count {
            let (role, declared_item_utf8_bytes) =
                match self.replay.borrow_mut().begin_item(item_ordinal) {
                    Ok(header) => header,
                    Err(source) => return self.fail(source),
                };
            sequence.serialize_element(&ThreadInjectionItemWire {
                role,
                declared_item_utf8_bytes,
                replay: &self.replay,
                failure: self.failure,
            })?;
        }
        if let Err(source) = self.replay.borrow_mut().finish_sequence() {
            return self.fail(source);
        }
        sequence.end()
    }
}

struct ThreadInjectionItemWire<'a, 'source> {
    role: ThreadInjectionRole,
    declared_item_utf8_bytes: u64,
    replay: &'a RefCell<ThreadInjectionReplay<'source>>,
    failure: &'a ThreadInjectionSourceFailureSlot,
}

impl Serialize for ThreadInjectionItemWire<'_, '_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let (role, content_type) = match self.role {
            ThreadInjectionRole::UserInputText => ("user", "input_text"),
            ThreadInjectionRole::AssistantOutputText => ("assistant", "output_text"),
        };
        let mut message = serializer.serialize_struct("ThreadInjectionMessage", 3)?;
        message.serialize_field("type", "message")?;
        message.serialize_field("role", role)?;
        message.serialize_field(
            "content",
            &ThreadInjectionContentWire {
                content_type,
                text: ThreadInjectionTextWire {
                    declared_item_utf8_bytes: self.declared_item_utf8_bytes,
                    replay: self.replay,
                    failure: self.failure,
                },
            },
        )?;
        message.end()
    }
}

struct ThreadInjectionContentWire<'a, 'source> {
    content_type: &'static str,
    text: ThreadInjectionTextWire<'a, 'source>,
}

impl Serialize for ThreadInjectionContentWire<'_, '_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut sequence = serializer.serialize_seq(Some(1))?;
        sequence.serialize_element(&ThreadInjectionTextContentWire {
            content_type: self.content_type,
            text: &self.text,
        })?;
        sequence.end()
    }
}

struct ThreadInjectionTextContentWire<'a, 'text, 'source> {
    content_type: &'static str,
    text: &'a ThreadInjectionTextWire<'text, 'source>,
}

impl Serialize for ThreadInjectionTextContentWire<'_, '_, '_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut content = serializer.serialize_struct("ThreadInjectionText", 2)?;
        content.serialize_field("type", self.content_type)?;
        content.serialize_field("text", self.text)?;
        content.end()
    }
}

struct ThreadInjectionTextWire<'a, 'source> {
    declared_item_utf8_bytes: u64,
    replay: &'a RefCell<ThreadInjectionReplay<'source>>,
    failure: &'a ThreadInjectionSourceFailureSlot,
}

impl ThreadInjectionTextWire<'_, '_> {
    fn fail(
        &self,
        formatter: &mut fmt::Formatter<'_>,
        source: ThreadInjectionSourceError,
    ) -> fmt::Result {
        self.failure.record(source);
        formatter.write_str(SOURCE_FAILURE_SENTINEL)
    }
}

impl fmt::Display for ThreadInjectionTextWire<'_, '_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        loop {
            let page = match self.replay.borrow_mut().next_active_page() {
                Ok(page) => page,
                Err(source) => return self.fail(formatter, source),
            };
            formatter.write_str(page.text())?;
            if page.item_terminal() {
                debug_assert_eq!(
                    page.item_offset() + page.text().len() as u64,
                    self.declared_item_utf8_bytes
                );
                return Ok(());
            }
        }
    }
}

impl Serialize for ThreadInjectionTextWire<'_, '_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_str(self)
    }
}

#[derive(Clone, Copy)]
struct ActiveItem {
    ordinal: u64,
    role: ThreadInjectionRole,
    declared_utf8_bytes: u64,
    next_offset: u64,
}

struct ThreadInjectionReplay<'a> {
    preflight: &'a ThreadInjectionPreflight,
    source: &'a mut dyn ThreadInjectionSource,
    accumulator: Option<RecoveryItemSequenceAccumulator>,
    active: Option<ActiveItem>,
    pending: Option<ThreadInjectionSourcePage>,
}

impl<'a> ThreadInjectionReplay<'a> {
    fn new(
        preflight: &'a ThreadInjectionPreflight,
        source: &'a mut dyn ThreadInjectionSource,
    ) -> Self {
        Self {
            preflight,
            source,
            accumulator: Some(RecoveryItemSequenceAccumulator::new(
                preflight.item_count(),
                preflight.canonical_utf8_bytes(),
            )),
            active: None,
            pending: None,
        }
    }

    fn begin_item(
        &mut self,
        expected_ordinal: u64,
    ) -> Result<(ThreadInjectionRole, u64), ThreadInjectionSourceError> {
        debug_assert!(self.active.is_none());
        debug_assert!(self.pending.is_none());
        let page = self
            .read_page()?
            .ok_or(ThreadInjectionSourceError::PrematureEof {
                expected_item_ordinal: expected_ordinal,
            })?;
        self.validate_source(&page)?;
        if page.item_ordinal() != expected_ordinal {
            return Err(ThreadInjectionSourceError::ItemOrdinalMismatch {
                expected: expected_ordinal,
                actual: page.item_ordinal(),
            });
        }
        if page.declared_item_utf8_bytes() == 0 {
            return Err(ThreadInjectionSourceError::EmptyItem {
                item_ordinal: expected_ordinal,
            });
        }
        if page.item_offset() != 0 {
            return Err(ThreadInjectionSourceError::ItemOffsetMismatch {
                item_ordinal: expected_ordinal,
                expected: 0,
                actual: page.item_offset(),
            });
        }
        self.accumulator
            .as_mut()
            .expect("injection digest exists until exact EOF")
            .begin_item(
                expected_ordinal,
                page.role().sequence_role(),
                page.declared_item_utf8_bytes(),
            )?;
        self.active = Some(ActiveItem {
            ordinal: expected_ordinal,
            role: page.role(),
            declared_utf8_bytes: page.declared_item_utf8_bytes(),
            next_offset: 0,
        });
        let header = (page.role(), page.declared_item_utf8_bytes());
        self.pending = Some(page);
        Ok(header)
    }

    fn next_active_page(
        &mut self,
    ) -> Result<ThreadInjectionSourcePage, ThreadInjectionSourceError> {
        let active = self
            .active
            .expect("serializer begins an item before its text");
        let page = match self.pending.take() {
            Some(page) => page,
            None => self
                .read_page()?
                .ok_or(ThreadInjectionSourceError::PrematureEof {
                    expected_item_ordinal: active.ordinal,
                })?,
        };
        self.validate_source(&page)?;
        self.validate_active_page(active, &page)?;
        let page_bytes = u64::try_from(page.text().len()).expect("bounded page length fits u64");
        let end = active.next_offset.checked_add(page_bytes).ok_or(
            ThreadInjectionSourceError::ItemEndOverflow {
                item_ordinal: active.ordinal,
            },
        )?;
        self.accumulator
            .as_mut()
            .expect("injection digest exists until exact EOF")
            .update_text(page.text().as_bytes())?;
        if page.item_terminal() {
            self.accumulator
                .as_mut()
                .expect("injection digest exists until exact EOF")
                .finish_item()?;
            self.active = None;
        } else {
            self.active = Some(ActiveItem {
                next_offset: end,
                ..active
            });
        }
        Ok(page)
    }

    fn finish_sequence(&mut self) -> Result<(), ThreadInjectionSourceError> {
        debug_assert!(self.active.is_none());
        debug_assert!(self.pending.is_none());
        if self.read_page()?.is_some() {
            return Err(ThreadInjectionSourceError::PageAfterSequenceTerminal);
        }
        let actual = self
            .accumulator
            .take()
            .expect("injection digest is finished exactly once")
            .finish()?;
        let expected = self.preflight.sequence_digest();
        if actual != expected {
            return Err(ThreadInjectionSourceError::SequenceDigestMismatch { expected, actual });
        }
        Ok(())
    }

    fn read_page(
        &mut self,
    ) -> Result<Option<ThreadInjectionSourcePage>, ThreadInjectionSourceError> {
        if THREAD_INJECTION_MAX_PAGE_BYTES == 0 {
            return Err(ThreadInjectionSourceError::ZeroPageRequest);
        }
        let page = self.source.next_page(THREAD_INJECTION_MAX_PAGE_BYTES)?;
        if let Some(page) = &page {
            if page.text().is_empty() {
                return Err(ThreadInjectionSourceError::EmptyPage);
            }
            if page.text().len() > THREAD_INJECTION_MAX_PAGE_BYTES {
                return Err(ThreadInjectionSourceError::PageTooLarge {
                    maximum: THREAD_INJECTION_MAX_PAGE_BYTES,
                    actual: page.text().len(),
                });
            }
        }
        Ok(page)
    }

    fn validate_source(
        &self,
        page: &ThreadInjectionSourcePage,
    ) -> Result<(), ThreadInjectionSourceError> {
        let expected_identity = self.preflight.source_identity();
        if page.source_identity() != expected_identity {
            return Err(ThreadInjectionSourceError::SourceIdentityMismatch {
                expected: expected_identity,
                actual: page.source_identity(),
            });
        }
        let expected_revision = self.preflight.source_revision();
        if page.source_revision() != expected_revision {
            return Err(ThreadInjectionSourceError::SourceRevisionMismatch {
                expected: expected_revision,
                actual: page.source_revision(),
            });
        }
        Ok(())
    }

    fn validate_active_page(
        &self,
        active: ActiveItem,
        page: &ThreadInjectionSourcePage,
    ) -> Result<(), ThreadInjectionSourceError> {
        if page.item_ordinal() != active.ordinal {
            return Err(ThreadInjectionSourceError::ItemOrdinalMismatch {
                expected: active.ordinal,
                actual: page.item_ordinal(),
            });
        }
        if page.role() != active.role {
            return Err(ThreadInjectionSourceError::ItemRoleMismatch {
                item_ordinal: active.ordinal,
            });
        }
        if page.declared_item_utf8_bytes() != active.declared_utf8_bytes {
            return Err(ThreadInjectionSourceError::ItemLengthMismatch {
                item_ordinal: active.ordinal,
                expected: active.declared_utf8_bytes,
                actual: page.declared_item_utf8_bytes(),
            });
        }
        if page.item_offset() != active.next_offset {
            return Err(ThreadInjectionSourceError::ItemOffsetMismatch {
                item_ordinal: active.ordinal,
                expected: active.next_offset,
                actual: page.item_offset(),
            });
        }
        let page_bytes = u64::try_from(page.text().len()).expect("bounded page length fits u64");
        let end = active.next_offset.checked_add(page_bytes).ok_or(
            ThreadInjectionSourceError::ItemEndOverflow {
                item_ordinal: active.ordinal,
            },
        )?;
        if end > active.declared_utf8_bytes {
            return Err(ThreadInjectionSourceError::ItemPastDeclaredEnd {
                item_ordinal: active.ordinal,
                declared: active.declared_utf8_bytes,
                actual: end,
            });
        }
        let expected_item_terminal = end == active.declared_utf8_bytes;
        if page.item_terminal() != expected_item_terminal {
            return Err(ThreadInjectionSourceError::ItemTerminalMismatch {
                item_ordinal: active.ordinal,
            });
        }
        let expected_sequence_terminal =
            expected_item_terminal && active.ordinal == self.preflight.item_count();
        if page.sequence_terminal() != expected_sequence_terminal {
            return Err(ThreadInjectionSourceError::SequenceTerminalMismatch {
                item_ordinal: active.ordinal,
            });
        }
        Ok(())
    }
}
