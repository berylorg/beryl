use super::*;

impl ContextCompactionCoordinator {
    pub(super) fn existing_local(
        &self,
        operation: &CompactionOperationRecord,
    ) -> Result<Arc<LocalCompaction>, ContextCompactionError> {
        let operations = self
            .operations
            .lock()
            .map_err(|_| ContextCompactionError::Unavailable)?;
        operations
            .get(&operation.id().thread_id())
            .filter(|local| {
                local.operation_id == operation.id() && local.attempt == operation.attempt()
            })
            .cloned()
            .ok_or(ContextCompactionError::AuthorityMismatch)
    }

    pub(super) fn admit_manual(
        self: &Arc<Self>,
        candidate: &syndic_storage::CompactionAdmissionCandidate,
        completion_timeout: Duration,
        command: LiveCommandPermit,
    ) -> Result<Arc<LocalCompaction>, ContextCompactionError> {
        let operation_nonce = random_operation_nonce()?;
        let attempt = random_attempt_nonce()?;
        let (connection, projection) = self.projection_for(candidate)?;
        let admission = candidate.admission(
            operation_nonce,
            attempt,
            projection.loaded_session_generation(),
            timestamp_now()?,
        );
        let operation_id = admission.operation_id();
        let local = Arc::new(LocalCompaction::new(
            operation_id,
            attempt,
            CompactionOrigin::Manual,
            completion_timeout,
            command,
        ));
        self.install_local(Arc::clone(&local))?;
        require_committed_command(
            self.home.execute_current(
                self.storage
                    .current_admit_compaction_operation(admission.clone()),
            ),
        )?;
        let operation = match self.read_operation(operation_id) {
            Ok(operation) => operation,
            Err(error) => {
                self.fail_local(&local);
                return Err(error);
            }
        };
        if operation.target() != admission.target()
            || operation.attempt() != attempt
            || operation.state() != &CompactionOperationState::Admitted
        {
            let _ = self.settle(&local, CompactionSettlement::CancelledBeforeDispatch);
            self.fail_local(&local);
            return Err(ContextCompactionError::Storage);
        }
        let authority =
            ContextCompactionTargetAuthority::new(operation_id, operation.target().turn_id());
        let target = match projection.into_context_compaction_live_event_target(authority) {
            Ok(target) => target,
            Err(_) => {
                let _ = self.settle(&local, CompactionSettlement::CancelledBeforeDispatch);
                self.fail_local(&local);
                return Err(ContextCompactionError::AuthorityMismatch);
            }
        };
        debug_assert_eq!(connection.runtime_id(), operation.target().runtime_id());
        if let Err(work) = self.enqueue(Arc::clone(&local), target) {
            let _ = self.settle(&local, CompactionSettlement::CancelledBeforeDispatch);
            self.fail_local(&local);
            if let Some(mut target) = work.into_nondispatch_target() {
                let _ = target.into_context_compaction_nondispatch_projection();
            }
            return Err(ContextCompactionError::Unavailable);
        }
        Ok(local)
    }

    pub(super) fn admit_lifecycle(
        self: &Arc<Self>,
        projection: LoadedCasProjection,
        candidate: &syndic_storage::CompactionAdmissionCandidate,
        yielding_turn_id: SyndicTurnId,
        completion_timeout: Duration,
        command: LiveCommandPermit,
    ) -> Result<(), ContextCompactionError> {
        let operation_nonce = random_operation_nonce()?;
        let attempt = random_attempt_nonce()?;
        let admission = candidate.admission(
            operation_nonce,
            attempt,
            projection.loaded_session_generation(),
            timestamp_now()?,
        );
        let operation_id = admission.operation_id();
        let local = Arc::new(LocalCompaction::new(
            operation_id,
            attempt,
            CompactionOrigin::Lifecycle { yielding_turn_id },
            completion_timeout,
            command,
        ));
        self.install_local(Arc::clone(&local))?;
        require_committed_command(
            self.home.execute_current(
                self.storage
                    .current_admit_compaction_operation(admission.clone()),
            ),
        )?;
        let operation = match self.read_operation(operation_id) {
            Ok(operation) => operation,
            Err(error) => {
                self.fail_local(&local);
                return Err(error);
            }
        };
        if operation.target() != admission.target()
            || operation.attempt() != attempt
            || operation.state() != &CompactionOperationState::Admitted
        {
            let _ = self.settle(&local, CompactionSettlement::CancelledBeforeDispatch);
            self.fail_local(&local);
            return Err(ContextCompactionError::Storage);
        }
        let authority =
            ContextCompactionTargetAuthority::new(operation_id, operation.target().turn_id());
        let target = match projection.into_context_compaction_live_event_target(authority) {
            Ok(target) => target,
            Err(_) => {
                let _ = self.settle(&local, CompactionSettlement::CancelledBeforeDispatch);
                self.fail_local(&local);
                return Err(ContextCompactionError::AuthorityMismatch);
            }
        };
        if let Err(work) = self.enqueue(Arc::clone(&local), target) {
            let _ = self.settle(&local, CompactionSettlement::CancelledBeforeDispatch);
            self.fail_local(&local);
            if let Some(mut target) = work.into_nondispatch_target() {
                let _ = target.into_context_compaction_nondispatch_projection();
            }
            return Err(ContextCompactionError::Unavailable);
        }
        Ok(())
    }

