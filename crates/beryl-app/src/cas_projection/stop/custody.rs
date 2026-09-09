use std::sync::{Arc, Weak};

use syndic_storage::{StopOperationId, StopOperationTarget};

use super::StopCoordinator;
use crate::cas_projection::{
    service_config::ConnectionWorkerRetention,
    stop_work::{PermissionInterruptionWorkFact, PermissionInterruptionWorkStage},
};

pub(super) struct LiveStopCustody {
    pub(super) target: StopOperationTarget,
    pub(super) primary: bool,
    pub(super) driver: bool,
}

pub(super) struct StopCustodyToken {
    coordinator: Weak<StopCoordinator>,
    operation_id: StopOperationId,
    driver: bool,
}

pub(in crate::cas_projection) struct StopDriverCustody {
    _observation: StopCustodyToken,
    _workers: ConnectionWorkerRetention,
}

impl StopCustodyToken {
    pub(super) fn primary(
        coordinator: &Arc<StopCoordinator>,
        operation_id: StopOperationId,
    ) -> Self {
        Self {
            coordinator: Arc::downgrade(coordinator),
            operation_id,
            driver: false,
        }
    }

    pub(super) fn driver(&self, workers: ConnectionWorkerRetention) -> StopDriverCustody {
        if let Some(coordinator) = self.coordinator.upgrade() {
            let mut state = coordinator
                .state
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            let custody = state
                .live_custody
                .get_mut(&self.operation_id)
                .expect("primary custody retains its observation until disposal");
            assert!(
                !custody.driver,
                "one foreground driver owns the stop cleanup interval"
            );
            custody.driver = true;
        }
        StopDriverCustody {
            _observation: Self {
                coordinator: self.coordinator.clone(),
                operation_id: self.operation_id,
                driver: true,
            },
            _workers: workers,
        }
    }
}

impl Drop for StopCustodyToken {
    fn drop(&mut self) {
        let Some(coordinator) = self.coordinator.upgrade() else {
            return;
        };
        let mut state = coordinator
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let Some(custody) = state.live_custody.get_mut(&self.operation_id) else {
            return;
        };
        if self.driver {
            custody.driver = false;
        } else {
            custody.primary = false;
        }
        if !custody.primary && !custody.driver {
            state.live_custody.remove(&self.operation_id);
        }
    }
}

pub(in crate::cas_projection) struct PermissionCustodyToken {
    coordinator: Weak<StopCoordinator>,
    serial: u64,
}

impl StopCoordinator {
    pub(in crate::cas_projection) fn observe_permission(
        self: &Arc<Self>,
        mut fact: PermissionInterruptionWorkFact,
    ) -> Option<PermissionCustodyToken> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let Some(serial) = state.next_permission.checked_add(1) else {
            state.invalidate_revision();
            return None;
        };
        state.next_permission = serial;
        fact.serial = serial;
        state.permissions.insert(serial, fact);
        Some(PermissionCustodyToken {
            coordinator: Arc::downgrade(self),
            serial,
        })
    }
}

impl PermissionCustodyToken {
    pub(in crate::cas_projection) fn prepared(&self, operation_id: StopOperationId) {
        if let Some(coordinator) = self.coordinator.upgrade() {
            let mut state = coordinator
                .state
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            if let Some(fact) = state.permissions.get_mut(&self.serial) {
                fact.operation_id = Some(operation_id);
                fact.stage = PermissionInterruptionWorkStage::Prepared;
            }
        }
    }

    pub(in crate::cas_projection) fn set_stage(&self, stage: PermissionInterruptionWorkStage) {
        if let Some(coordinator) = self.coordinator.upgrade() {
            let mut state = coordinator
                .state
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            if let Some(fact) = state.permissions.get_mut(&self.serial) {
                fact.stage = stage;
            }
        }
    }
}

impl Drop for PermissionCustodyToken {
    fn drop(&mut self) {
        if let Some(coordinator) = self.coordinator.upgrade() {
            #[cfg(test)]
            coordinator.pause_race_if_requested(super::StopRaceStage::PermissionCustodyDrop);
            coordinator
                .state
                .lock()
                .unwrap_or_else(|poison| poison.into_inner())
                .permissions
                .remove(&self.serial);
        }
    }
}
