use super::*;

impl SyndicStorage {
    pub fn draft_mutation_staging_status(
        &self,
        store: &beryl_home_store::HomeStore,
        identity: DraftMutationStagingIdentityV1,
    ) -> Result<DraftMutationStagingStatusV1, DraftMutationStagingErrorV1> {
        let Some(head) = self.draft_mutation_staging_head(store, identity)? else {
            let begin_receipt = DraftMutationStagingProgressReceiptKeyV1::new(identity, 1)
                .ok_or(DraftMutationStagingErrorV1::Invariant)?;
            let limit =
                crate::SyndicPointReadLimit::new(65_536).expect("staging point limit is nonzero");
            if self
                .point::<DraftMutationStagingProgressFamily>(store, begin_receipt, limit)?
                .is_some()
            {
                return Err(DraftMutationStagingErrorV1::Invariant);
            }
            return Ok(DraftMutationStagingStatusV1::Absent);
        };
        if head.identity() != identity || !draft_mutation_staging_head_is_locally_exact(&head) {
            return Err(DraftMutationStagingErrorV1::Invariant);
        }
        let limit =
            crate::SyndicPointReadLimit::new(65_536).expect("staging point limit is nonzero");
        let key = DraftMutationStagingProgressReceiptKeyV1::new(
            identity,
            head.receipt().transition_ordinal(),
        )
        .ok_or(DraftMutationStagingErrorV1::Invariant)?;
        let receipt = self
            .point::<DraftMutationStagingProgressFamily>(store, key, limit)?
            .ok_or(DraftMutationStagingErrorV1::Invariant)?;
        if !draft_mutation_staging_receipt_is_locally_exact(&receipt)
            || !receipt_finish_digest_is_exact(&receipt)
            || receipt.digest() != head.receipt().digest()
            || receipt.after_head_digest() != head.digest()
            || receipt.after_lifecycle() != head.lifecycle()
            || receipt.after_source() != head.source()
            || receipt.after_proposal() != head.proposal()
        {
            return Err(DraftMutationStagingErrorV1::Invariant);
        }
        if let Some((page_key, page_digest)) = receipt.page() {
            let page = self
                .draft_mutation_staging_page(store, page_key)?
                .ok_or(DraftMutationStagingErrorV1::Invariant)?;
            if page.digest() != page_digest {
                return Err(DraftMutationStagingErrorV1::Invariant);
            }
            authenticate_staging_page_closure(self, store, &head, &page)?;
        }
        match (receipt.key().transition_ordinal(), receipt.prior()) {
            (1, None) => {}
            (ordinal, Some(prior))
                if ordinal > 1
                    && prior.identity() == identity
                    && prior.transition_ordinal() == ordinal - 1 =>
            {
                let prior_key = DraftMutationStagingProgressReceiptKeyV1::new(
                    identity,
                    prior.transition_ordinal(),
                )
                .ok_or(DraftMutationStagingErrorV1::Invariant)?;
                let prior_record = self
                    .point::<DraftMutationStagingProgressFamily>(store, prior_key, limit)?
                    .ok_or(DraftMutationStagingErrorV1::Invariant)?;
                if !draft_mutation_staging_receipt_is_locally_exact(&prior_record)
                    || !receipt_finish_digest_is_exact(&prior_record)
                    || prior_record.digest() != prior.digest()
                    || receipt.before_head_digest() != Some(prior_record.after_head_digest())
                {
                    return Err(DraftMutationStagingErrorV1::Invariant);
                }
                if let Some((page_key, _)) = prior_record.page() {
                    let page = self
                        .draft_mutation_staging_page(store, page_key)?
                        .ok_or(DraftMutationStagingErrorV1::Invariant)?;
                    authenticate_staging_page_closure(self, store, &head, &page)?;
                }
            }
            _ => return Err(DraftMutationStagingErrorV1::Invariant),
        }
        let terminal = matches!(
            head.lifecycle(),
            DraftMutationStagingLifecycleV1::Cancelled
                | DraftMutationStagingLifecycleV1::Rejected
                | DraftMutationStagingLifecycleV1::Conflict
                | DraftMutationStagingLifecycleV1::Error
        );
        let session_key = DraftEditorCandidateSessionRecordKeyV1::head(
            identity.draft_id(),
            identity.session_id(),
        );
        let stored_session =
            match self.point::<DraftEditorCandidateSessionsFamily>(store, session_key, limit)? {
                Some(DraftEditorCandidateSessionRecordV1::Head(head)) => Some(head),
                Some(DraftEditorCandidateSessionRecordV1::OpenReceipt(_)) => {
                    return Err(DraftMutationStagingErrorV1::Invariant);
                }
                None => None,
            };
        if terminal
            && stored_session.as_ref().is_some_and(|session| {
                terminal_session_has_same_operation_custody(session, identity)
            })
        {
            return Err(DraftMutationStagingErrorV1::Invariant);
        }
        let session = if terminal {
            None
        } else {
            Some(stored_session.ok_or(DraftMutationStagingErrorV1::Invariant)?)
        };
        let status = match head.lifecycle() {
            DraftMutationStagingLifecycleV1::Receiving => {
                let session = session
                    .as_ref()
                    .ok_or(DraftMutationStagingErrorV1::Invariant)?;
                let expected = staging_custody(head.begin(), head.begin_digest(), head.receipt());
                if !staging_session_matches_head(session, &head)
                    || session.active_operation() != Some(&expected)
                    || receipt.custody_after() != DraftMutationStagingCustodyTagV1::Staging
                {
                    return Err(DraftMutationStagingErrorV1::Invariant);
                }
                DraftMutationStagingStatusV1::Receiving {
                    head: head.receipt(),
                }
            }
            DraftMutationStagingLifecycleV1::Finished(_) => {
                let session = session
                    .as_ref()
                    .ok_or(DraftMutationStagingErrorV1::Invariant)?;
                let expected = staging_custody(head.begin(), head.begin_digest(), head.receipt());
                if !staging_session_matches_head(session, &head)
                    || session.active_operation() != Some(&expected)
                    || receipt.custody_after() != DraftMutationStagingCustodyTagV1::Staging
                {
                    return Err(DraftMutationStagingErrorV1::Invariant);
                }
                DraftMutationStagingStatusV1::Finished {
                    head: head.receipt(),
                }
            }
            DraftMutationStagingLifecycleV1::Building(build) => {
                let session = session
                    .as_ref()
                    .ok_or(DraftMutationStagingErrorV1::Invariant)?;
                let Some(operation) = session.active_operation().copied() else {
                    return Err(DraftMutationStagingErrorV1::Invariant);
                };
                let settlement = DraftPieceSettlementKeyV1::new(
                    identity.draft_id(),
                    identity.session_id(),
                    identity.operation_id().as_piece_operation(),
                );
                let Some((current_build, current_session)) =
                    super::mutation::authenticated_staging_build_from_store(
                        self, store, settlement,
                    )
                    .map_err(|_| DraftMutationStagingErrorV1::Invariant)?
                else {
                    return Err(DraftMutationStagingErrorV1::Invariant);
                };
                if !operation.is_building()
                    || operation.operation_id() != identity.operation_id().as_piece_operation()
                    || current_session != *session
                    || operation.build_receipt() != Some(current_build.progress_receipt())
                    || current_build.progress_receipt().key().transition_ordinal()
                        < build.key().transition_ordinal()
                    || receipt.custody_after() != DraftMutationStagingCustodyTagV1::Building
                    || receipt.build_endpoint() != Some(build)
                {
                    return Err(DraftMutationStagingErrorV1::Invariant);
                }
                DraftMutationStagingStatusV1::Building {
                    staging: head.receipt(),
                    build: current_build.progress_receipt(),
                }
            }
            lifecycle @ (DraftMutationStagingLifecycleV1::Cancelled
            | DraftMutationStagingLifecycleV1::Rejected
            | DraftMutationStagingLifecycleV1::Conflict
            | DraftMutationStagingLifecycleV1::Error) => {
                if receipt.custody_after() != DraftMutationStagingCustodyTagV1::None {
                    return Err(DraftMutationStagingErrorV1::Invariant);
                }
                let evidence = receipt
                    .terminal_evidence()
                    .filter(|value| terminal_lifecycle(*value) == lifecycle)
                    .ok_or(DraftMutationStagingErrorV1::Invariant)?;
                if !stored_terminal_evidence_matches(head.begin(), &receipt, evidence)
                    || !stored_occupied_error_is_exact(self, store, evidence)?
                {
                    return Err(DraftMutationStagingErrorV1::Invariant);
                }
                match lifecycle {
                    DraftMutationStagingLifecycleV1::Cancelled => {
                        DraftMutationStagingStatusV1::Cancelled {
                            head: head.receipt(),
                            evidence,
                        }
                    }
                    DraftMutationStagingLifecycleV1::Rejected => {
                        DraftMutationStagingStatusV1::Rejected {
                            head: head.receipt(),
                            evidence,
                        }
                    }
                    DraftMutationStagingLifecycleV1::Conflict => {
                        DraftMutationStagingStatusV1::Conflict {
                            head: head.receipt(),
                            evidence,
                        }
                    }
                    DraftMutationStagingLifecycleV1::Error => DraftMutationStagingStatusV1::Error {
                        head: head.receipt(),
                        evidence,
                    },
                    _ => unreachable!(),
                }
            }
        };
        Ok(status)
    }