    fn enqueue(
        &self,
        local: Arc<LocalCompaction>,
        target: LiveEventTarget,
    ) -> Result<(), CompactionWork> {
        self.try_enqueue_work(CompactionWork::Operation { local, target })
    }

    pub(super) fn try_enqueue_work(&self, work: CompactionWork) -> Result<(), CompactionWork> {
        if self.closing.load(Ordering::Acquire) {
            increment_bounded(&self.denied_admissions);
            return Err(work);
        }
        let sender = match self.work.lock() {
            Ok(sender) => sender,
            Err(_) => {
                increment_bounded(&self.denied_admissions);
                return Err(work);
            }
        };
        let Some(sender) = sender.as_ref() else {
            increment_bounded(&self.denied_admissions);
            return Err(work);
        };
        let queued = self.queued_current.fetch_add(1, Ordering::AcqRel) + 1;
        update_high_water(&self.queued_high_water, queued);
        match sender.try_send(work) {
            Ok(()) => Ok(()),
            Err(mpsc::TrySendError::Full(work) | mpsc::TrySendError::Disconnected(work)) => {
                self.queued_current.fetch_sub(1, Ordering::AcqRel);
                increment_bounded(&self.denied_admissions);
                Err(work)
            }
        }
    }

    fn projection_for(
        &self,
        candidate: &syndic_storage::CompactionAdmissionCandidate,
    ) -> Result<(Arc<ProjectionConnection>, LoadedCasProjection), ContextCompactionError> {
        let connections = {
            let mut registry = self
                .connections
                .lock()
                .map_err(|_| ContextCompactionError::Unavailable)?;
            registry.retain(|connection| !connection.is_detached());
            registry.clone()
        };
        let mut exact = None;
        for connection in connections {
            if connection.runtime_id() != candidate.runtime_id() {
                continue;
            }
            match connection
                .acquire_existing(
                    candidate.cas_thread_id(),
                    candidate.thread_id(),
                    COMPACTION_REQUEST_TIMEOUT,
                )
                .map_err(|_| ContextCompactionError::Driver)?
            {
                ExistingLease::Exact(lease) if exact.is_none() => {
                    exact = Some((connection, lease));
                }
                ExistingLease::Exact(_) => return Err(ContextCompactionError::AuthorityMismatch),
                ExistingLease::Absent
                | ExistingLease::AnotherConnection
                | ExistingLease::AnotherOwner { .. } => {}
            }
        }
        let (connection, lease) = exact.ok_or(ContextCompactionError::Unavailable)?;
        let current = self
            .storage
            .current_binding(&self.home, candidate.thread_id(), point_limit())
            .map_err(|_| ContextCompactionError::Storage)?
            .ok_or(ContextCompactionError::AuthorityMismatch)?;
        let BindingState::Valid(usable) = current.binding().state() else {
            return Err(ContextCompactionError::AuthorityMismatch);
        };
        if current.binding().revision() != candidate.binding_revision()
            || usable.cas_thread_id() != candidate.cas_thread_id()
            || usable.execution().runtime_id() != candidate.runtime_id()
            || usable.represented_prefix() != candidate.represented_prefix()
        {
            return Err(ContextCompactionError::AuthorityMismatch);
        }
        let coordinator = CasProjectionCoordinator::for_healthy_home(&self.home)
            .map_err(|_| ContextCompactionError::Unavailable)?;
        let projection = LoadedCasProjection::new(
            &coordinator,
            candidate.thread_id(),
            candidate.binding_revision(),
            usable.execution().clone(),
            candidate.cas_thread_id().clone(),
            lease,
            usable.lineage(),
        );
        Ok((connection, projection))
    }

    pub(super) fn install_local(
        &self,
        local: Arc<LocalCompaction>,
    ) -> Result<(), ContextCompactionError> {
        let mut operations = self
            .operations
            .lock()
            .map_err(|_| ContextCompactionError::Unavailable)?;
        if operations
            .get(&local.operation_id.thread_id())
            .is_some_and(|existing| !existing.is_finished())
        {
            return Err(ContextCompactionError::AuthorityMismatch);
        }
        operations.insert(local.operation_id.thread_id(), local);
        Ok(())
    }

    pub(super) fn remove_local(&self, local: &LocalCompaction) {
        if let Ok(mut operations) = self.operations.lock()
            && operations
                .get(&local.operation_id.thread_id())
                .is_some_and(|current| current.operation_id == local.operation_id)
        {
            operations.remove(&local.operation_id.thread_id());
        }
    }

    pub(super) fn read_operation(
        &self,
        operation_id: CompactionOperationId,
    ) -> Result<CompactionOperationRecord, ContextCompactionError> {
        self.storage
            .compaction_operation(&self.home, operation_id, point_limit())
            .map_err(|_| ContextCompactionError::Storage)?
            .ok_or(ContextCompactionError::AuthorityMismatch)
    }

    pub(super) fn ensure_current(&self) -> Result<(), ContextCompactionError> {
        let health = self.home.health();
        if self.closing.load(Ordering::Acquire)
            || self.home.home_id() != self.home_id
            || health.state() != beryl_home_store::HomeHealthState::Healthy
            || health.generation() != Some(self.home_generation)
        {
            return Err(ContextCompactionError::Unavailable);
        }
        Ok(())
    }
}
