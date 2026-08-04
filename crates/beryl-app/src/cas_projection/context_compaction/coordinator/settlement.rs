use super::*;

pub(super) enum LifecycleContentFailure {
    DefinitivePreparation,
    Home,
}

impl ContextCompactionCoordinator {
    pub(in crate::cas_projection) fn publish_provider_event(
        &self,
        authority: ContextCompactionTargetAuthority,
        event: CompactionProviderEvent,
        observed_at: SyndicTimestamp,
    ) -> Result<(), ContextCompactionError> {
        let local = self.local_for(authority.operation_id())?;
        self.ensure_local_current(&local)?;
        let _mutation = local
            .mutation
            .lock()
            .map_err(|_| ContextCompactionError::AuthorityMismatch)?;
        let before = self.read_operation(authority.operation_id())?;
        if before.target().turn_id() != authority.provider_turn_id()
            || !before.state().is_live() && before.state() != &CompactionOperationState::Finalizing
        {
            return Err(ContextCompactionError::AuthorityMismatch);
        }
        let sequence = before.provider_frontier().map_or(
            Ok(CompactionProviderSequence::FIRST),
            |frontier| {
                frontier
                    .checked_next()
                    .map_err(|_| ContextCompactionError::Storage)
            },
        )?;
        let terminal = matches!(event, CompactionProviderEvent::Terminal(_));
        let request = PublishCompactionProviderEvent::new(
            authority.operation_id(),
            before.revision(),
            sequence,
            event,
            observed_at,
        );
        let _ = self.home.execute_current(
            self.storage
                .current_publish_compaction_provider_event(request),
        );
        let after = self.read_operation(authority.operation_id())?;
        if after.provider_frontier() != Some(sequence) {
            return Err(ContextCompactionError::Storage);
        }
        if terminal {
            self.finalize_terminal_locked(&local, &after)?;
        }
        Ok(())
    }

    pub(in crate::cas_projection) fn abandon_target_loss(
        &self,
        authority: ContextCompactionTargetAuthority,
    ) -> Result<(), ContextCompactionError> {
        let local = self.local_for(authority.operation_id())?;
        self.ensure_local_current(&local)?;
        let operation = self.read_operation(authority.operation_id())?;
        if operation.target().turn_id() != authority.provider_turn_id() {
            return Err(ContextCompactionError::AuthorityMismatch);
        }
        self.abandon(&local, CompactionAbandonmentReason::TargetAuthorityLost)?;
        self.fail_local(&local);
        Ok(())
    }

    fn local_for(
        &self,
        operation_id: CompactionOperationId,
    ) -> Result<Arc<LocalCompaction>, ContextCompactionError> {
        let operations = self
            .operations
            .lock()
            .map_err(|_| ContextCompactionError::Unavailable)?;
        operations
            .get(&operation_id.thread_id())
            .filter(|local| local.operation_id == operation_id)
            .cloned()
            .ok_or(ContextCompactionError::AuthorityMismatch)
    }

    pub(super) fn finalize_terminal_locked(
        &self,
        local: &LocalCompaction,
        operation: &CompactionOperationRecord,
    ) -> Result<(), ContextCompactionError> {
        self.ensure_local_current(local)?;
        let result = self.finalize_terminal_inner(local, operation);
        if result.is_err() {
            self.fail_local(local);
        }
        result
    }

