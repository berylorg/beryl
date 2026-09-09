use beryl_model::{InputGateRevision, SyndicThreadId};

use crate::{InputGateRecord, InputGateState};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NonIdleGateSourceRecord {
    thread_id: SyndicThreadId,
    gate_revision: InputGateRevision,
}

impl NonIdleGateSourceRecord {
    pub const fn new(thread_id: SyndicThreadId, gate_revision: InputGateRevision) -> Self {
        Self {
            thread_id,
            gate_revision,
        }
    }

    pub const fn thread_id(self) -> SyndicThreadId {
        self.thread_id
    }

    pub const fn gate_revision(self) -> InputGateRevision {
        self.gate_revision
    }

    pub(crate) fn for_gate(gate: &InputGateRecord) -> Option<Self> {
        (!matches!(gate.state(), InputGateState::Idle))
            .then(|| Self::new(gate.thread_id(), gate.revision()))
    }
}

pub(crate) fn non_idle_gate_source_matches(
    thread_id: SyndicThreadId,
    gate: Option<&InputGateRecord>,
    source: Option<&NonIdleGateSourceRecord>,
) -> bool {
    if gate.is_some_and(|gate| gate.thread_id() != thread_id) {
        return false;
    }
    gate.and_then(NonIdleGateSourceRecord::for_gate).as_ref() == source
}
