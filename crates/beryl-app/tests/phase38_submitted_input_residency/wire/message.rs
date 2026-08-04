use super::{
    COMPLETED_AT_MS, InputSpec, RunIdentity, STARTED_AT_MS,
    content::{ContentBytes, ContentFlavor},
    generator::Part,
};

enum MessagePart {
    Empty,
    Simple(Part),
    Identity { kind: IdentityKind, index: usize },
}

#[derive(Clone, Copy)]
enum IdentityKind {
    Thread,
    Turn,
    Item,
}

impl MessagePart {
    const fn empty() -> Self {
        Self::Empty
    }

    const fn bytes(bytes: &'static [u8]) -> Self {
        Self::Simple(Part::bytes(bytes))
    }

    fn decimal(value: u64) -> Self {
        Self::Simple(Part::decimal(value))
    }

    const fn identity(kind: IdentityKind) -> Self {
        Self::Identity { kind, index: 0 }
    }

    fn next(&mut self, identity: &RunIdentity) -> Option<u8> {
        match self {
            Self::Empty => None,
            Self::Simple(part) => part.next_simple(),
            Self::Identity { kind, index } => {
                let bytes = match kind {
                    IdentityKind::Thread => identity.thread_id().as_bytes(),
                    IdentityKind::Turn => identity.turn_id().as_bytes(),
                    IdentityKind::Item => identity.item_id().as_bytes(),
                };
                let byte = bytes.get(*index).copied()?;
                *index += 1;
                Some(byte)
            }
        }
    }
}

pub(crate) struct ExpectedTurnStart {
    identity: RunIdentity,
    request_id: u64,
    content: ContentBytes,
    stage: RequestStage,
    part: MessagePart,
}

#[derive(Clone, Copy)]
enum RequestStage {
    Prefix,
    Id,
    ThreadPrefix,
    Thread,
    InputPrefix,
    Content,
    Suffix,
    Done,
}

impl ExpectedTurnStart {
    pub(crate) const fn new(identity: RunIdentity, request_id: u64, spec: InputSpec) -> Self {
        Self {
            identity,
            request_id,
            content: ContentBytes::new(spec, ContentFlavor::Request),
            stage: RequestStage::Prefix,
            part: MessagePart::empty(),
        }
    }

    fn schedule(&mut self) {
        match self.stage {
            RequestStage::Prefix => {
                self.part = MessagePart::bytes(b"{\"jsonrpc\":\"2.0\",\"id\":");
                self.stage = RequestStage::Id;
            }
            RequestStage::Id => {
                self.part = MessagePart::decimal(self.request_id);
                self.stage = RequestStage::ThreadPrefix;
            }
            RequestStage::ThreadPrefix => {
                self.part =
                    MessagePart::bytes(b",\"method\":\"turn/start\",\"params\":{\"threadId\":\"");
                self.stage = RequestStage::Thread;
            }
            RequestStage::Thread => {
                self.part = MessagePart::identity(IdentityKind::Thread);
                self.stage = RequestStage::InputPrefix;
            }
            RequestStage::InputPrefix => {
                self.part = MessagePart::bytes(b"\",\"input\":");
                self.stage = RequestStage::Content;
            }
            RequestStage::Content => {}
            RequestStage::Suffix => {
                self.part = MessagePart::bytes(b"}}");
                self.stage = RequestStage::Done;
            }
            RequestStage::Done => {}
        }
    }
}