    pub fn reconcile_draft_mutation_staging_command(
        &self,
        store: &beryl_home_store::HomeStore,
        prepared: &PreparedDraftMutationStagingCommandV1,
    ) -> Result<DraftMutationStagingReconcileV1, DraftMutationStagingErrorV1> {
        if prepared.target_head.begin().writer_admission().is_some() {
            return Err(DraftMutationStagingErrorV1::Invalid);
        }
        self.reconcile_draft_mutation_staging_command_exact(store, prepared)
    }

    pub fn reconcile_draft_mutation_staging_command_outcome(
        &self,
        store: &beryl_home_store::HomeStore,
        prepared: &PreparedDraftMutationStagingCommandV1,
        outcome: CommandOutcome,
    ) -> Result<DraftMutationStagingReconcileV1, DraftMutationStagingErrorV1> {
        let expectation = staging_command_outcome_expectation(store, outcome, |local, receipt| {
            if receipt.generation() != self.home_generation {
                return Err(DraftMutationStagingErrorV1::LocalFinalization(
                    CommittedLocalFinalizationError::StaleOrForeign,
                ));
            }
            store
                .with_committed_local_finalization(local, receipt, &self.handle, |attachment| {
                    let Some(admission) = prepared.target_head.begin().writer_admission() else {
                        return Ok(());
                    };
                    if admission.binding().home_generation().get() != self.home_generation.get() {
                        return Err(());
                    }
                    attachment.finalize_writer_committed_unknown(admission.binding().owner())
                })
                .map_err(DraftMutationStagingErrorV1::LocalFinalization)?
                .map_err(|()| DraftMutationStagingErrorV1::LocalCustody)
        })?;
        let reconciled = self.reconcile_draft_mutation_staging_command_exact(store, prepared)?;
        if !staging_outcome_matches(expectation, &reconciled) {
            return Err(DraftMutationStagingErrorV1::Invariant);
        }
        Ok(reconciled)
    }

