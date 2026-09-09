use beryl_model::{SyndicThreadId, SyndicTurnId};
use syndic_storage::{
    CompactionAttemptNonce, CompactionOperationId, CompactionOperationTarget,
    CompactionRequestDisposition,
};

use crate::cas_projection::ContextCompactionOutcome;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContinuationWorkStage {
    Accepting,
    Registered,
    Detached,
    Disposing,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContinuationWorkFact {
    pub yielding_turn_id: SyndicTurnId,
    pub pending: bool,
    pub compaction_turn_id: Option<SyndicTurnId>,
    pub stage: ContinuationWorkStage,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompactionCommandWorkStage {
    Preparing,
    PendingDispatch,
    Driver,
    Settling,
    Cleanup,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompactionOperationWorkFact {
    pub operation_id: CompactionOperationId,
    pub attempt: CompactionAttemptNonce,
    pub target: CompactionOperationTarget,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompactionWorkFact {
    pub yielding_turn_id: Option<SyndicTurnId>,
    pub operation: Option<CompactionOperationWorkFact>,
    pub command: Option<CompactionCommandWorkStage>,
    pub local_registered: bool,
    pub request_disposition: Option<CompactionRequestDisposition>,
    pub result: Option<ContextCompactionOutcome>,
}

impl CompactionWorkFact {
    pub(super) fn preparing(yielding_turn_id: Option<SyndicTurnId>) -> Self {
        Self {
            yielding_turn_id,
            operation: None,
            command: Some(CompactionCommandWorkStage::Preparing),
            local_registered: false,
            request_disposition: None,
            result: None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompactionWorkRecord {
    pub serial: u64,
    pub thread_id: SyndicThreadId,
    pub continuation: Option<ContinuationWorkFact>,
    pub compaction: Option<CompactionWorkFact>,
}

impl CompactionWorkRecord {
    pub fn thread_id(&self) -> SyndicThreadId {
        self.thread_id
    }

    pub(super) fn bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + self
                .compaction
                .as_ref()
                .and_then(|fact| fact.operation.as_ref())
                .map_or(0, |operation| {
                    operation.target.cas_thread_id().as_str().len()
                })
    }

    pub(super) fn prune(&mut self) {
        if self
            .compaction
            .as_ref()
            .is_some_and(|fact| fact.command.is_none() && !fact.local_registered)
        {
            self.compaction = None;
        }
    }

    pub(super) fn is_empty(&self) -> bool {
        self.continuation.is_none() && self.compaction.is_none()
    }
}
