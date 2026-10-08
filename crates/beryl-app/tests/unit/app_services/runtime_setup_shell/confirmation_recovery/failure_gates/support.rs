pub(super) struct BlockedHomeCommand {
    pause: beryl_home_store::test_faults::FaultBlock,
    worker: Option<std::thread::JoinHandle<beryl_home_store::CommandOutcome>>,
}

impl BlockedHomeCommand {
    pub(super) fn new(
        pause: beryl_home_store::test_faults::FaultBlock,
        worker: std::thread::JoinHandle<beryl_home_store::CommandOutcome>,
    ) -> Self {
        Self {
            pause,
            worker: Some(worker),
        }
    }

    pub(super) fn reached(&self) -> bool {
        self.pause
            .wait_until_reached(std::time::Duration::from_secs(10))
    }

    pub(super) fn release_and_join(
        &mut self,
    ) -> std::thread::Result<beryl_home_store::CommandOutcome> {
        self.pause.release();
        self.worker.take().unwrap().join()
    }
}

impl Drop for BlockedHomeCommand {
    fn drop(&mut self) {
        self.pause.release();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
