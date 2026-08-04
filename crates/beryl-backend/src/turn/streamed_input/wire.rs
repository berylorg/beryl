use std::{cell::RefCell, fmt};

use beryl_model::{CasThreadId, CasTurnId};
use serde::{
    Serialize, Serializer,
    ser::{SerializeSeq, SerializeStruct},
};

use crate::{
    ClientUserMessageId, TurnStartOptions,
    session::outbound::{SourceAwareWriteFailure, SourceFailureSlot},
};

use super::{
    StreamedInputDescriptorKind, StreamedInputHeader, StreamedInputPass, StreamedInputSource,
    StreamedInputSourceError, StreamedTextDescriptor, StreamedUserMessageVerifierSlot,
};

const SOURCE_FAILURE_SENTINEL: &str =
    "__BERYL_PRIVATE_STREAMED_INPUT_SOURCE_FAILURE_2C739F2A51B4__";

pub(crate) type StreamedInputSourceFailureSlot = SourceFailureSlot<StreamedInputSourceError>;
pub(crate) type StreamedInputJsonWriteFailure<E> =
    SourceAwareWriteFailure<StreamedInputSourceError, E>;
pub(crate) use crate::session::outbound::write_source_aware_json;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StreamedTurnStartParams<'a> {
    thread_id: &'a CasThreadId,
    input: StreamedInputWire<'a>,
    #[serde(skip_serializing_if = "Option::is_none")]
    model: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    effort: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    collaboration_mode: Option<StreamedTurnStartCollaborationMode<'a>>,
}

impl<'a> StreamedTurnStartParams<'a> {
    pub(crate) fn new(
        thread_id: &'a CasThreadId,
        verifier: &'a StreamedUserMessageVerifierSlot,
        options: &'a TurnStartOptions,
        failure: &'a StreamedInputSourceFailureSlot,
    ) -> Self {
        let collaboration_mode = options.developer_instructions_context().map(|context| {
            StreamedTurnStartCollaborationMode {
                mode: StreamedTurnStartCollaborationModeKind::Default,
                settings: StreamedTurnStartCollaborationModeSettings {
                    model: context.model(),
                    reasoning_effort: context.reasoning_effort(),
                    developer_instructions: context.developer_instructions(),
                },
            }
        });
        Self {
            thread_id,
            input: StreamedInputWire {
                source: StreamedInputWireSource::Verifier(verifier),
                failure,
            },
            model: options.model(),
            effort: options.reasoning_effort(),
            collaboration_mode,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StreamedTurnSteerParams<'a> {
    thread_id: &'a CasThreadId,
    client_user_message_id: &'a ClientUserMessageId,
    input: StreamedInputWire<'a>,
    expected_turn_id: &'a CasTurnId,
}

impl<'a> StreamedTurnSteerParams<'a> {
    pub(crate) fn new(
        thread_id: &'a CasThreadId,
        client_user_message_id: &'a ClientUserMessageId,
        expected_turn_id: &'a CasTurnId,
        header: StreamedInputHeader,
        source: &'a RefCell<Box<dyn StreamedInputSource>>,
        failure: &'a StreamedInputSourceFailureSlot,
    ) -> Self {
        Self {
            thread_id,
            client_user_message_id,
            input: StreamedInputWire {
                source: StreamedInputWireSource::Direct { header, source },
                failure,
            },
            expected_turn_id,
        }
    }
}

struct StreamedInputWire<'a> {
    source: StreamedInputWireSource<'a>,
    failure: &'a StreamedInputSourceFailureSlot,
}

enum StreamedInputWireSource<'a> {
    Verifier(&'a StreamedUserMessageVerifierSlot),
    Direct {
        header: StreamedInputHeader,
        source: &'a RefCell<Box<dyn StreamedInputSource>>,
    },
}

impl StreamedInputWire<'_> {
    fn fail<T, E>(&self, source: StreamedInputSourceError) -> Result<T, E>
    where
        E: serde::ser::Error,
    {
        self.failure.record(source);
        Err(E::custom(SOURCE_FAILURE_SENTINEL))
    }
}

impl Serialize for StreamedInputWire<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match &self.source {
            StreamedInputWireSource::Verifier(verifier) => {
                let mut verifier = match verifier.lock() {
                    Ok(verifier) => verifier,
                    Err(_) => return self.fail(StreamedInputSourceError::VerifierUnavailable),
                };
                let (header, source) = verifier.source_and_header();
                self.serialize_source(serializer, header, source)
            }
            StreamedInputWireSource::Direct { header, source } => {
                let mut source = match source.try_borrow_mut() {
                    Ok(source) => source,
                    Err(_) => return self.fail(StreamedInputSourceError::VerifierUnavailable),
                };
                self.serialize_source(serializer, *header, &mut **source)
            }
        }
    }
}

