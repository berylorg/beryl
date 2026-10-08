use super::*;

impl RuntimeBackedWindowAcquisitionService {
    pub(super) fn prepare_acquisition(
        &self,
        request: &RuntimeBackedWindowAcquisitionRequest,
        cancellation: &CommandCancellation,
    ) -> Result<PreparedAcquisition, PreparationFailure> {
        let home_revision = self.store.home_revision().map_err(preparation)?;
        let session_revision = self
            .state
            .session()
            .revision(&self.store)
            .map_err(preparation)?;
        let Some(session) = self
            .state
            .session()
            .minimal_bootstrap(&self.store)
            .map_err(preparation)?
        else {
            return Err(PreparationFailure::NotCommitted(
                RuntimeBackedWindowAcquisitionNotCommitted::SessionNotInitialized,
            ));
        };
        if session.windows().len() >= MAX_RESTORABLE_WINDOWS {
            return Err(PreparationFailure::NotCommitted(
                RuntimeBackedWindowAcquisitionNotCommitted::WindowCapacity,
            ));
        }

        let runtime_revision = self
            .state
            .runtime_roots()
            .revision(&self.store)
            .map_err(preparation)?;
        let runtime_source = self
            .state
            .runtime_roots()
            .catalog_source(
                &self.store,
                request.target.runtime_id(),
                request.target.root_id(),
            )
            .map_err(preparation)?;
        validate_execution(request.fallback_execution(), &runtime_source).map_err(|error| {
            PreparationFailure::NotCommitted(
                RuntimeBackedWindowAcquisitionNotCommitted::Preparation(Box::new(error)),
            )
        })?;

        let catalog_revision = self
            .state
            .catalog()
            .revision(&self.store)
            .map_err(preparation)?;
        let durable_job_revision = self
            .state
            .durable_jobs()
            .revision(&self.store)
            .map_err(preparation)?;
        let syndic_revision = self.syndic.revision(&self.store).map_err(preparation)?;
        let limits = CursorReadLimits::new(
            syndic_storage::THREAD_DISCOVERY_PAGE_MAX_ITEMS,
            syndic_storage::THREAD_DISCOVERY_PAGE_MAX_BYTES,
        )
        .expect("thread discovery page limits are nonzero");
        let mut after = None;
        let mut selected: Option<ReuseCandidate> = None;
        loop {
            if cancellation.is_cancelled() {
                return Err(PreparationFailure::NotCommitted(
                    RuntimeBackedWindowAcquisitionNotCommitted::Cancelled,
                ));
            }
            let page = self
                .syndic
                .threads_page(&self.store, after, limits)
                .map_err(preparation)?;
            for thread in page.records() {
                if cancellation.is_cancelled() {
                    return Err(PreparationFailure::NotCommitted(
                        RuntimeBackedWindowAcquisitionNotCommitted::Cancelled,
                    ));
                }
                if self
                    .state
                    .session()
                    .thread_claim_catalog_source(&self.store, thread.id())
                    .map_err(preparation)?
                    .claim()
                    .is_some()
                {
                    continue;
                }
                let Some(candidate) = self
                    .syndic
                    .inspect_eligible_empty_thread(
                        &self.store,
                        thread.id(),
                        request.fallback_execution(),
                    )
                    .map_err(preparation)?
                else {
                    continue;
                };
                let Some(job_guard) = self
                    .state
                    .durable_jobs()
                    .thread_reuse_guard(&self.store, candidate.thread_id())
                    .map_err(preparation)?
                else {
                    continue;
                };
                let replace = selected.as_ref().is_none_or(|best| {
                    (candidate.created_at(), candidate.thread_id())
                        < (best.candidate.created_at(), best.candidate.thread_id())
                });
                if replace {
                    selected = Some(ReuseCandidate {
                        candidate,
                        job_guard,
                    });
                }
            }
            if !page.has_more() {
                break;
            }
            after = page.records().last().map(|thread| thread.id());
            if after.is_none() {
                return Err(PreparationFailure::NotCommitted(
                    RuntimeBackedWindowAcquisitionNotCommitted::Preparation(Box::new(
                        AcquisitionInvariant(
                            "thread page advertised continuation without a cursor",
                        ),
                    )),
                ));
            }
        }

        let target = request.target;
        let placement = request.placement.clone();
        if selected.as_ref().is_some_and(|reuse| {
            reuse.candidate.thread_id() == request.fallback_thread_id
                || reuse.candidate.draft_id() == request.fallback_draft_id
        }) {
            return Err(PreparationFailure::NotCommitted(
                RuntimeBackedWindowAcquisitionNotCommitted::WindowIdentityCollision,
            ));
        }
        let (thread_id, draft_id, disposition, intent) = match selected {
            Some(reuse) => (
                reuse.candidate.thread_id(),
                reuse.candidate.draft_id(),
                RuntimeBackedWindowAcquisitionDisposition::Reused,
                AcquisitionIntent::Reuse(reuse),
            ),
            None => {
                let creation = CreateThread::ordinary(
                    request.fallback_thread_id,
                    request.fallback_draft_id,
                    request.fallback_execution.clone(),
                    request.fallback_created_at,
                    request.fallback_history_policy,
                );
                (
                    request.fallback_thread_id,
                    request.fallback_draft_id,
                    RuntimeBackedWindowAcquisitionDisposition::Created,
                    AcquisitionIntent::Create(creation),
                )
            }
        };
        let mut acquisition = RuntimeBackedWindowAcquisition {
            home_id: self.store.home_id(),
            window_id: request.window_id,
            thread_id,
            draft_id,
            target,
            placement: placement.clone(),
            disposition,
            window_revision: beryl_state::RecordRevision::INITIAL,
            fallback_thread_id: request.fallback_thread_id,
            fallback_draft_id: request.fallback_draft_id,
            fallback_execution: request.fallback_execution.clone(),
            fallback_created_at: request.fallback_created_at,
            reused_source: None,
            catalog_audit: None,
        };
        let session_command = CreateClaimedWindow::new(
            session.header().revision(),
            request.window_id,
            target,
            thread_id,
            placement,
        );
        let claim = session_command.catalog_claim();
        let mut command = HomeCommand::new(home_revision).with_cancellation(cancellation.clone());
        command
            .add(
                self.state
                    .session()
                    .create_claimed_window(session_revision, session_command),
            )
            .map_err(command_build)?;
        let catalog_row = match intent {
            AcquisitionIntent::Reuse(reuse) => {
                let (row, source, _) =
                    crate::main_window::running_threads::activation::prepare_catalog_row_for_claim(
                        &self.store,
                        &self.state,
                        &self.syndic,
                        thread_id,
                        Some(claim),
                    )
                    .map_err(preparation)?;
                acquisition.reused_source = Some(
                    self.syndic
                        .prepare_eligible_empty_thread_outcome(
                            &self.store,
                            reuse.candidate.clone(),
                            source.clone(),
                        )
                        .map_err(preparation)?,
                );
                match self
                    .syndic
                    .reuse_empty_thread_with_catalog_predecessor(
                        &self.store,
                        reuse.candidate,
                        source,
                        None,
                    )
                    .map_err(preparation)?
                {
                    syndic_storage::ThreadAcquisitionContribution::Validation(contribution) => {
                        command
                            .add_validation(contribution)
                            .map_err(command_build)?;
                    }
                    syndic_storage::ThreadAcquisitionContribution::Mutation(contribution) => {
                        command.add(contribution).map_err(command_build)?;
                    }
                }
                command
                    .add_validation(
                        self.state
                            .durable_jobs()
                            .validate_thread_reuse_guard(durable_job_revision, reuse.job_guard),
                    )
                    .map_err(command_build)?;
                row
            }
            AcquisitionIntent::Create(creation) => {
                let summary = creation.initial_catalog_summary();
                let facts = crate::catalog_projection::project_facts(
                    &summary,
                    &runtime_source,
                    CatalogClaimSummary::claimed(
                        claim.window_id(),
                        beryl_state::CatalogClaimKind::Active,
                    ),
                )
                .map_err(preparation)?;
                let sources = CatalogSourceRevisions::new(
                    summary.revision(),
                    runtime_source.runtime().revision(),
                    runtime_source.root().revision(),
                    Some(claim.revision()),
                );
                let row = CatalogClaimReplacementRow::new(thread_id, None, sources, facts)
                    .map_err(preparation)?;
                command
                    .add(self.syndic.create_thread(syndic_revision, creation))
                    .map_err(command_build)?;
                row
            }
        };
        let (publication, audit) =
            PublishCatalogClaimReplacement::from_planned_rows(catalog_row, None);
        acquisition.catalog_audit = Some(audit);
        command
            .add(
                self.state
                    .catalog()
                    .replace_claim_projection(catalog_revision, publication),
            )
            .map_err(command_build)?;
        command
            .add_validation(
                self.state
                    .runtime_roots()
                    .validate_catalog_source(runtime_revision, runtime_source),
            )
            .map_err(command_build)?;
        if cancellation.is_cancelled() {
            return Err(PreparationFailure::NotCommitted(
                RuntimeBackedWindowAcquisitionNotCommitted::Cancelled,
            ));
        }
        if self.store.home_revision().map_err(preparation)? != home_revision {
            return Err(preparation(AcquisitionInvariant(
                "source changed during thread election",
            )));
        }
        Ok(PreparedAcquisition {
            acquisition,
            command,
        })
    }
}
