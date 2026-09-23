use super::{ClientUserMessageId, ItemLifecycleTimestampMs, UserMessageEchoLifecycle};
use beryl_model::{CasItemId, CasThreadId, CasTurnId};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SteeringUserMessageCaptureMode {
    Verify,
    Passive,
}

pub struct UnverifiedSteeringUserMessage {
    pub(crate) lifecycle: UserMessageEchoLifecycle,
    pub(crate) thread_id: CasThreadId,
    pub(crate) turn_id: CasTurnId,
    pub(crate) item_id: CasItemId,
    pub(crate) timestamp: ItemLifecycleTimestampMs,
    pub(crate) client_user_message_id: ClientUserMessageId,
}

impl UnverifiedSteeringUserMessage {
    pub fn lifecycle(&self) -> UserMessageEchoLifecycle {
        self.lifecycle
    }
    pub fn thread_id(&self) -> &CasThreadId {
        &self.thread_id
    }
    pub fn turn_id(&self) -> &CasTurnId {
        &self.turn_id
    }
    pub fn item_id(&self) -> &CasItemId {
        &self.item_id
    }
    pub fn timestamp(&self) -> ItemLifecycleTimestampMs {
        self.timestamp
    }
    pub fn client_user_message_id(&self) -> &ClientUserMessageId {
        &self.client_user_message_id
    }
}
