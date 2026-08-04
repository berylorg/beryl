use super::*;

const TEST_WAIT_LIMIT: Duration = Duration::from_secs(5);

pub(super) struct LifecycleStagingPause {
    arrived: AtomicBool,
    released: AtomicBool,
    changed: Condvar,
    state: Mutex<()>,
}

impl LifecycleStagingPause {
    fn new() -> Self {
        Self {
            arrived: AtomicBool::new(false),
            released: AtomicBool::new(false),
            changed: Condvar::new(),
            state: Mutex::new(()),
        }
    }

    fn wait(&self, closing: &AtomicBool) {
        self.arrived.store(true, Ordering::Release);
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        while !self.released.load(Ordering::Acquire) && !closing.load(Ordering::Acquire) {
            state = self
                .changed
                .wait_timeout(state, Duration::from_millis(10))
                .unwrap_or_else(|poison| poison.into_inner())
                .0;
        }
    }

    fn release(&self) {
        self.released.store(true, Ordering::Release);
        self.changed.notify_all();
    }
}

#[doc(hidden)]
pub struct ContextCompactionStagingPauseController {
    gate: Arc<LifecycleStagingPause>,
}

impl ContextCompactionStagingPauseController {
    pub fn wait_until_staged(&self) {
        let deadline = Instant::now() + TEST_WAIT_LIMIT;
        while !self.gate.arrived.load(Ordering::Acquire) {
            assert!(
                Instant::now() < deadline,
                "context compaction did not reach the post-staging pause"
            );
            std::thread::yield_now();
        }
    }

    pub fn release(&self) {
        self.gate.release();
    }
}

impl Drop for ContextCompactionStagingPauseController {
    fn drop(&mut self) {
        self.gate.release();
    }
}

#[derive(Clone)]
#[doc(hidden)]
pub struct ContextCompactionLifecycleTestHarness(Weak<ContextCompactionCoordinator>);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[doc(hidden)]
pub enum ContextCompactionTerminalResponseTestOutcome {
    AwaitRouterTerminal,
    RetireConnection,
    InvariantFailure,
}

impl ContextCompactionLifecycleTestHarness {
    pub(in crate::cas_projection) fn new(coordinator: &Arc<ContextCompactionCoordinator>) -> Self {
        Self(Arc::downgrade(coordinator))
    }

    fn coordinator(&self) -> Result<Arc<ContextCompactionCoordinator>, ContextCompactionError> {
        self.0.upgrade().ok_or(ContextCompactionError::Unavailable)
    }

    pub fn mount_lifecycle_operation(
        &self,
        operation_id: CompactionOperationId,
        attempt: CompactionAttemptNonce,
        yielding_turn_id: SyndicTurnId,
        completion_timeout: Duration,
    ) -> Result<(), ContextCompactionError> {
        validate_completion_timeout(completion_timeout)?;
        let coordinator = self.coordinator()?;
        let operation = coordinator.read_operation(operation_id)?;
        if operation.attempt() != attempt || !operation.state().is_live() {
            return Err(ContextCompactionError::AuthorityMismatch);
        }
        coordinator.install_local(Arc::new(LocalCompaction::new(
            operation_id,
            attempt,
            CompactionOrigin::Lifecycle { yielding_turn_id },
            completion_timeout,
            coordinator
                .commands
                .authorize()
                .map_err(|_| ContextCompactionError::Unavailable)?,
        )))
    }

    pub fn publish_provider_event(
        &self,
        operation_id: CompactionOperationId,
        event: CompactionProviderEvent,
        observed_at: SyndicTimestamp,
    ) -> Result<(), ContextCompactionError> {
        let coordinator = self.coordinator()?;
        let operation = coordinator.read_operation(operation_id)?;
        coordinator.publish_provider_event(
            ContextCompactionTargetAuthority::new(operation_id, operation.target().turn_id()),
            event,
            observed_at,
        )
    }

    pub fn abandon_target_loss(
        &self,
        operation_id: CompactionOperationId,
    ) -> Result<(), ContextCompactionError> {
        let coordinator = self.coordinator()?;
        let operation = coordinator.read_operation(operation_id)?;
        coordinator.abandon_target_loss(ContextCompactionTargetAuthority::new(
            operation_id,
            operation.target().turn_id(),
        ))
    }