    fn reconcile_draft_mutation_staging_command_exact(
        &self,
        store: &beryl_home_store::HomeStore,
        prepared: &PreparedDraftMutationStagingCommandV1,
    ) -> Result<DraftMutationStagingReconcileV1, DraftMutationStagingErrorV1> {
        let identity = prepared.target_head.identity();
        let stored = self.draft_mutation_staging_head(store, identity)?;
        let limit =
            crate::SyndicPointReadLimit::new(65_536).expect("staging point limit is nonzero");
        let target_receipt =
            self.point::<DraftMutationStagingProgressFamily>(store, prepared.receipt.key(), limit)?;
        if stored.as_ref() == prepared.source_head.as_ref() {
            if target_receipt.is_some() {
                return Err(DraftMutationStagingErrorV1::Invariant);
            }
            return Ok(DraftMutationStagingReconcileV1::SourceSelected);
        }
        if stored.as_ref() != Some(&prepared.target_head)
            || target_receipt.as_ref() != Some(&prepared.receipt)
        {
            return Err(DraftMutationStagingErrorV1::Invariant);
        }
        let status = self.draft_mutation_staging_status(store, identity)?;
        if matches!(
            status,
            DraftMutationStagingStatusV1::Cancelled { .. }
                | DraftMutationStagingStatusV1::Rejected { .. }
                | DraftMutationStagingStatusV1::Conflict { .. }
                | DraftMutationStagingStatusV1::Error { .. }
        ) {
            if let Some(admission) = prepared.target_head.begin().writer_admission() {
                self.release_draft_marker_writer_terminal(
                    store,
                    admission,
                    prepared.receipt.digest(),
                )?;
            }
            Ok(DraftMutationStagingReconcileV1::Terminal(status))
        } else {
            if let Some(admission) = prepared.target_head.begin().writer_admission() {
                store
                    .with_domain_attachment(&self.handle.attachment_capability(), |attachment| {
                        attachment.resolve_writer_progress(admission.binding().owner())
                    })
                    .map_err(|_| DraftMutationStagingErrorV1::LocalCustody)?
                    .map_err(|()| DraftMutationStagingErrorV1::LocalCustody)?;
            }
            Ok(DraftMutationStagingReconcileV1::TargetSelected)
        }
    }