impl Iterator for ExpectedTurnStart {
    type Item = u8;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(byte) = self.part.next(&self.identity) {
                return Some(byte);
            }
            if matches!(self.stage, RequestStage::Content) {
                if let Some(byte) = self.content.next() {
                    return Some(byte);
                }
                self.stage = RequestStage::Suffix;
            }
            if matches!(self.stage, RequestStage::Done) {
                return None;
            }
            self.schedule();
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LifecycleStage {
    Started,
    Completed,
}

pub(crate) struct LifecycleMessage {
    identity: RunIdentity,
    lifecycle: LifecycleStage,
    content: ContentBytes,
    stage: LifecycleMessageStage,
    part: MessagePart,
}

#[derive(Clone, Copy)]
enum LifecycleMessageStage {
    Prefix,
    Item,
    ContentPrefix,
    Content,
    ThreadPrefix,
    Thread,
    TurnPrefix,
    Turn,
    TimestampPrefix,
    Timestamp,
    Suffix,
    Done,
}

impl LifecycleMessage {
    pub(crate) const fn new(
        identity: RunIdentity,
        lifecycle: LifecycleStage,
        spec: InputSpec,
    ) -> Self {
        Self {
            identity,
            lifecycle,
            content: ContentBytes::new(spec, ContentFlavor::Echo),
            stage: LifecycleMessageStage::Prefix,
            part: MessagePart::empty(),
        }
    }

    fn schedule(&mut self) {
        match self.stage {
            LifecycleMessageStage::Prefix => {
                self.part = MessagePart::bytes(match self.lifecycle {
                    LifecycleStage::Started => b"{\"method\":\"item/started\",\"params\":{\"item\":{\"type\":\"userMessage\",\"id\":\"",
                    LifecycleStage::Completed => b"{\"method\":\"item/completed\",\"params\":{\"item\":{\"type\":\"userMessage\",\"id\":\"",
                });
                self.stage = LifecycleMessageStage::Item;
            }
            LifecycleMessageStage::Item => {
                self.part = MessagePart::identity(IdentityKind::Item);
                self.stage = LifecycleMessageStage::ContentPrefix;
            }
            LifecycleMessageStage::ContentPrefix => {
                self.part = MessagePart::bytes(b"\",\"clientId\":null,\"content\":");
                self.stage = LifecycleMessageStage::Content;
            }
            LifecycleMessageStage::Content => {}
            LifecycleMessageStage::ThreadPrefix => {
                self.part = MessagePart::bytes(b"},\"threadId\":\"");
                self.stage = LifecycleMessageStage::Thread;
            }
            LifecycleMessageStage::Thread => {
                self.part = MessagePart::identity(IdentityKind::Thread);
                self.stage = LifecycleMessageStage::TurnPrefix;
            }
            LifecycleMessageStage::TurnPrefix => {
                self.part = MessagePart::bytes(b"\",\"turnId\":\"");
                self.stage = LifecycleMessageStage::Turn;
            }
            LifecycleMessageStage::Turn => {
                self.part = MessagePart::identity(IdentityKind::Turn);
                self.stage = LifecycleMessageStage::TimestampPrefix;
            }
            LifecycleMessageStage::TimestampPrefix => {
                self.part = MessagePart::bytes(match self.lifecycle {
                    LifecycleStage::Started => b"\",\"startedAtMs\":",
                    LifecycleStage::Completed => b"\",\"completedAtMs\":",
                });
                self.stage = LifecycleMessageStage::Timestamp;
            }
            LifecycleMessageStage::Timestamp => {
                self.part = MessagePart::decimal(match self.lifecycle {
                    LifecycleStage::Started => STARTED_AT_MS,
                    LifecycleStage::Completed => COMPLETED_AT_MS,
                });
                self.stage = LifecycleMessageStage::Suffix;
            }
            LifecycleMessageStage::Suffix => {
                self.part = MessagePart::bytes(b"}}");
                self.stage = LifecycleMessageStage::Done;
            }
            LifecycleMessageStage::Done => {}
        }
    }
}

impl Iterator for LifecycleMessage {
    type Item = u8;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(byte) = self.part.next(&self.identity) {
                return Some(byte);
            }
            if matches!(self.stage, LifecycleMessageStage::Content) {
                if let Some(byte) = self.content.next() {
                    return Some(byte);
                }
                self.stage = LifecycleMessageStage::ThreadPrefix;
            }
            if matches!(self.stage, LifecycleMessageStage::Done) {
                return None;
            }
            self.schedule();
        }
    }
}

