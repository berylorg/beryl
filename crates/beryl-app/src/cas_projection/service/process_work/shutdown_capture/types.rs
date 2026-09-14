use super::*;
use crate::cas_projection::ProjectionServiceGeneration;
use beryl_model::{InputGateRevision, SyndicTurnId};
use syndic_storage::{InputGateState, SelectedPathProof};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ShutdownWorkRevision {
    pub(super) required: RequiredWorkRevision,
    pub(super) flights: u64,
    pub(super) loaded: u64,
    pub(super) connections: ConnectionCustodyWorkStamp,
}

impl ShutdownWorkRevision {
    pub(crate) fn requires_connection_cleanup(&self) -> bool {
        self.connections.requires_cleanup()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ShutdownWorkCursor {
    pub(super) revision: ShutdownWorkRevision,
    pub(super) after: SyndicThreadId,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct Custody {
    pub(super) work: ProcessWorkFacts,
    pub(super) session_registered: bool,
    pub(super) preparation_retained: bool,
    pub(super) projection_flight: bool,
    pub(super) loaded_projection: bool,
}

impl Custody {
    pub(super) fn merge(&mut self, other: Self) {
        self.work.merge(other.work);
        self.session_registered |= other.session_registered;
        self.preparation_retained |= other.preparation_retained;
        self.projection_flight |= other.projection_flight;
        self.loaded_projection |= other.loaded_projection;
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ShutdownTerminalCompletion {
    pub(in crate::cas_projection::service::process_work) service_generation:
        ProjectionServiceGeneration,
    pub(in crate::cas_projection::service::process_work) observer:
        super::super::super::flight_registry::TerminalCompletionObserver,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ShutdownWorkRecord {
    pub(crate) thread_id: SyndicThreadId,
    pub(crate) selected_path: SelectedPathProof,
    pub(crate) current_turn_id: Option<SyndicTurnId>,
    pub(crate) gate_revision: InputGateRevision,
    pub(crate) gate_state: InputGateState,
    pub(crate) work: ProcessWorkFacts,
    pub(crate) session_registered: bool,
    pub(crate) preparation_retained: bool,
    pub(crate) projection_flight: bool,
    pub(crate) loaded_projection: bool,
    pub(crate) terminal_completion: Option<ShutdownTerminalCompletion>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ShutdownWorkPage {
    pub(crate) revision: ShutdownWorkRevision,
    pub(crate) records: Vec<ShutdownWorkRecord>,
    pub(crate) next_cursor: Option<ShutdownWorkCursor>,
    pub(crate) bytes: usize,
}
