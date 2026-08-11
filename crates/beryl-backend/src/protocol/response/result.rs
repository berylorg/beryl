use crate::turn::{ThreadUnsubscribeResponse, ThreadUnsubscribeStatus};

use super::{ConfigReadResponse, InitializeResponse, ModelPage};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EmptyAcknowledgement {
    ThreadCompactStart,
    ThreadInjectItems,
    TurnInterrupt,
}

#[derive(Debug, PartialEq, Eq)]
pub enum BoundedResponseResult {
    Initialize(InitializeResponse),
    ConfigRead(ConfigReadResponse),
    ModelList(Box<ModelPage>),
    ThreadStart(crate::ThreadLineageResponse),
    ThreadRead(crate::ThreadReadMetadata),
    ThreadResume(crate::ThreadLineageResponse),
    ThreadFork(crate::ThreadLineageResponse),
    TurnStart(crate::turn::TurnStartResponseWire),
    TurnSteer(crate::TurnSteerResponseWire),
    EmptyAcknowledgement(EmptyAcknowledgement),
    ThreadUnsubscribe(ThreadUnsubscribeStatus),
}

impl ThreadUnsubscribeStatus {
    #[must_use]
    pub fn from_wire(value: &str) -> Option<Self> {
        Some(match value {
            "notLoaded" => Self::NotLoaded,
            "notSubscribed" => Self::NotSubscribed,
            "unsubscribed" => Self::Unsubscribed,
            _ => return None,
        })
    }
}

impl ThreadUnsubscribeResponse {
    #[must_use]
    pub const fn new(status: ThreadUnsubscribeStatus) -> Self {
        Self { status }
    }
}