pub(crate) struct TurnStartResponse {
    identity: RunIdentity,
    request_id: u64,
    stage: ResponseStage,
    part: MessagePart,
}

#[derive(Clone, Copy)]
enum ResponseStage {
    Prefix,
    Id,
    TurnPrefix,
    Turn,
    Suffix,
    Done,
}

impl TurnStartResponse {
    pub(crate) const fn new(identity: RunIdentity, request_id: u64) -> Self {
        Self {
            identity,
            request_id,
            stage: ResponseStage::Prefix,
            part: MessagePart::empty(),
        }
    }

    fn schedule(&mut self) {
        match self.stage {
            ResponseStage::Prefix => {
                self.part = MessagePart::bytes(b"{\"id\":");
                self.stage = ResponseStage::Id;
            }
            ResponseStage::Id => {
                self.part = MessagePart::decimal(self.request_id);
                self.stage = ResponseStage::TurnPrefix;
            }
            ResponseStage::TurnPrefix => {
                self.part = MessagePart::bytes(b",\"result\":{\"turn\":{\"id\":\"");
                self.stage = ResponseStage::Turn;
            }
            ResponseStage::Turn => {
                self.part = MessagePart::identity(IdentityKind::Turn);
                self.stage = ResponseStage::Suffix;
            }
            ResponseStage::Suffix => {
                self.part = MessagePart::bytes(b"\",\"items\":[],\"status\":\"inProgress\"}}}");
                self.stage = ResponseStage::Done;
            }
            ResponseStage::Done => {}
        }
    }
}

impl Iterator for TurnStartResponse {
    type Item = u8;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(byte) = self.part.next(&self.identity) {
                return Some(byte);
            }
            if matches!(self.stage, ResponseStage::Done) {
                return None;
            }
            self.schedule();
        }
    }
}

pub(crate) struct TerminalMessage {
    identity: RunIdentity,
    stage: TerminalStage,
    part: MessagePart,
}

#[derive(Clone, Copy)]
enum TerminalStage {
    Prefix,
    Thread,
    TurnPrefix,
    Turn,
    Suffix,
    Done,
}

impl TerminalMessage {
    pub(crate) const fn new(identity: RunIdentity) -> Self {
        Self {
            identity,
            stage: TerminalStage::Prefix,
            part: MessagePart::empty(),
        }
    }

    fn schedule(&mut self) {
        match self.stage {
            TerminalStage::Prefix => {
                self.part = MessagePart::bytes(
                    b"{\"method\":\"turn/completed\",\"params\":{\"threadId\":\"",
                );
                self.stage = TerminalStage::Thread;
            }
            TerminalStage::Thread => {
                self.part = MessagePart::identity(IdentityKind::Thread);
                self.stage = TerminalStage::TurnPrefix;
            }
            TerminalStage::TurnPrefix => {
                self.part = MessagePart::bytes(b"\",\"turn\":{\"id\":\"");
                self.stage = TerminalStage::Turn;
            }
            TerminalStage::Turn => {
                self.part = MessagePart::identity(IdentityKind::Turn);
                self.stage = TerminalStage::Suffix;
            }
            TerminalStage::Suffix => {
                self.part = MessagePart::bytes(
                    b"\",\"items\":[],\"itemsView\":\"notLoaded\",\"status\":\"completed\",\"error\":null,\"startedAt\":38010,\"completedAt\":38011,\"durationMs\":1}}}",
                );
                self.stage = TerminalStage::Done;
            }
            TerminalStage::Done => {}
        }
    }
}

impl Iterator for TerminalMessage {
    type Item = u8;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(byte) = self.part.next(&self.identity) {
                return Some(byte);
            }
            if matches!(self.stage, TerminalStage::Done) {
                return None;
            }
            self.schedule();
        }
    }
}
