use std::sync::{Arc, Mutex, Weak};

pub(super) const OPERATION_FEEDBACK_CAPACITY: usize = 72;

#[derive(Default)]
pub(super) struct OperationFeedbackBudget {
    leases: Mutex<Vec<Weak<()>>>,
}

impl OperationFeedbackBudget {
    pub(super) fn reserve(&self) -> Option<Arc<()>> {
        let mut leases = self.leases.lock().ok()?;
        leases.retain(|lease| lease.strong_count() != 0);
        if leases.len() == OPERATION_FEEDBACK_CAPACITY {
            return None;
        }
        let lease = Arc::new(());
        leases.push(Arc::downgrade(&lease));
        Some(lease)
    }
}