    pub fn reconcile_settled_response(
        &self,
        operation_id: CompactionOperationId,
        response_attempt: CompactionAttemptNonce,
        disposition: CompactionRequestDisposition,
        unbind_failed: bool,
    ) -> Result<ContextCompactionTerminalResponseTestOutcome, ContextCompactionError> {
        let coordinator = self.coordinator()?;
        let operation = coordinator.read_operation(operation_id)?;
        let local = LocalCompaction::new(
            operation_id,
            operation.attempt(),
            CompactionOrigin::Manual,
            Duration::from_secs(30),
            coordinator
                .commands
                .authorize()
                .map_err(|_| ContextCompactionError::Unavailable)?,
        );
        let observation = coordinator
            .observe_request(&local, disposition)
            .unwrap_or(CompactionRequestTransitionStatus::Collision);
        let response = match disposition {
            CompactionRequestDisposition::Accepted => {
                dispatch::TerminalResponseDisposition::Accepted
            }
            CompactionRequestDisposition::CompletionUnknown => {
                dispatch::TerminalResponseDisposition::CompletionUnknown
            }
            CompactionRequestDisposition::RejectedBeforeCore => {
                dispatch::TerminalResponseDisposition::Rejected
            }
            CompactionRequestDisposition::ProvenLocalNondispatch => {
                dispatch::TerminalResponseDisposition::ProvenNondispatch
            }
        };
        Ok(
            match dispatch::terminal_response_reconciliation(
                response_attempt == operation.attempt(),
                response,
                observation,
                unbind_failed,
            ) {
                dispatch::TerminalResponseReconciliation::AwaitRouterTerminal => {
                    ContextCompactionTerminalResponseTestOutcome::AwaitRouterTerminal
                }
                dispatch::TerminalResponseReconciliation::RetireConnection => {
                    ContextCompactionTerminalResponseTestOutcome::RetireConnection
                }
                dispatch::TerminalResponseReconciliation::InvariantFailure => {
                    ContextCompactionTerminalResponseTestOutcome::InvariantFailure
                }
            },
        )
    }

    pub fn fail_next_lifecycle_staging(&self) -> Result<(), ContextCompactionError> {
        let coordinator = self.coordinator()?;
        coordinator
            .fail_next_lifecycle_staging
            .store(true, Ordering::Release);
        Ok(())
    }

    pub fn pause_after_lifecycle_staging(
        &self,
    ) -> Result<ContextCompactionStagingPauseController, ContextCompactionError> {
        let coordinator = self.coordinator()?;
        let gate = Arc::new(LifecycleStagingPause::new());
        let mut pause = coordinator
            .lifecycle_staging_pause
            .lock()
            .map_err(|_| ContextCompactionError::Unavailable)?;
        if pause.is_some() {
            return Err(ContextCompactionError::AuthorityMismatch);
        }
        *pause = Some(Arc::clone(&gate));
        Ok(ContextCompactionStagingPauseController { gate })
    }

    pub fn request_shutdown(&self) -> Result<(), ContextCompactionError> {
        self.coordinator()?.request_shutdown();
        Ok(())
    }

    pub fn shutdown_requested(&self) -> Result<bool, ContextCompactionError> {
        Ok(self.coordinator()?.closing.load(Ordering::Acquire))
    }
}

pub(super) struct TestHoldGate {
    released: AtomicBool,
    changed: Condvar,
    state: Mutex<()>,
}

impl TestHoldGate {
    fn new() -> Self {
        Self {
            released: AtomicBool::new(false),
            changed: Condvar::new(),
            state: Mutex::new(()),
        }
    }

    pub(super) fn wait(&self, closing: &AtomicBool) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        while !self.released.load(Ordering::Acquire) && !closing.load(Ordering::Acquire) {
            state = self
                .changed
                .wait_timeout(state, Duration::from_millis(10))
                .unwrap_or_else(|poison| poison.into_inner())
                .0;
        }
    }

    fn release(&self) {
        self.released.store(true, Ordering::Release);
        self.changed.notify_all();
    }
}

