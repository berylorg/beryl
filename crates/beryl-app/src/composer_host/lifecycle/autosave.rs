use super::*;

impl SyndicComposerHost {
    pub fn publish_autosave_interval(
        &mut self,
        binding: ComposerHostBinding,
        settings_generation: u64,
        interval: ComposerHostAutosaveInterval,
    ) -> Result<ComposerHostAutosaveSettingsCompletion, ComposerHostError> {
        self.validate_lifecycle_binding(binding)?;
        if settings_generation <= self.lifecycle.settings_generation {
            return Ok(ComposerHostAutosaveSettingsCompletion::Stale);
        }
        self.lifecycle.settings_generation = settings_generation;
        self.lifecycle.interval = interval;
        let timer = if self.lifecycle.dirty_adoption_seen
            && self.is_dirty()
            && self.lifecycle.barrier.is_none()
        {
            self.lifecycle.arm(binding, Instant::now())
        } else {
            self.lifecycle.timer = None;
            None
        };
        Ok(ComposerHostAutosaveSettingsCompletion::Published(timer))
    }

    pub fn fire_autosave(
        &mut self,
        store: &HomeStore,
        timer: ComposerHostAutosaveTimer,
        assets: AssetState,
        marker_seals: &DraftMarkerSealService,
        operation_id: DraftPieceOperationIdV1,
        marker_authority: Option<ComposerHostMarkerSealAuthority>,
        published_at: SyndicTimestamp,
        cancellation: &CommandCancellation,
    ) -> Result<ComposerHostAutosaveCapture, ComposerHostError> {
        if !self.lifecycle.timer_matches(timer) || self.lifecycle.barrier.is_some() {
            return Ok(ComposerHostAutosaveCapture::Stale);
        }
        if self.lifecycle.autosave.is_some() {
            return Ok(ComposerHostAutosaveCapture::PublicationPending);
        }
        let Ok(binding) = self.validate_timer_binding(timer) else {
            return Ok(ComposerHostAutosaveCapture::Stale);
        };
        if !ComposerHostLifecycleCoordinator::callback_store_matches(binding, store) {
            return Ok(ComposerHostAutosaveCapture::Stale);
        }
        self.lifecycle.timer = None;
        let capture = self.capture_lifecycle_publication(
            store,
            assets,
            marker_seals,
            operation_id,
            marker_authority,
            published_at,
            cancellation,
        );
        let capture = match capture {
            Ok(capture) => capture,
            Err(error) => {
                if recoverable_error(&error)
                    && self.publication_unavailable().is_none()
                    && self.is_dirty()
                {
                    let _ = self.lifecycle.arm(binding, Instant::now());
                } else {
                    self.lifecycle.timer = None;
                }
                return Err(error);
            }
        };
        Ok(match capture {
            ComposerHostPublicationCapture::CleanNoOp => ComposerHostAutosaveCapture::Clean,
            ComposerHostPublicationCapture::CancelledBeforeAdmission => {
                if self.is_dirty() {
                    let _ = self.lifecycle.arm(binding, Instant::now());
                }
                ComposerHostAutosaveCapture::Cancelled
            }
            ComposerHostPublicationCapture::Captured(ticket) => {
                self.lifecycle.last_publication_completion = None;
                self.lifecycle.autosave = Some(PendingSave {
                    ticket,
                    timer_generation: Some(timer.timer_generation),
                    failure: None,
                });
                ComposerHostAutosaveCapture::Captured(ticket)
            }
        })
    }

