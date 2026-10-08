use super::*;

impl RuntimeBackedWindowAcquisitionService {
    pub(super) fn reconcile_natural_state_in_flight(
        &self,
        request: &RuntimeBackedWindowAcquisitionRequest,
        cancellation: &CommandCancellation,
    ) -> RuntimeBackedWindowAcquisitionNaturalReconciliationOutcome {
        let window_id = request.window_id;
        for _ in 0..=MAX_RESTORABLE_WINDOWS {
            let before = match self.store.home_revision() {
                Ok(revision) => revision,
                Err(error) => {
                    return natural_reconciliation_not_committed(window_id, error);
                }
            };
            let natural = match self.state.audit_window_acquisition_with_cancellation(
                &self.store,
                window_id,
                cancellation,
            ) {
                Ok(natural) => natural,
                Err(WindowAcquisitionAuditError::Cancelled) => {
                    return RuntimeBackedWindowAcquisitionNaturalReconciliationOutcome::NotCommitted {
                        window_id,
                        evidence: RuntimeBackedWindowAcquisitionNotCommitted::Cancelled,
                    };
                }
                Err(WindowAcquisitionAuditError::RepairNeeded { thread_id }) => {
                    return RuntimeBackedWindowAcquisitionNaturalReconciliationOutcome::NotCommitted {
                        window_id,
                        evidence:
                            RuntimeBackedWindowAcquisitionNotCommitted::CatalogRepairNeeded(
                                thread_id,
                            ),
                    };
                }
                Err(error) => {
                    return natural_reconciliation_not_committed(window_id, error);
                }
            };
            if cancellation.is_cancelled() {
                return RuntimeBackedWindowAcquisitionNaturalReconciliationOutcome::NotCommitted {
                    window_id,
                    evidence: RuntimeBackedWindowAcquisitionNotCommitted::Cancelled,
                };
            }
            let audited = match natural {
                WindowAcquisitionNaturalState::Missing => {
                    match self.syndic.audit_pristine_thread(
                        &self.store,
                        request.fallback_thread_id,
                        &request.fallback_execution,
                    ) {
                        Ok(PristineThreadAudit::Missing) => NaturalAcquisitionAudit::ExactOld,
                        Ok(PristineThreadAudit::Exact(_) | PristineThreadAudit::Conflict) => {
                            NaturalAcquisitionAudit::Collision
                        }
                        Err(error) => {
                            return natural_reconciliation_not_committed(window_id, error);
                        }
                    }
                }
                WindowAcquisitionNaturalState::Collision => NaturalAcquisitionAudit::Collision,
                WindowAcquisitionNaturalState::Committed(facts) => {
                    if facts.target() != request.target
                        || facts.placement() != &request.placement
                        || facts.fallback_target() != request.target
                    {
                        NaturalAcquisitionAudit::Collision
                    } else if facts.thread_id() != request.fallback_thread_id {
                        match self.inspect_natural_reused_source(
                            facts.thread_id(),
                            &request.fallback_execution,
                        ) {
                            Ok(Some((candidate, source)))
                                if candidate.draft_id() != request.fallback_draft_id =>
                            {
                                NaturalAcquisitionAudit::ExactCommitted(
                                    RuntimeBackedWindowAcquisition {
                                        home_id: self.store.home_id(),
                                        window_id,
                                        thread_id: candidate.thread_id(),
                                        draft_id: candidate.draft_id(),
                                        target: facts.target(),
                                        placement: facts.placement().clone(),
                                        disposition:
                                            RuntimeBackedWindowAcquisitionDisposition::Reused,
                                        window_revision: facts.window_revision(),
                                        fallback_thread_id: request.fallback_thread_id,
                                        fallback_draft_id: request.fallback_draft_id,
                                        fallback_execution: request.fallback_execution.clone(),
                                        fallback_created_at: request.fallback_created_at,
                                        reused_source: Some(source),
                                        catalog_audit: None,
                                    },
                                )
                            }
                            Ok(_) => NaturalAcquisitionAudit::Collision,
                            Err(eligible_source::AcquisitionSourceError::CatalogRepair(
                                thread_id,
                            )) => {
                                return RuntimeBackedWindowAcquisitionNaturalReconciliationOutcome::NotCommitted {
                                    window_id,
                                    evidence: RuntimeBackedWindowAcquisitionNotCommitted::CatalogRepairNeeded(thread_id),
                                };
                            }
                            Err(error) => {
                                return natural_reconciliation_not_committed(window_id, error);
                            }
                        }
                    } else {
                        match self.syndic.audit_pristine_thread(
                            &self.store,
                            facts.thread_id(),
                            &request.fallback_execution,
                        ) {
                            Ok(PristineThreadAudit::Exact(candidate)) => {
                                let disposition = (candidate.draft_id()
                                    == request.fallback_draft_id
                                    && candidate.created_at() == request.fallback_created_at)
                                    .then_some(RuntimeBackedWindowAcquisitionDisposition::Created);
                                match disposition {
                                    Some(disposition) => NaturalAcquisitionAudit::ExactCommitted(
                                        RuntimeBackedWindowAcquisition {
                                            home_id: self.store.home_id(),
                                            window_id,
                                            thread_id: candidate.thread_id(),
                                            draft_id: candidate.draft_id(),
                                            target: facts.target(),
                                            placement: facts.placement().clone(),
                                            disposition,
                                            window_revision: facts.window_revision(),
                                            fallback_thread_id: request.fallback_thread_id,
                                            fallback_draft_id: request.fallback_draft_id,
                                            fallback_execution: request.fallback_execution.clone(),
                                            fallback_created_at: request.fallback_created_at,
                                            reused_source: None,
                                            catalog_audit: None,
                                        },
                                    ),
                                    None => NaturalAcquisitionAudit::Collision,
                                }
                            }
                            Ok(PristineThreadAudit::Missing | PristineThreadAudit::Conflict) => {
                                NaturalAcquisitionAudit::Collision
                            }
                            Err(error) => {
                                return natural_reconciliation_not_committed(window_id, error);
                            }
                        }
                    }
                }
            };
            let after = match self.store.home_revision() {
                Ok(revision) => revision,
                Err(error) => {
                    return natural_reconciliation_not_committed(window_id, error);
                }
            };
            if before != after {
                continue;
            }
            return match audited {
                NaturalAcquisitionAudit::ExactOld => {
                    RuntimeBackedWindowAcquisitionNaturalReconciliationOutcome::ExactOld {
                        window_id,
                    }
                }
                NaturalAcquisitionAudit::ExactCommitted(acquisition) => {
                    RuntimeBackedWindowAcquisitionNaturalReconciliationOutcome::ExactCommitted {
                        acquisition,
                    }
                }
                NaturalAcquisitionAudit::Collision => {
                    RuntimeBackedWindowAcquisitionNaturalReconciliationOutcome::Collision {
                        window_id,
                    }
                }
            };
        }
        natural_reconciliation_not_committed(
            window_id,
            AcquisitionInvariant("natural-state reconciliation retry budget exhausted"),
        )
    }
}
