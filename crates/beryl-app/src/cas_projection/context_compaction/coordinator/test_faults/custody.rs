use super::*;

#[derive(Clone, Copy)]
pub enum CompactionCustodyTestStage {
    LifecyclePreparation,
    AdmissionReady,
    AdmissionFailed,
}

#[derive(Default)]
pub(in crate::cas_projection::context_compaction::coordinator) struct CompactionCustodyPauses {
    lifecycle_preparation: Option<Arc<CustodyPause>>,
    ready: Option<Arc<CustodyPause>>,
    failed: Option<Arc<CustodyPause>>,
}

struct CustodyPause {
    gate: LifecycleStagingPause,
    unwind: AtomicBool,
}

pub struct CompactionCustodyPauseController(Arc<CustodyPause>);

impl CompactionCustodyPauseController {
    pub fn wait_until_paused(&self) {
        let deadline = Instant::now() + TEST_WAIT_LIMIT;
        while !self.0.gate.arrived.load(Ordering::Acquire) {
            assert!(
                Instant::now() < deadline,
                "compaction custody pause timed out"
            );
            std::thread::yield_now();
        }
    }

    pub fn release(&self) {
        self.0.gate.release();
    }

    pub fn unwind(&self) {
        self.0.unwind.store(true, Ordering::Release);
        self.release();
    }
}

impl Drop for CompactionCustodyPauseController {
    fn drop(&mut self) {
        self.release();
    }
}

pub struct CompactionCustodyPressureGuard {
    _reservations: Vec<super::super::custody::CompactionCustodyReservation>,
    pool: Arc<CompactionCustodyPool>,
}

impl CompactionCustodyPressureGuard {
    pub fn in_use(&self) -> usize {
        self.pool.in_use()
    }
}

impl ContextCompactionLifecycleTestHarness {
    pub fn pause_compaction_custody(
        &self,
        stage: CompactionCustodyTestStage,
    ) -> CompactionCustodyPauseController {
        let coordinator = self.coordinator().unwrap();
        let pause = Arc::new(CustodyPause {
            gate: LifecycleStagingPause::new(),
            unwind: AtomicBool::new(false),
        });
        let mut pauses = coordinator.custody_pauses.lock().unwrap();
        let slot = match stage {
            CompactionCustodyTestStage::LifecyclePreparation => &mut pauses.lifecycle_preparation,
            CompactionCustodyTestStage::AdmissionReady => &mut pauses.ready,
            CompactionCustodyTestStage::AdmissionFailed => &mut pauses.failed,
        };
        assert!(slot.is_none());
        *slot = Some(Arc::clone(&pause));
        CompactionCustodyPauseController(pause)
    }

    pub fn occupy_compaction_custody(&self, count: usize) -> CompactionCustodyPressureGuard {
        assert!(count <= COMPACTION_QUEUE_CAPACITY + COMPACTION_WORKER_CAPACITY);
        let coordinator = self.coordinator().unwrap();
        CompactionCustodyPressureGuard {
            pool: Arc::clone(&coordinator.custody),
            _reservations: (0..count)
                .map(|_| coordinator.custody.reserve().unwrap())
                .collect(),
        }
    }

    pub fn compaction_custody_in_use(&self) -> usize {
        self.coordinator().unwrap().custody.in_use()
    }

    pub fn has_local_compaction(&self, thread: SyndicThreadId) -> bool {
        self.coordinator()
            .unwrap()
            .operations
            .lock()
            .unwrap()
            .contains_key(&thread)
    }

    pub fn retain_compaction_result(
        &self,
        thread: SyndicThreadId,
    ) -> ContextCompactionWaitTestHarness {
        let coordinator = self.coordinator().unwrap();
        let operations = coordinator.operations.lock().unwrap();
        ContextCompactionWaitTestHarness(Arc::clone(operations.get(&thread).unwrap()))
    }
}

impl ContextCompactionCoordinator {
    pub(in crate::cas_projection::context_compaction::coordinator) fn pause_compaction_custody(
        &self,
        stage: CompactionCustodyTestStage,
    ) {
        let pause = {
            let mut pauses = self.custody_pauses.lock().unwrap();
            match stage {
                CompactionCustodyTestStage::LifecyclePreparation => {
                    pauses.lifecycle_preparation.take()
                }
                CompactionCustodyTestStage::AdmissionReady => pauses.ready.take(),
                CompactionCustodyTestStage::AdmissionFailed => pauses.failed.take(),
            }
        };
        if let Some(pause) = pause {
            pause.gate.wait(&self.closing);
            assert!(
                !pause.unwind.load(Ordering::Acquire),
                "compaction custody test unwind"
            );
        }
    }
}
