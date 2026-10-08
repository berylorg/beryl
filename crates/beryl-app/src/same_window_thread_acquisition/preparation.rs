use super::*;

impl SameWindowThreadRequest {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        window: WindowId,
        selected: Option<WindowClaimSelection>,
        target: RememberedTarget,
        thread: SyndicThreadId,
        draft: SyndicDraftId,
        execution: ExecutionBinding,
        created: SyndicTimestamp,
        history: DraftEditHistoryPolicyV1,
    ) -> Result<Self, SameWindowThreadError> {
        if execution.runtime_id() != target.runtime_id() || execution.root_id() != target.root_id()
        {
            return Err(RunningThreadActivationError::TargetMismatch.into());
        }
        Ok(Self {
            window,
            selected,
            target,
            creation: CreateThread::ordinary(thread, draft, execution, created, history),
        })
    }

    pub fn window_id(&self) -> WindowId {
        self.window
    }
    pub fn selected(&self) -> Option<WindowClaimSelection> {
        self.selected
    }

    pub fn prepare(
        self,
        store: &HomeStore,
        state: &BerylState,
        syndic: &SyndicStorage,
        cancellation: CommandCancellation,
    ) -> Result<SameWindowThreadPreparation, SameWindowThreadError> {
        let mut command = HomeCommand::new(
            store
                .home_revision()
                .map_err(RunningThreadActivationError::from)?,
        )
        .with_cancellation(cancellation.clone());
        let revision = store
            .home_revision()
            .map_err(RunningThreadActivationError::from)?;
        let original = state
            .session()
            .capture_window_removal(store, self.window)
            .map_err(RunningThreadActivationError::from)?;
        if original.window().selected_thread() != self.selected {
            return Err(SameWindowThreadError::SourceChanged);
        }
        let summary = self.creation.initial_catalog_summary();
        let runtime_revision = state
            .runtime_roots()
            .revision(store)
            .map_err(RunningThreadActivationError::from)?;
        let runtime = state
            .runtime_roots()
            .catalog_source(store, self.target.runtime_id(), self.target.root_id())
            .map_err(CatalogProjectionBuildError::from)
            .map_err(RunningThreadActivationError::from)?;
        validate_execution_binding(&summary, &runtime)
            .map_err(RunningThreadActivationError::from)?;
        if cancellation.is_cancelled() {
            return Err(SameWindowThreadError::Cancelled);
        }
        if let Some(claim) = original.claim() {
            if original.window().remembered_target() == Some(self.target) {
                if let Some(candidate) = syndic
                    .inspect_eligible_empty_thread(store, claim.thread_id(), summary.execution())
                    .map_err(RunningThreadActivationError::from)?
                {
                    if state
                        .durable_jobs()
                        .thread_reuse_guard(store, candidate.thread_id())
                        .map_err(|error| SameWindowThreadError::Eligibility(error.to_string()))?
                        .is_some()
                    {
                        if cancellation.is_cancelled() {
                            return Err(SameWindowThreadError::Cancelled);
                        }
                        if store
                            .home_revision()
                            .map_err(RunningThreadActivationError::from)?
                            != revision
                        {
                            return Err(SameWindowThreadError::SourceChanged);
                        }
                        return Ok(SameWindowThreadPreparation::Current {
                            window: original.window().clone(),
                            claim,
                            draft: candidate.draft_id(),
                        });
                    }
                }
            }
        }
        let catalog_revision = state
            .catalog()
            .revision(store)
            .map_err(RunningThreadActivationError::from)?;
        let jobs = state
            .durable_jobs()
            .revision(store)
            .map_err(RunningThreadActivationError::from)?;
        let mut after = None;
        let mut best = None;
        let limits = CursorReadLimits::new(
            syndic_storage::THREAD_DISCOVERY_PAGE_MAX_ITEMS,
            syndic_storage::THREAD_DISCOVERY_PAGE_MAX_BYTES,
        )
        .unwrap();
        loop {
            let page = syndic
                .threads_page(store, after, limits)
                .map_err(RunningThreadActivationError::from)?;
            for thread in page.records() {
                if cancellation.is_cancelled() {
                    return Err(SameWindowThreadError::Cancelled);
                }
                if state
                    .session()
                    .thread_claim_catalog_source(store, thread.id())
                    .map_err(CatalogProjectionBuildError::from)
                    .map_err(RunningThreadActivationError::from)?
                    .claim()
                    .is_some()
                {
                    continue;
                }
                let Some(candidate) = syndic
                    .inspect_eligible_empty_thread(store, thread.id(), summary.execution())
                    .map_err(RunningThreadActivationError::from)?
                else {
                    continue;
                };
                let Some(guard) = state
                    .durable_jobs()
                    .thread_reuse_guard(store, candidate.thread_id())
                    .map_err(|error| SameWindowThreadError::Eligibility(error.to_string()))?
                else {
                    continue;
                };
                if best.as_ref().is_none_or(
                    |(prior, _): &(
                        syndic_storage::EligibleEmptyThreadCandidate,
                        beryl_state::ThreadReuseJobGuard,
                    )| {
                        (candidate.created_at(), candidate.thread_id())
                            < (prior.created_at(), prior.thread_id())
                    },
                ) {
                    best = Some((candidate, guard));
                }
            }
            if !page.has_more() {
                break;
            }
            after = Some(
                page.records()
                    .last()
                    .map(|thread| thread.id())
                    .ok_or(SameWindowThreadError::SourceChanged)?,
            );
        }
        let (thread, draft, disposition) = best.as_ref().map_or(
            (
                summary.thread_id(),
                self.creation.draft_id(),
                SameWindowThreadDisposition::Created,
            ),
            |(candidate, _)| {
                (
                    candidate.thread_id(),
                    candidate.draft_id(),
                    SameWindowThreadDisposition::Reused,
                )
            },
        );
        let WindowClaimReplacementPreparation::Prepared(replacement) = state
            .session()
            .prepare_window_claim_replacement(
                store,
                self.window,
                self.selected,
                self.target,
                thread,
            )
            .map_err(RunningThreadActivationError::from)?
        else {
            return Err(SameWindowThreadError::SourceChanged);
        };
        command
            .add(
                replacement
                    .contribution(&state.session(), store)
                    .map_err(RunningThreadActivationError::from)?,
            )
            .map_err(RunningThreadActivationError::from)?;
        let claim = replacement.future_claim();
        let (old_row, old_source) = match replacement.prior_claim() {
            Some(prior) => {
                let (row, source, _) =
                    prepare_catalog_row(store, state, syndic, prior.thread_id(), None)?;
                (Some(row), Some(source))
            }
            None => (None, None),
        };
        let target_row = if let Some((candidate, guard)) = best {
            let (row, target_source, _) =
                prepare_catalog_row(store, state, syndic, thread, Some(claim))?;
            match syndic
                .reuse_empty_thread_with_catalog_predecessor(
                    store,
                    candidate,
                    target_source,
                    old_source,
                )
                .map_err(RunningThreadActivationError::from)?
            {
                syndic_storage::ThreadAcquisitionContribution::Validation(contribution) => {
                    command
                        .add_validation(contribution)
                        .map_err(RunningThreadActivationError::from)?;
                }
                syndic_storage::ThreadAcquisitionContribution::Mutation(contribution) => {
                    command
                        .add(contribution)
                        .map_err(RunningThreadActivationError::from)?;
                }
            }
            command
                .add_validation(
                    state
                        .durable_jobs()
                        .validate_thread_reuse_guard(jobs, guard),
                )
                .map_err(RunningThreadActivationError::from)?;
            row
        } else {
            let facts = project_facts(
                &summary,
                &runtime,
                CatalogClaimSummary::claimed(self.window, CatalogClaimKind::Active),
            )
            .map_err(RunningThreadActivationError::from)?;
            let sources = CatalogSourceRevisions::new(
                summary.revision(),
                runtime.runtime().revision(),
                runtime.root().revision(),
                Some(claim.revision()),
            );
            let row = CatalogClaimReplacementRow::new(thread, None, sources, facts)
                .map_err(CatalogProjectionBuildError::from)
                .map_err(RunningThreadActivationError::from)?;
            command
                .add(
                    syndic
                        .create_thread_with_catalog_predecessor(
                            syndic
                                .revision(store)
                                .map_err(RunningThreadActivationError::from)?,
                            self.creation,
                            old_source,
                        )
                        .map_err(RunningThreadActivationError::from)?,
                )
                .map_err(RunningThreadActivationError::from)?;
            row
        };
        let (publication, rows) =
            PublishCatalogClaimReplacement::from_planned_rows(target_row, old_row);
        command
            .add(
                state
                    .catalog()
                    .replace_claim_projection(catalog_revision, publication),
            )
            .map_err(RunningThreadActivationError::from)?;
        command
            .add_validation(
                state
                    .runtime_roots()
                    .validate_catalog_source(runtime_revision, runtime),
            )
            .map_err(RunningThreadActivationError::from)?;
        if cancellation.is_cancelled() {
            return Err(SameWindowThreadError::Cancelled);
        }
        if store
            .home_revision()
            .map_err(RunningThreadActivationError::from)?
            != revision
        {
            return Err(SameWindowThreadError::SourceChanged);
        }
        Ok(SameWindowThreadPreparation::Prepared(
            SameWindowThreadAcquisition {
                home: store.home_id(),
                generation: store
                    .generation_identity()
                    .map_err(RunningThreadActivationError::from)?,
                replacement,
                draft,
                disposition,
                command,
                rows,
            },
        ))
    }
}