    pub fn advance_autosave(
        &mut self,
        store: &HomeStore,
        ticket: ComposerHostPublicationTicket,
    ) -> Result<ComposerHostAutosaveAdvance, ComposerHostError> {
        let Some(pending) = self.lifecycle.autosave.as_ref() else {
            return Ok(ComposerHostAutosaveAdvance::Stale);
        };
        if pending.ticket != ticket {
            return Ok(ComposerHostAutosaveAdvance::Stale);
        }
        if !self.current_publication_callback(store, ticket) {
            return Ok(ComposerHostAutosaveAdvance::Stale);
        }
        match self.advance_publication_step(store, ticket) {
            Ok(PublicationStep::Progress) => {
                let failure = self
                    .lifecycle
                    .autosave
                    .as_ref()
                    .and_then(|save| save.failure);
                if self.publication.lane.is_none()
                    && let Some(failure) = failure
                {
                    return Ok(self.finish_autosave_failure(failure));
                }
                Ok(ComposerHostAutosaveAdvance::Progress)
            }
            Ok(PublicationStep::Ready) => Ok(ComposerHostAutosaveAdvance::Ready),
            Ok(PublicationStep::Complete(completion)) => {
                self.lifecycle.last_publication_completion = Some(completion);
                Ok(self.finish_autosave_completion(completion))
            }
            Err(error) => {
                if stale_callback_error(&error) {
                    return Ok(ComposerHostAutosaveAdvance::Stale);
                }
                if let Some(reason) = self.publication_unavailable() {
                    self.lifecycle.autosave = None;
                    self.lifecycle.timer = None;
                    return Ok(ComposerHostAutosaveAdvance::Unsatisfied(
                        publication_failure(reason),
                    ));
                }
                if !recoverable_error(&error) {
                    self.lifecycle.autosave = None;
                    self.lifecycle.timer = None;
                    return Ok(ComposerHostAutosaveAdvance::Unsatisfied(
                        ComposerHostFlushFailure::GenerationLost,
                    ));
                }
                if let Some(pending) = self.lifecycle.autosave.as_mut() {
                    pending.failure = Some(ComposerHostFlushFailure::Recoverable);
                }
                Err(error)
            }
        }
    }

    fn finish_autosave_completion(
        &mut self,
        completion: ComposerHostPublicationCompletion,
    ) -> ComposerHostAutosaveAdvance {
        match completion {
            ComposerHostPublicationCompletion::ReconciliationPending => {
                ComposerHostAutosaveAdvance::ReconciliationPending
            }
            ComposerHostPublicationCompletion::Published
            | ComposerHostPublicationCompletion::ExactReplay
            | ComposerHostPublicationCompletion::Superseded => {
                let timer_generation = self
                    .lifecycle
                    .autosave
                    .as_ref()
                    .and_then(|save| save.timer_generation);
                self.lifecycle.autosave = None;
                let dirty = self.is_dirty();
                if dirty {
                    self.rearm_autosave_generation(timer_generation);
                } else {
                    self.lifecycle.timer = None;
                    self.lifecycle.dirty_adoption_seen = false;
                }
                ComposerHostAutosaveAdvance::Saved {
                    dirty_successor: dirty,
                }
            }
            ComposerHostPublicationCompletion::NotCommitted => {
                self.finish_autosave_failure(ComposerHostFlushFailure::NotCommitted)
            }
            ComposerHostPublicationCompletion::CancelledBeforeAdmission => {
                self.finish_autosave_failure(ComposerHostFlushFailure::Cancelled)
            }
            ComposerHostPublicationCompletion::DurableBaseConflict => {
                self.finish_autosave_failure(ComposerHostFlushFailure::DurableBaseConflict)
            }
            ComposerHostPublicationCompletion::SessionDisposed => {
                self.finish_autosave_failure(ComposerHostFlushFailure::SessionDisposed)
            }
            ComposerHostPublicationCompletion::OccupiedIdentityCollision => {
                self.finish_autosave_failure(ComposerHostFlushFailure::IdentityCollision)
            }
            ComposerHostPublicationCompletion::ReconciliationCollision => {
                self.finish_autosave_failure(ComposerHostFlushFailure::ReconciliationCollision)
            }
        }
    }

    fn finish_autosave_failure(
        &mut self,
        failure: ComposerHostFlushFailure,
    ) -> ComposerHostAutosaveAdvance {
        let timer_generation = self
            .lifecycle
            .autosave
            .as_ref()
            .and_then(|save| save.timer_generation);
        self.lifecycle.autosave = None;
        if matches!(
            failure,
            ComposerHostFlushFailure::Cancelled
                | ComposerHostFlushFailure::NotCommitted
                | ComposerHostFlushFailure::Recoverable
        ) && self.lifecycle.dirty_adoption_seen
            && self.is_dirty()
        {
            self.rearm_autosave_generation(timer_generation);
        } else {
            self.lifecycle.timer = None;
        }
        ComposerHostAutosaveAdvance::Unsatisfied(failure)
    }

    fn rearm_autosave_generation(&mut self, captured_generation: Option<u64>) {
        if self.lifecycle.timer.is_some()
            || captured_generation != Some(self.lifecycle.timer_generation)
        {
            return;
        }
        if let Some(binding) = self.binding() {
            let _ = self.lifecycle.arm(binding, Instant::now());
        }
    }
}
