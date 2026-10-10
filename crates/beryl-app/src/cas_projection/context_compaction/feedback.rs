use std::{
    sync::{Arc, Mutex, Weak},
    time::Instant,
};

use syndic_storage::CompactionOperationId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContextCompactionFeedbackState {
    Pending,
    Waiting,
    StillRunning,
    Rejected,
    Succeeded,
    Failed,
    Interrupted,
    AuthorityLost,
}

impl ContextCompactionFeedbackState {
    pub fn resolved(self) -> bool {
        !matches!(self, Self::Pending | Self::Waiting | Self::StillRunning)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ContextCompactionFeedbackSnapshot {
    pub revision: u64,
    pub state: ContextCompactionFeedbackState,
    pub operation: Option<CompactionOperationId>,
}

#[derive(Clone)]
pub struct ContextCompactionFeedback {
    inner: Arc<FeedbackRecord>,
}

pub(in crate::cas_projection) struct FeedbackRecord {
    _presentation: Arc<()>,
    latest: Mutex<LatestFeedback>,
}

struct LatestFeedback {
    snapshot: ContextCompactionFeedbackSnapshot,
    deadline: Option<Instant>,
}

impl PartialEq for ContextCompactionFeedback {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }
}

impl Eq for ContextCompactionFeedback {}

impl std::fmt::Debug for ContextCompactionFeedback {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ContextCompactionFeedback")
            .field("latest", &self.snapshot())
            .finish()
    }
}

impl ContextCompactionFeedback {
    pub(in crate::cas_projection) fn from_destination(
        destination: &Weak<FeedbackRecord>,
    ) -> Option<Self> {
        Some(Self {
            inner: destination.upgrade()?,
        })
    }
    pub(in crate::cas_projection) fn new(presentation: Arc<()>) -> Self {
        Self {
            inner: Arc::new(FeedbackRecord {
                _presentation: presentation,
                latest: Mutex::new(LatestFeedback {
                    snapshot: ContextCompactionFeedbackSnapshot {
                        revision: 0,
                        state: ContextCompactionFeedbackState::Pending,
                        operation: None,
                    },
                    deadline: None,
                }),
            }),
        }
    }

    pub fn snapshot(&self) -> ContextCompactionFeedbackSnapshot {
        let mut latest = self.inner.latest.lock().unwrap_or_else(|p| p.into_inner());
        if latest.snapshot.state == ContextCompactionFeedbackState::Waiting
            && latest
                .deadline
                .is_some_and(|deadline| Instant::now() >= deadline)
        {
            latest.transition(ContextCompactionFeedbackState::StillRunning);
        }
        latest.snapshot
    }

    pub(in crate::cas_projection) fn destination(&self) -> Weak<FeedbackRecord> {
        Arc::downgrade(&self.inner)
    }

    pub(in crate::cas_projection) fn settle(&self, state: ContextCompactionFeedbackState) {
        self.inner.settle(state);
    }
}

impl FeedbackRecord {
    pub(in crate::cas_projection) fn settle_execution(
        &self,
        state: ContextCompactionFeedbackState,
    ) {
        let mut latest = self.latest.lock().unwrap_or_else(|p| p.into_inner());
        if latest.snapshot.state != ContextCompactionFeedbackState::Pending {
            latest.transition(state);
        }
    }
    pub(in crate::cas_projection) fn admitted(
        &self,
        operation: CompactionOperationId,
        deadline: Instant,
    ) {
        let mut latest = self.latest.lock().unwrap_or_else(|p| p.into_inner());
        if latest.snapshot.state == ContextCompactionFeedbackState::Pending {
            latest.snapshot.operation = Some(operation);
            latest.deadline = Some(deadline);
            latest.transition(ContextCompactionFeedbackState::Waiting);
        }
    }

    pub(in crate::cas_projection) fn settle(&self, state: ContextCompactionFeedbackState) {
        assert!(state.resolved());
        self.latest
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .transition(state);
    }
}

impl LatestFeedback {
    fn transition(&mut self, state: ContextCompactionFeedbackState) {
        if self.snapshot.state.resolved() || self.snapshot.state == state {
            return;
        }
        self.snapshot.state = state;
        self.snapshot.revision = self
            .snapshot
            .revision
            .checked_add(1)
            .expect("bounded compaction feedback transitions");
    }
}