#[doc(hidden)]
pub struct ContextCompactionCapacityTestGuard {
    gate: Arc<TestHoldGate>,
}

impl Drop for ContextCompactionCapacityTestGuard {
    fn drop(&mut self) {
        self.gate.release();
    }
}

#[doc(hidden)]
pub struct ContextCompactionWaitTestHarness(Arc<LocalCompaction>);

impl ContextCompactionWaitTestHarness {
    #[must_use]
    pub fn new(completion_timeout: Duration) -> Self {
        let thread_id = SyndicThreadId::from_bytes([201; 16]);
        Self(Arc::new(LocalCompaction::new(
            CompactionOperationId::new(thread_id, CompactionOperationNonce::from_bytes([202; 16])),
            CompactionAttemptNonce::from_bytes([203; 16]),
            CompactionOrigin::Manual,
            completion_timeout,
            test_live_command(),
        )))
    }

    pub fn mark_accepted(&self) {
        self.0.mark_accepted();
    }

    #[must_use]
    pub fn wait(&self) -> ContextCompactionOutcome {
        self.0.wait()
    }

    pub fn succeed(&self) {
        self.0.complete(ContextCompactionOutcome::Succeeded);
    }
}

fn test_live_command() -> LiveCommandPermit {
    let gate = crate::cas_projection::persistent_failure::MasterCommandGate::new(
        crate::cas_projection::ProjectionServiceGeneration::allocate()
            .expect("test service generation is available"),
        None,
    );
    gate.authorizer()
        .authorize()
        .expect("test live command is admitted")
}

impl ContextCompactionCoordinator {
    pub(in crate::cas_projection) fn lifecycle_test_harness(
        self: &Arc<Self>,
    ) -> ContextCompactionLifecycleTestHarness {
        ContextCompactionLifecycleTestHarness::new(self)
    }

    pub(super) fn fail_lifecycle_staging_for_test(&self) -> bool {
        self.fail_next_lifecycle_staging
            .swap(false, Ordering::AcqRel)
    }

    pub(super) fn pause_after_lifecycle_staging_for_test(&self) {
        let pause = self
            .lifecycle_staging_pause
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .take();
        if let Some(pause) = pause {
            pause.wait(&self.closing);
        }
    }

    pub(in crate::cas_projection) fn stage_lifecycle_content_for_test(
        &self,
    ) -> Result<syndic_storage::ContentReference, ContextCompactionError> {
        self.ensure_lifecycle_content()
            .map_err(|_| ContextCompactionError::Storage)
    }

    pub(in crate::cas_projection) fn saturate_capacity_for_test(
        &self,
    ) -> Result<ContextCompactionCapacityTestGuard, ContextCompactionError> {
        if self.diagnostics().workers_current() != 0 || self.diagnostics().queued_current() != 0 {
            return Err(ContextCompactionError::AuthorityMismatch);
        }
        let gate = Arc::new(TestHoldGate::new());
        for _ in 0..COMPACTION_WORKER_CAPACITY {
            self.try_enqueue_work(CompactionWork::Hold(Arc::clone(&gate)))
                .map_err(|_| ContextCompactionError::Unavailable)?;
        }
        let deadline = Instant::now() + TEST_WAIT_LIMIT;
        while self.workers_current.load(Ordering::Acquire) != COMPACTION_WORKER_CAPACITY {
            if Instant::now() >= deadline {
                gate.release();
                return Err(ContextCompactionError::Unavailable);
            }
            std::thread::yield_now();
        }
        for _ in 0..COMPACTION_QUEUE_CAPACITY {
            self.try_enqueue_work(CompactionWork::Hold(Arc::clone(&gate)))
                .map_err(|_| ContextCompactionError::Unavailable)?;
        }
        Ok(ContextCompactionCapacityTestGuard { gate })
    }

    pub(in crate::cas_projection) fn deny_capacity_probe_for_test(&self) -> bool {
        self.try_enqueue_work(CompactionWork::Probe).is_err()
    }
}