    pub fn reconcile_draft_mutation_staging_page_batch(
        &self,
        store: &beryl_home_store::HomeStore,
        prepared: &PreparedDraftMutationStagingBatchV1,
    ) -> Result<DraftMutationStagingReconcileV1, DraftMutationStagingErrorV1> {
        if prepared.source_head.begin().writer_admission().is_some() {
            return Err(DraftMutationStagingErrorV1::Invalid);
        }
        self.reconcile_draft_mutation_staging_page_batch_exact(store, prepared)
    }

    pub fn reconcile_draft_mutation_staging_page_batch_outcome(
        &self,
        store: &beryl_home_store::HomeStore,
        prepared: &PreparedDraftMutationStagingBatchV1,
        outcome: CommandOutcome,
    ) -> Result<DraftMutationStagingReconcileV1, DraftMutationStagingErrorV1> {
        let expectation = staging_command_outcome_expectation(store, outcome, |local, receipt| {
            if receipt.generation() != self.home_generation {
                return Err(DraftMutationStagingErrorV1::LocalFinalization(
                    CommittedLocalFinalizationError::StaleOrForeign,
                ));
            }
            store
                .with_committed_local_finalization(local, receipt, &self.handle, |attachment| {
                    let Some(admission) = prepared.target_head.begin().writer_admission() else {
                        return Ok(());
                    };
                    if admission.binding().home_generation().get() != self.home_generation.get() {
                        return Err(());
                    }
                    attachment.finalize_writer_committed_unknown(admission.binding().owner())
                })
                .map_err(DraftMutationStagingErrorV1::LocalFinalization)?
                .map_err(|()| DraftMutationStagingErrorV1::LocalCustody)
        })?;
        let reconciled = self.reconcile_draft_mutation_staging_page_batch_exact(store, prepared)?;
        if !staging_outcome_matches(expectation, &reconciled) {
            return Err(DraftMutationStagingErrorV1::Invariant);
        }
        Ok(reconciled)
    }