    fn finalize_terminal_inner(
        &self,
        local: &LocalCompaction,
        operation: &CompactionOperationRecord,
    ) -> Result<(), ContextCompactionError> {
        let successful = operation.terminal().is_some_and(|terminal| {
            terminal.status().outcome() == syndic_storage::TurnTerminalOutcome::Complete
        }) && operation
            .marker()
            .is_some_and(|marker| marker.lifecycle() == CompactionMarkerLifecycle::Completed);
        if matches!(local.origin, CompactionOrigin::Lifecycle { .. }) && successful {
            let content = match self.ensure_lifecycle_content() {
                Ok(content) => content,
                Err(LifecycleContentFailure::DefinitivePreparation) => {
                    increment_bounded(&self.lifecycle_continuation_failures);
                    let _fence = self
                        .settlement_fence
                        .lock()
                        .map_err(|_| ContextCompactionError::Unavailable)?;
                    self.cancel_lifecycle_intent(local);
                    self.execute_manual_settlement(operation, CompactionSettlement::ManualSuccess);
                    return self.finish_settlement(local, successful);
                }
                Err(LifecycleContentFailure::Home) => {
                    self.cancel_lifecycle_intent(local);
                    return Err(ContextCompactionError::Storage);
                }
            };
            #[cfg(feature = "test-faults")]
            self.pause_after_lifecycle_staging_for_test();
            let settled_at = match timestamp_now() {
                Ok(settled_at) => settled_at,
                Err(error) => {
                    self.cancel_lifecycle_intent(local);
                    return Err(error);
                }
            };
            let _fence = self
                .settlement_fence
                .lock()
                .map_err(|_| ContextCompactionError::Unavailable)?;
            let phase_continue = if self.closing.load(Ordering::Acquire) {
                self.cancel_lifecycle_intent(local);
                false
            } else {
                self.stop
                    .take_terminal_lifecycle_yield(
                        local.operation_id.thread_id(),
                        local
                            .yielding_turn_id()
                            .ok_or(ContextCompactionError::AuthorityMismatch)?,
                    )
                    .map_err(|_| ContextCompactionError::Unavailable)?
                    .is_some_and(|outcome| outcome == crate::LifecycleYieldOutcome::PhaseContinue)
            };
            if phase_continue {
                let _ =
                    self.home
                        .execute_current(self.storage.current_settle_lifecycle_compaction(
                            SettleLifecycleCompaction::new(operation, content, settled_at),
                        ));
            } else {
                self.execute_manual_settlement(operation, CompactionSettlement::ManualSuccess);
            }
        } else {
            let settlement = match (local.origin, successful) {
                (CompactionOrigin::Manual, true) => CompactionSettlement::ManualSuccess,
                (CompactionOrigin::Manual, false) | (CompactionOrigin::Lifecycle { .. }, false) => {
                    CompactionSettlement::ManualFailure
                }
                (CompactionOrigin::Lifecycle { .. }, true) => unreachable!(),
            };
            self.execute_manual_settlement(operation, settlement);
        }
        self.finish_settlement(local, successful)
    }

    fn execute_manual_settlement(
        &self,
        operation: &CompactionOperationRecord,
        settlement: CompactionSettlement,
    ) {
        let _ = self
            .home
            .execute_current(self.storage.current_settle_compaction_operation(
                SettleCompactionOperation::new(operation.id(), operation.revision(), settlement),
            ));
    }

    fn finish_settlement(
        &self,
        local: &LocalCompaction,
        successful: bool,
    ) -> Result<(), ContextCompactionError> {
        let settled = self.read_operation(local.operation_id)?;
        if !matches!(settled.state(), CompactionOperationState::Consumed(_)) {
            self.cancel_lifecycle_intent(local);
            return Err(ContextCompactionError::Storage);
        }
        if matches!(local.origin, CompactionOrigin::Lifecycle { .. }) {
            self.cancel_lifecycle_intent(local);
            self.scheduler_signal
                .wake(AcceptedInputWakeReason::ExecutionReady);
        }
        self.complete_local(
            local,
            if successful {
                ContextCompactionOutcome::Succeeded
            } else {
                ContextCompactionOutcome::Failed
            },
        );
        Ok(())
    }

    pub(super) fn ensure_lifecycle_content(
        &self,
    ) -> Result<syndic_storage::ContentReference, LifecycleContentFailure> {
        #[cfg(feature = "test-faults")]
        if self.fail_lifecycle_staging_for_test() {
            return Err(LifecycleContentFailure::DefinitivePreparation);
        }
        let prepared = syndic_storage::prepare_lifecycle_continuation_content()
            .map_err(|_| LifecycleContentFailure::DefinitivePreparation)?;
        for _ in 0..8 {
            let manifest = self
                .storage
                .content_manifest(&self.home, prepared.id(), point_limit())
                .map_err(|_| LifecycleContentFailure::Home)?;
            let Some(manifest) = manifest else {
                let _ = self.execute_content_contribution(|revision| {
                    self.storage
                        .begin_content(revision, ContentBuild::from_prepared(&prepared))
                });
                continue;
            };
            if manifest.lifecycle() == ContentLifecycle::Sealed {
                return manifest
                    .sealed_reference()
                    .filter(|content| {
                        content.id() == prepared.id()
                            && content.encoding() == prepared.encoding()
                            && content.summary() == prepared.summary()
                    })
                    .ok_or(LifecycleContentFailure::Home);
            }
            let append = ContentAppend::prepare(&manifest, &prepared)
                .map_err(|_| LifecycleContentFailure::Home)?;
            if let Some(append) = append {
                let _ = self.execute_content_contribution(|revision| {
                    self.storage.append_content(revision, append)
                });
                continue;
            }
            let _ = self.home.execute_current(
                self.storage.current_seal_lifecycle_continuation_content(
                    SealLifecycleContinuationContent::new(manifest),
                ),
            );
        }
        Err(LifecycleContentFailure::Home)
    }

