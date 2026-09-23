use beryl_backend::{
    ApprovalOperationCompletion, OrderedTurnStreamCompletion, OrderedTurnStreamOperation,
    OrderedTurnStreamSubmitError,
};
use beryl_home_store::{HomeGeneration, HomeHealthState, HomeServiceReference};
use beryl_model::BerylHomeId;
use std::sync::{
    Arc, Weak,
    atomic::{AtomicBool, Ordering},
};

pub(in crate::cas_projection::connection) struct PassiveApprovalFence {
    home: Weak<HomeServiceReference>,
    home_id: BerylHomeId,
    generation: HomeGeneration,
    failed: AtomicBool,
}

impl PassiveApprovalFence {
    pub(super) fn new(
        home: &Arc<HomeServiceReference>,
        home_id: BerylHomeId,
        generation: HomeGeneration,
    ) -> Self {
        Self {
            home: Arc::downgrade(home),
            home_id,
            generation,
            failed: AtomicBool::new(false),
        }
    }

    pub(in crate::cas_projection::connection) fn observe(&self) -> bool {
        if self.failed.load(Ordering::Acquire) {
            return true;
        }
        if let Some(home) = self.home.upgrade() {
            let health = home.health();
            if home.home_id() == self.home_id
                && health.generation() == Some(self.generation)
                && health.state() == HomeHealthState::Failed
            {
                self.failed.store(true, Ordering::Release);
                return true;
            }
        }
        false
    }

    pub(in crate::cas_projection::connection) fn dispose(
        &self,
        result: Result<OrderedTurnStreamCompletion, OrderedTurnStreamSubmitError>,
    ) -> Result<OrderedTurnStreamCompletion, OrderedTurnStreamSubmitError> {
        match result {
            Err(error) if self.observe() => {
                let (operation, cause) = error.into_parts();
                match operation {
                    OrderedTurnStreamOperation::Approval(request) => {
                        Ok(OrderedTurnStreamCompletion::Approval(
                            ApprovalOperationCompletion::Unanswered { request },
                        ))
                    }
                    operation => Err(OrderedTurnStreamSubmitError::new(operation, cause)),
                }
            }
            other => other,
        }
    }
}