impl StreamedInputWire<'_> {
    fn serialize_source<S>(
        &self,
        serializer: S,
        header: StreamedInputHeader,
        source: &mut dyn StreamedInputSource,
    ) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let pass = match StreamedInputPass::begin(header, source) {
            Ok(pass) => pass,
            Err(source) => return self.fail(source),
        };
        let replay = RefCell::new(RequestReplay { source, pass });
        let mut sequence = serializer.serialize_seq(None)?;
        for item_ordinal in 1..=header.item_count() {
            let kind = match replay.borrow_mut().next_descriptor() {
                Ok(Some(kind)) => kind,
                Ok(None) => unreachable!("declared request item has a descriptor"),
                Err(source) => return self.fail(source),
            };
            sequence.serialize_element(&StreamedDescriptorWire {
                item_ordinal,
                kind,
                replay: &replay,
                failure: self.failure,
            })?;
        }
        if let Err(source) = replay.borrow_mut().finish() {
            return self.fail(source);
        }
        sequence.end()
    }
}

struct RequestReplay<'a> {
    source: &'a mut dyn StreamedInputSource,
    pass: StreamedInputPass,
}

impl RequestReplay<'_> {
    fn next_descriptor(
        &mut self,
    ) -> Result<Option<StreamedInputDescriptorKind>, StreamedInputSourceError> {
        self.pass.next_descriptor(self.source)
    }

    fn read_text_page(
        &mut self,
        item_ordinal: u64,
        descriptor: &StreamedTextDescriptor,
        start: u64,
    ) -> Result<super::StreamedTextPage, StreamedInputSourceError> {
        self.pass
            .read_text_page(self.source, item_ordinal, descriptor, start)
    }

    fn finish(&mut self) -> Result<(), StreamedInputSourceError> {
        self.pass.finish(self.source)
    }
}

struct StreamedDescriptorWire<'a, 'source> {
    item_ordinal: u64,
    kind: StreamedInputDescriptorKind,
    replay: &'a RefCell<RequestReplay<'source>>,
    failure: &'a StreamedInputSourceFailureSlot,
}

impl Serialize for StreamedDescriptorWire<'_, '_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match &self.kind {
            StreamedInputDescriptorKind::Text(descriptor) => {
                let mut state = serializer.serialize_struct("UserInput", 2)?;
                state.serialize_field("type", "text")?;
                state.serialize_field(
                    "text",
                    &StreamedTextWire {
                        item_ordinal: self.item_ordinal,
                        descriptor,
                        replay: self.replay,
                        failure: self.failure,
                    },
                )?;
                state.end()
            }
            StreamedInputDescriptorKind::LocalImage(image) => {
                let field_count = if image.detail().is_some() { 3 } else { 2 };
                let mut state = serializer.serialize_struct("UserInput", field_count)?;
                state.serialize_field("type", "localImage")?;
                if let Some(detail) = image.detail() {
                    state.serialize_field("detail", &detail)?;
                }
                state.serialize_field("path", image.path())?;
                state.end()
            }
        }
    }
}

struct StreamedTextWire<'a, 'text, 'source> {
    item_ordinal: u64,
    descriptor: &'text StreamedTextDescriptor,
    replay: &'a RefCell<RequestReplay<'source>>,
    failure: &'a StreamedInputSourceFailureSlot,
}

impl StreamedTextWire<'_, '_, '_> {
    fn fail(
        &self,
        formatter: &mut fmt::Formatter<'_>,
        source: StreamedInputSourceError,
    ) -> fmt::Result {
        self.failure.record(source);
        formatter.write_str(SOURCE_FAILURE_SENTINEL)
    }
}

impl fmt::Display for StreamedTextWire<'_, '_, '_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut start = 0_u64;
        loop {
            let page = match self.replay.borrow_mut().read_text_page(
                self.item_ordinal,
                self.descriptor,
                start,
            ) {
                Ok(page) => page,
                Err(source) => return self.fail(formatter, source),
            };
            formatter.write_str(page.text())?;
            match page.next_offset() {
                Some(next_offset) => start = next_offset,
                None => return Ok(()),
            }
        }
    }
}

impl Serialize for StreamedTextWire<'_, '_, '_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_str(self)
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StreamedTurnStartCollaborationMode<'a> {
    mode: StreamedTurnStartCollaborationModeKind,
    settings: StreamedTurnStartCollaborationModeSettings<'a>,
}

#[derive(Serialize)]
#[serde(rename_all = "lowercase")]
enum StreamedTurnStartCollaborationModeKind {
    Default,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
struct StreamedTurnStartCollaborationModeSettings<'a> {
    model: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning_effort: Option<&'a str>,
    developer_instructions: Option<&'a str>,
}
