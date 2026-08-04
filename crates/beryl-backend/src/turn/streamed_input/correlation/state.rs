use beryl_model::{CasItemId, CasThreadId, CasTurnId};

use super::{
    super::{
        StreamedInputHeader, StreamedInputPass, StreamedInputSource, StreamedLocalImageDescriptor,
        StreamedTextDescriptor, StreamedTextPage,
    },
    types::UserMessageEchoLifecycle,
};

pub(crate) struct StreamedUserMessageVerifier {
    pub(super) request_scope: u64,
    pub(super) target_thread_id: CasThreadId,
    pub(super) header: StreamedInputHeader,
    pub(super) source: Box<dyn StreamedInputSource>,
    pub(super) state: VerifierState,
    pub(super) echo: Option<EchoReplay>,
    pub(super) pending_lifecycle: Option<UserMessageEchoLifecycle>,
}

pub(super) enum VerifierState {
    Armed,
    Started {
        item_id: CasItemId,
        turn_id: CasTurnId,
    },
    Completed {
        turn_id: CasTurnId,
    },
}

pub(super) struct EchoReplay {
    pub(super) lifecycle: UserMessageEchoLifecycle,
    pub(super) pass: StreamedInputPass,
    pub(super) active: Option<ActiveInput>,
}

pub(super) enum ActiveInput {
    Text {
        item_index: u64,
        descriptor: StreamedTextDescriptor,
        offset: u64,
        page: Option<Box<StreamedTextPage>>,
        page_index: usize,
        finished: bool,
    },
    LocalImage {
        item_index: u64,
        descriptor: StreamedLocalImageDescriptor,
        path_offset: usize,
    },
}

impl ActiveInput {
    pub(super) const fn item_index(&self) -> u64 {
        match self {
            Self::Text { item_index, .. } | Self::LocalImage { item_index, .. } => *item_index,
        }
    }
}