    fn execute_content_contribution(
        &self,
        contribution: impl FnOnce(beryl_model::DomainRevision) -> beryl_home_store::MutationContribution,
    ) -> Result<(), ContextCompactionError> {
        let home_revision = self
            .home
            .home_revision()
            .map_err(|_| ContextCompactionError::Storage)?;
        let domain_revision = self
            .storage
            .revision(&self.home)
            .map_err(|_| ContextCompactionError::Storage)?;
        let mut command = HomeCommand::new(home_revision);
        command
            .add(contribution(domain_revision))
            .map_err(|_| ContextCompactionError::Storage)?;
        self.home
            .execute(command)
            .map_err(|_| ContextCompactionError::Storage)?;
        Ok(())
    }

    pub(super) fn cancel_lifecycle_intent(&self, local: &LocalCompaction) {
        if let CompactionOrigin::Lifecycle { yielding_turn_id } = local.origin {
            let _ = self
                .stop
                .take_terminal_lifecycle_yield(local.operation_id.thread_id(), yielding_turn_id);
        }
    }

    pub(super) fn fail_local(&self, local: &LocalCompaction) {
        self.cancel_lifecycle_intent(local);
        self.complete_local(local, ContextCompactionOutcome::Failed);
    }

    pub(super) fn complete_local(
        &self,
        local: &LocalCompaction,
        outcome: ContextCompactionOutcome,
    ) {
        local.complete(outcome);
        local.release_command();
        self.remove_local(local);
    }

    pub(super) fn ensure_local_current(
        &self,
        local: &LocalCompaction,
    ) -> Result<(), ContextCompactionError> {
        if !local.command_is_current() {
            return Err(ContextCompactionError::Unavailable);
        }
        self.ensure_current()
    }

    pub(super) fn observe_request(
        &self,
        local: &LocalCompaction,
        disposition: CompactionRequestDisposition,
    ) -> Result<CompactionRequestTransitionStatus, ContextCompactionError> {
        self.ensure_local_current(local)?;
        let _mutation = local
            .mutation
            .lock()
            .map_err(|_| ContextCompactionError::AuthorityMismatch)?;
        let before = self.read_operation(local.operation_id)?;
        let request = PublishCompactionRequestDisposition::new(
            local.operation_id,
            before.revision(),
            local.attempt,
            disposition,
        );
        let _ = self.home.execute_current(
            self.storage
                .current_publish_compaction_request_disposition(request),
        );
        let status = self
            .storage
            .compaction_request_disposition_status(&self.home, &request, point_limit())
            .map_err(|_| ContextCompactionError::Storage)?;
        match status {
            CompactionRequestTransitionStatus::Exact
            | CompactionRequestTransitionStatus::TerminalAlreadySettled => Ok(status),
            CompactionRequestTransitionStatus::Prior
            | CompactionRequestTransitionStatus::Collision => Err(ContextCompactionError::Storage),
        }
    }

    pub(super) fn settle(
        &self,
        local: &LocalCompaction,
        settlement: CompactionSettlement,
    ) -> Result<(), ContextCompactionError> {
        self.ensure_local_current(local)?;
        let _mutation = local
            .mutation
            .lock()
            .map_err(|_| ContextCompactionError::AuthorityMismatch)?;
        let before = self.read_operation(local.operation_id)?;
        let _ = self
            .home
            .execute_current(self.storage.current_settle_compaction_operation(
                SettleCompactionOperation::new(local.operation_id, before.revision(), settlement),
            ));
        let after = self.read_operation(local.operation_id)?;
        if !matches!(after.state(), CompactionOperationState::Consumed(_)) {
            return Err(ContextCompactionError::Storage);
        }
        Ok(())
    }

    pub(super) fn abandon(
        &self,
        local: &LocalCompaction,
        reason: CompactionAbandonmentReason,
    ) -> Result<(), ContextCompactionError> {
        self.ensure_local_current(local)?;
        let _mutation = local
            .mutation
            .lock()
            .map_err(|_| ContextCompactionError::AuthorityMismatch)?;
        let before = self.read_operation(local.operation_id)?;
        if matches!(before.state(), CompactionOperationState::Consumed(_)) {
            return Ok(());
        }
        let _ = self
            .home
            .execute_current(self.storage.current_abandon_compaction_operation(
                syndic_storage::AbandonCompactionOperation::new(
                    local.operation_id,
                    before.revision(),
                    reason,
                ),
            ));
        let after = self.read_operation(local.operation_id)?;
        if !matches!(after.state(), CompactionOperationState::Consumed(_)) {
            return Err(ContextCompactionError::Storage);
        }
        Ok(())
    }
}
