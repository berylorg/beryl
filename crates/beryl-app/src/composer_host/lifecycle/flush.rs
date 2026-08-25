use super::*;

impl SyndicComposerHost {
    pub fn begin_flush(
        &mut self,
        purpose: ComposerHostFlushPurpose,
    ) -> Result<ComposerHostFlushAdmission, ComposerHostError> {
        let active = self.active.as_ref().ok_or(ComposerHostError::OldBinding)?;
        if active.unavailable || active.session_disposed || self.lifecycle.service_disposed {
            return Err(ComposerHostError::PublicationUnavailable);
        }
        if let Some(barrier) = self.lifecycle.barrier.as_mut() {
            if purpose.disposes_session() && !barrier.purpose.disposes_session() {
                barrier.purpose = purpose;
            }
            let ticket = barrier.ticket;
            return Ok(ComposerHostFlushAdmission::Joined {
                ticket,
                state: self.flush_state(ticket)?,
            });
        }
        let disposal = match self.publication.lane.as_deref() {
            Some(ComposerHostPublicationLane::Disposal(pending)) => {
                if !purpose.disposes_session() {
                    return Err(ComposerHostError::PublicationPending);
                }
                Some(pending.ticket)
            }
            _ => None,
        };
        let generation = self
            .lifecycle
            .barrier_generation
            .checked_add(1)
            .ok_or(ComposerHostError::GenerationExhausted)?;
        let ticket = ComposerHostFlushTicket {
            home_id: active.binding.home_id(),
            home_generation: active.binding.home_generation(),
            host_generation: active.binding.host_generation(),
            barrier_generation: generation,
        };
        let publication =
            self.lifecycle
                .autosave
                .take()
                .or_else(|| match self.publication.lane.as_deref() {
                    Some(ComposerHostPublicationLane::Publication(pending)) => Some(PendingSave {
                        ticket: pending.ticket,
                        timer_generation: None,
                        failure: None,
                    }),
                    _ => None,
                });
        self.lifecycle.timer = None;
        self.lifecycle.barrier_generation = generation;
        if publication.is_some() {
            self.lifecycle.last_publication_completion = None;
        }
        self.lifecycle.barrier = Some(PendingFlushBarrier {
            ticket,
            purpose,
            publication,
            disposal,
        });
        let state = self.flush_state(ticket)?;
        if !purpose.disposes_session() && state == ComposerHostFlushState::DisposalRequired {
            self.lifecycle.barrier = None;
            return Ok(ComposerHostFlushAdmission::Satisfied(purpose));
        }
        Ok(ComposerHostFlushAdmission::Started { ticket, state })
    }

    pub fn flush_state(
        &self,
        ticket: ComposerHostFlushTicket,
    ) -> Result<ComposerHostFlushState, ComposerHostError> {
        let barrier = self
            .lifecycle
            .barrier
            .as_ref()
            .filter(|barrier| barrier.ticket == ticket)
            .ok_or(ComposerHostError::StalePublicationGeneration)?;
        if barrier.disposal.is_some() || (barrier.publication.is_none() && !self.is_dirty()) {
            Ok(ComposerHostFlushState::DisposalRequired)
        } else if barrier.publication.is_some() {
            Ok(ComposerHostFlushState::PublicationPending)
        } else {
            Ok(ComposerHostFlushState::CaptureRequired)
        }
    }

