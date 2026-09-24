use super::*;
use crate::cas_projection::initial_start::InitialStartOwner;

#[derive(Default)]
pub(super) struct Probe {
    pub passes: u64,
    pub capacity_waits: u64,
    pub maximum_ready: usize,
    pause: Option<(JobId, bool)>,
    paused: bool,
}

impl HandoffSignal {
    pub(super) fn test_update(&self, update: impl FnOnce(&mut Probe)) {
        update(&mut self.state.lock().unwrap().probe);
    }
    pub(super) fn test_checkpoint(&self, job: JobId, selection: bool) {
        let mut state = self.state.lock().unwrap();
        if state.probe.pause == Some((job, selection)) {
            state.probe.paused = true;
            self.changed.notify_all();
            while state.probe.pause.is_some() && !state.stopped {
                state = self.changed.wait(state).unwrap();
            }
            state.probe.paused = false;
        }
    }
}

pub struct HandoffCapacityTestGuard {
    _flight: super::super::flight::Flight,
}

pub struct HandoffCoordinatorTestHarness {
    coordinator: HandoffCoordinator,
    start: Option<InitialStartOwner>,
    _observer: beryl_home_store::HomeMutationObserver,
}

impl HandoffCoordinatorTestHarness {
    pub fn pause_at(&self, job: JobId, before_preparation: bool) {
        self.coordinator
            .signal
            .test_update(|probe| probe.pause = Some((job, before_preparation)));
    }
    pub fn paused(&self) -> bool {
        self.coordinator.signal.state.lock().unwrap().probe.paused
    }
    pub fn resume(&self) {
        self.coordinator
            .signal
            .test_update(|probe| probe.pause = None);
        self.coordinator.signal.changed.notify_all();
    }
    pub fn progress(&self) -> (u64, u64, usize) {
        let state = self.coordinator.signal.state.lock().unwrap();
        (
            state.probe.passes,
            state.probe.capacity_waits,
            state.probe.maximum_ready,
        )
    }
    pub fn occupy_slot(&self, job: JobId) -> HandoffCapacityTestGuard {
        HandoffCapacityTestGuard {
            _flight: self.coordinator.operations.acquire(job).unwrap(),
        }
    }
    pub fn prepare(
        service: DiscussionSettlementService,
        limits: HandoffScanLimits,
    ) -> Result<Self, String> {
        let start = InitialStartOwner::new();
        let store = service.store.clone();
        let coordinator = HandoffCoordinator::prepare(service, limits, start.gate())
            .map_err(|e| e.to_string())?;
        let observer = store
            .observe_mutations(coordinator.waker())
            .map_err(|e| e.to_string())?;
        Ok(Self {
            coordinator,
            start: Some(start),
            _observer: observer,
        })
    }

    pub fn release(&mut self) {
        assert!(self.start.take().unwrap().release());
    }

    pub fn wake(&self) {
        self.coordinator.waker().wake();
    }

    pub fn shutdown(&mut self) -> Result<(), String> {
        self.coordinator.shutdown().map_err(|e| e.to_string())
    }
}
