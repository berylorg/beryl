use beryl_model::{ExecutionBinding, SyndicThreadId};

/// Immutable canonical execution authority for one named Syndic thread.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThreadExecutionRecord {
    thread_id: SyndicThreadId,
    execution: ExecutionBinding,
}

impl ThreadExecutionRecord {
    #[must_use]
    pub const fn new(thread_id: SyndicThreadId, execution: ExecutionBinding) -> Self {
        Self {
            thread_id,
            execution,
        }
    }

    #[must_use]
    pub const fn thread_id(&self) -> SyndicThreadId {
        self.thread_id
    }

    #[must_use]
    pub const fn execution(&self) -> &ExecutionBinding {
        &self.execution
    }
}