    pub fn capture_flush_publication(
        &mut self,
        store: &HomeStore,
        flush: ComposerHostFlushTicket,
        assets: AssetState,
        marker_seals: &DraftMarkerSealService,
        operation_id: DraftPieceOperationIdV1,
        marker_authority: Option<ComposerHostMarkerSealAuthority>,
        published_at: SyndicTimestamp,
        cancellation: &CommandCancellation,
    ) -> Result<ComposerHostFlushCapture, ComposerHostError> {
        if !self.lifecycle.barrier_matches(flush) {
            return Ok(ComposerHostFlushCapture::Stale);
        }
        if !self.current_flush_callback(store, flush) {
            return Ok(ComposerHostFlushCapture::Stale);
        }
        let state = self.flush_state(flush)?;
        if state != ComposerHostFlushState::CaptureRequired {
            let purpose = self.lifecycle.barrier.as_ref().unwrap().purpose;
            if state == ComposerHostFlushState::DisposalRequired && !purpose.disposes_session() {
                self.lifecycle.barrier = None;
                return Ok(ComposerHostFlushCapture::Satisfied(purpose));
            }
            return Ok(ComposerHostFlushCapture::State(state));
        }
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
                let failure = self.flush_error_failure(&error);
                self.finish_flush_failure(failure);
                return Ok(ComposerHostFlushCapture::Unsatisfied(failure));
            }
        };
        Ok(match capture {
            ComposerHostPublicationCapture::CleanNoOp => {
                let purpose = self.lifecycle.barrier.as_ref().unwrap().purpose;
                if purpose.disposes_session() {
                    ComposerHostFlushCapture::State(ComposerHostFlushState::DisposalRequired)
                } else {
                    self.lifecycle.barrier = None;
                    ComposerHostFlushCapture::Satisfied(purpose)
                }
            }
            ComposerHostPublicationCapture::CancelledBeforeAdmission => {
                self.finish_flush_failure(ComposerHostFlushFailure::Cancelled);
                ComposerHostFlushCapture::Unsatisfied(ComposerHostFlushFailure::Cancelled)
            }
            ComposerHostPublicationCapture::Captured(ticket) => {
                self.lifecycle.last_publication_completion = None;
                if let Some(barrier) = self.lifecycle.barrier.as_mut() {
                    barrier.publication = Some(PendingSave {
                        ticket,
                        timer_generation: None,
                        failure: None,
                    });
                }
                ComposerHostFlushCapture::Captured(ticket)
            }
        })
    }

    pub fn capture_flush_disposal(
        &mut self,
        store: &HomeStore,
        flush: ComposerHostFlushTicket,
        operation_id: DraftPieceOperationIdV1,
        cancellation: &CommandCancellation,
    ) -> Result<ComposerHostFlushCapture, ComposerHostError> {
        if !self.lifecycle.barrier_matches(flush) {
            return Ok(ComposerHostFlushCapture::Stale);
        }
        if !self.current_flush_callback(store, flush) {
            return Ok(ComposerHostFlushCapture::Stale);
        }
        let purpose = self.lifecycle.barrier.as_ref().unwrap().purpose;
        if !purpose.disposes_session() {
            self.lifecycle.barrier = None;
            return Ok(ComposerHostFlushCapture::Stale);
        }
        if self.flush_state(flush)? != ComposerHostFlushState::DisposalRequired {
            return Ok(ComposerHostFlushCapture::State(self.flush_state(flush)?));
        }
        if self
            .lifecycle
            .barrier
            .as_ref()
            .is_some_and(|barrier| barrier.disposal.is_some())
        {
            return Ok(ComposerHostFlushCapture::State(
                ComposerHostFlushState::DisposalRequired,
            ));
        }
        let ticket = match self.capture_clean_disposal(store, operation_id, cancellation) {
            Ok(ticket) => ticket,
            Err(error) => {
                let failure = self.flush_error_failure(&error);
                self.finish_flush_failure(failure);
                return Ok(ComposerHostFlushCapture::Unsatisfied(failure));
            }
        };
        self.lifecycle.barrier.as_mut().unwrap().disposal = Some(ticket);
        Ok(ComposerHostFlushCapture::State(
            ComposerHostFlushState::DisposalRequired,
        ))
    }

    pub fn advance_flush(
        &mut self,
        store: &HomeStore,
        flush: ComposerHostFlushTicket,
    ) -> Result<ComposerHostFlushAdvance, ComposerHostError> {
        if !self.lifecycle.barrier_matches(flush) {
            return Ok(ComposerHostFlushAdvance::Stale);
        }
        if !self.current_flush_callback(store, flush) {
            return Ok(ComposerHostFlushAdvance::Stale);
        }
        if let Some(disposal) = self
            .lifecycle
            .barrier
            .as_ref()
            .and_then(|barrier| barrier.disposal)
        {
            return self.advance_flush_disposal(store, disposal);
        }
        let publication = self
            .lifecycle
            .barrier
            .as_ref()
            .and_then(|barrier| barrier.publication.as_ref().map(|save| save.ticket));
        let Some(publication) = publication else {
            let state = self.flush_state(flush)?;
            let purpose = self.lifecycle.barrier.as_ref().unwrap().purpose;
            if state == ComposerHostFlushState::DisposalRequired && !purpose.disposes_session() {
                self.lifecycle.barrier = None;
                return Ok(ComposerHostFlushAdvance::Satisfied(purpose));
            }
            return Ok(ComposerHostFlushAdvance::Progress(state));
        };
        match self.advance_publication_step(store, publication) {
            Ok(PublicationStep::Progress) => {
                let failure = self
                    .lifecycle
                    .barrier
                    .as_ref()
                    .and_then(|barrier| barrier.publication.as_ref())
                    .and_then(|save| save.failure);
                if self.publication.lane.is_none()
                    && let Some(failure) = failure
                {
                    return Ok(self.finish_flush_failure(failure));
                }
                Ok(ComposerHostFlushAdvance::Progress(
                    ComposerHostFlushState::PublicationPending,
                ))
            }
            Ok(PublicationStep::Ready) => Ok(ComposerHostFlushAdvance::Progress(
                ComposerHostFlushState::PublicationPending,
            )),
            Ok(PublicationStep::Complete(completion)) => {
                self.lifecycle.last_publication_completion = Some(completion);
                Ok(self.finish_flush_publication(completion))
            }
            Err(error) => {
                if stale_callback_error(&error) {
                    return Ok(ComposerHostFlushAdvance::Stale);
                }
                if let Some(reason) = self.publication_unavailable() {
                    return Ok(self.finish_flush_failure(publication_failure(reason)));
                }
                if !recoverable_error(&error) {
                    return Ok(self.finish_flush_failure(error_failure(&error)));
                }
                if let Some(save) = self
                    .lifecycle
                    .barrier
                    .as_mut()
                    .and_then(|barrier| barrier.publication.as_mut())
                {
                    save.failure = Some(ComposerHostFlushFailure::Recoverable);
                }
                Err(error)
            }
        }
    }

    fn finish_flush_publication(
        &mut self,
        completion: ComposerHostPublicationCompletion,
    ) -> ComposerHostFlushAdvance {
        match completion {
            ComposerHostPublicationCompletion::ReconciliationPending => {
                ComposerHostFlushAdvance::ReconciliationPending
            }
            ComposerHostPublicationCompletion::Published
            | ComposerHostPublicationCompletion::ExactReplay
            | ComposerHostPublicationCompletion::Superseded => {
                if let Some(barrier) = self.lifecycle.barrier.as_mut() {
                    barrier.publication = None;
                }
                if self.is_dirty() {
                    ComposerHostFlushAdvance::Progress(ComposerHostFlushState::CaptureRequired)
                } else {
                    self.lifecycle.dirty_adoption_seen = false;
                    let purpose = self.lifecycle.barrier.as_ref().unwrap().purpose;
                    if purpose.disposes_session() {
                        ComposerHostFlushAdvance::Progress(ComposerHostFlushState::DisposalRequired)
                    } else {
                        self.lifecycle.barrier = None;
                        ComposerHostFlushAdvance::Satisfied(purpose)
                    }
                }
            }
            ComposerHostPublicationCompletion::NotCommitted => {
                self.finish_flush_failure(ComposerHostFlushFailure::NotCommitted)
            }
            ComposerHostPublicationCompletion::CancelledBeforeAdmission => {
                self.finish_flush_failure(ComposerHostFlushFailure::Cancelled)
            }
            ComposerHostPublicationCompletion::DurableBaseConflict => {
                self.finish_flush_failure(ComposerHostFlushFailure::DurableBaseConflict)
            }
            ComposerHostPublicationCompletion::SessionDisposed => {
                self.finish_flush_failure(ComposerHostFlushFailure::SessionDisposed)
            }
            ComposerHostPublicationCompletion::OccupiedIdentityCollision => {
                self.finish_flush_failure(ComposerHostFlushFailure::IdentityCollision)
            }
            ComposerHostPublicationCompletion::ReconciliationCollision => {
                self.finish_flush_failure(ComposerHostFlushFailure::ReconciliationCollision)
            }
        }
    }

    fn finish_flush_failure(
        &mut self,
        failure: ComposerHostFlushFailure,
    ) -> ComposerHostFlushAdvance {
        self.lifecycle.barrier = None;
        if matches!(
            failure,
            ComposerHostFlushFailure::Cancelled
                | ComposerHostFlushFailure::NotCommitted
                | ComposerHostFlushFailure::Recoverable
        ) && self.lifecycle.dirty_adoption_seen
            && self.is_dirty()
        {
            if let Some(binding) = self.binding() {
                let _ = self.lifecycle.arm(binding, Instant::now());
            }
        } else {
            self.lifecycle.timer = None;
        }
        ComposerHostFlushAdvance::Unsatisfied(failure)
    }

    fn advance_flush_disposal(
        &mut self,
        store: &HomeStore,
        ticket: ComposerHostDisposalTicket,
    ) -> Result<ComposerHostFlushAdvance, ComposerHostError> {
        let reconciling = matches!(
            self.publication.lane.as_deref(),
            Some(ComposerHostPublicationLane::Disposal(pending))
                if pending.ticket == ticket && pending.reconciliation.is_some()
        );
        let completion = if reconciling {
            self.reconcile_clean_disposal(store, ticket)
        } else {
            self.execute_clean_disposal(store, ticket)
        };
        let completion = match completion {
            Ok(completion) => completion,
            Err(error) => {
                if stale_callback_error(&error) {
                    return Ok(ComposerHostFlushAdvance::Stale);
                }
                let failure = self.flush_error_failure(&error);
                return Ok(self.finish_flush_failure(failure));
            }
        };
        Ok(match completion {
            ComposerHostDisposalCompletion::ReconciliationPending => {
                ComposerHostFlushAdvance::ReconciliationPending
            }
            ComposerHostDisposalCompletion::Disposed
            | ComposerHostDisposalCompletion::ExactReplay
            | ComposerHostDisposalCompletion::AlreadyDisposed => {
                let purpose = self.lifecycle.barrier.as_ref().unwrap().purpose;
                self.active = None;
                self.pending.clear();
                self.pending_mutation = None;
                self.detached_mutations.clear();
                self.pending_history = None;
                self.detached_history.clear();
                self.last_request_id = 0;
                self.lifecycle.clear_runtime();
                self.lifecycle.dirty_adoption_seen = false;
                ComposerHostFlushAdvance::Satisfied(purpose)
            }
            ComposerHostDisposalCompletion::NotCommitted => {
                self.finish_flush_failure(ComposerHostFlushFailure::NotCommitted)
            }
            ComposerHostDisposalCompletion::CancelledBeforeAdmission => {
                self.finish_flush_failure(ComposerHostFlushFailure::Cancelled)
            }
            ComposerHostDisposalCompletion::DirtyConflict => {
                self.finish_flush_failure(ComposerHostFlushFailure::DisposalDirtyConflict)
            }
            ComposerHostDisposalCompletion::OccupiedIdentityCollision => {
                self.finish_flush_failure(ComposerHostFlushFailure::IdentityCollision)
            }
            ComposerHostDisposalCompletion::ReconciliationCollision => {
                self.finish_flush_failure(ComposerHostFlushFailure::ReconciliationCollision)
            }
        })
    }

    fn flush_error_failure(&self, error: &ComposerHostError) -> ComposerHostFlushFailure {
        self.publication_unavailable()
            .map_or_else(|| error_failure(error), publication_failure)
    }
}