    fn reconcile_draft_mutation_staging_page_batch_exact(
        &self,
        store: &beryl_home_store::HomeStore,
        prepared: &PreparedDraftMutationStagingBatchV1,
    ) -> Result<DraftMutationStagingReconcileV1, DraftMutationStagingErrorV1> {
        let identity = prepared.source_head.identity();
        let stored_head = self.draft_mutation_staging_head(store, identity)?;
        let limit =
            crate::SyndicPointReadLimit::new(65_536).expect("staging point limit is nonzero");
        let session_key = DraftEditorCandidateSessionRecordKeyV1::head(
            prepared.source_session.draft_id(),
            prepared.source_session.session_id(),
        );
        let stored_session =
            match self.point::<DraftEditorCandidateSessionsFamily>(store, session_key, limit)? {
                Some(DraftEditorCandidateSessionRecordV1::Head(head)) => head,
                _ => return Err(DraftMutationStagingErrorV1::Invariant),
            };

        if stored_head.as_ref() == Some(&prepared.source_head) {
            if stored_session != prepared.source_session {
                return Err(DraftMutationStagingErrorV1::Invariant);
            }
            for target in prepared.targets.iter() {
                if self
                    .point::<DraftMutationStagingPagesFamily>(store, target.page.key(), limit)?
                    .is_some()
                    || self
                        .point::<DraftMutationStagingProgressFamily>(
                            store,
                            target.receipt.key(),
                            limit,
                        )?
                        .is_some()
                {
                    return Err(DraftMutationStagingErrorV1::Invariant);
                }
            }
            if !matches!(
                self.draft_mutation_staging_status(store, identity)?,
                DraftMutationStagingStatusV1::Receiving { .. }
            ) {
                return Err(DraftMutationStagingErrorV1::Invariant);
            }
            return Ok(DraftMutationStagingReconcileV1::SourceSelected);
        }

        if stored_head.as_ref() != Some(&prepared.target_head)
            || stored_session != prepared.target_session
        {
            return Err(DraftMutationStagingErrorV1::Invariant);
        }
        for target in prepared.targets.iter() {
            if self
                .point::<DraftMutationStagingPagesFamily>(store, target.page.key(), limit)?
                .as_ref()
                != Some(&target.page)
                || self
                    .point::<DraftMutationStagingProgressFamily>(
                        store,
                        target.receipt.key(),
                        limit,
                    )?
                    .as_ref()
                    != Some(&target.receipt)
            {
                return Err(DraftMutationStagingErrorV1::Invariant);
            }
        }
        if !matches!(
            self.draft_mutation_staging_status(store, identity)?,
            DraftMutationStagingStatusV1::Receiving { .. }
        ) {
            return Err(DraftMutationStagingErrorV1::Invariant);
        }
        if let Some(admission) = prepared.target_head.begin().writer_admission() {
            store
                .with_domain_attachment(&self.handle.attachment_capability(), |attachment| {
                    attachment.resolve_writer_progress(admission.binding().owner())
                })
                .map_err(|_| DraftMutationStagingErrorV1::LocalCustody)?
                .map_err(|()| DraftMutationStagingErrorV1::LocalCustody)?;
        }
        Ok(DraftMutationStagingReconcileV1::TargetSelected)
    }
}

#[derive(Clone, Copy)]
enum StagingOutcomeExpectation {
    Source,
    Target,
    ExactTargetReplay,
}

fn staging_command_outcome_expectation(
    store: &beryl_home_store::HomeStore,
    outcome: CommandOutcome,
    finalize: impl FnOnce(
        beryl_home_store::CommittedLocalFinalization,
        &beryl_home_store::CommitReceipt,
    ) -> Result<(), DraftMutationStagingErrorV1>,
) -> Result<StagingOutcomeExpectation, DraftMutationStagingErrorV1> {
    match outcome {
        CommandOutcome::NotCommitted {
            evidence: CommandError::EmptyContribution { domain },
        } if domain == SyndicDomain::NAME => Ok(StagingOutcomeExpectation::ExactTargetReplay),
        CommandOutcome::NotCommitted { .. } => Ok(StagingOutcomeExpectation::Source),
        CommandOutcome::Committed {
            receipt,
            local_finalization,
            ..
        } => {
            if let Some(local_finalization) = local_finalization {
                finalize(local_finalization, &receipt)?;
            }
            Ok(StagingOutcomeExpectation::Target)
        }
        CommandOutcome::Indeterminate { reconciliation, .. } => {
            match store
                .reconcile(&reconciliation.install_and_handle())
                .map_err(DraftMutationStagingErrorV1::Reconciliation)?
            {
                ReconciliationResolution::ExactOld => Ok(StagingOutcomeExpectation::Source),
                ReconciliationResolution::ExactNew { .. } => Ok(StagingOutcomeExpectation::Target),
                ReconciliationResolution::ExactSuccessor { .. }
                | ReconciliationResolution::Collision => {
                    Err(DraftMutationStagingErrorV1::Invariant)
                }
            }
        }
    }
}

fn staging_outcome_matches(
    expectation: StagingOutcomeExpectation,
    reconciled: &DraftMutationStagingReconcileV1,
) -> bool {
    match expectation {
        StagingOutcomeExpectation::Source => {
            matches!(reconciled, DraftMutationStagingReconcileV1::SourceSelected)
        }
        StagingOutcomeExpectation::Target | StagingOutcomeExpectation::ExactTargetReplay => {
            matches!(
                reconciled,
                DraftMutationStagingReconcileV1::TargetSelected
                    | DraftMutationStagingReconcileV1::Terminal(_)
            )
        }
    }
}
