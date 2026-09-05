use std::sync::Arc;

use beryl_state::{PreparedThemeAppearance, ThemeAppearanceSource, ThemeHomeIdentity};

use super::{
    AppearanceCoordinator, AppearanceGeneration, AppearancePublication,
    AppearancePublicationFailure, DurablePublicationError, DurablePublicationIdentity,
    DurablePublicationOutcome, DurableRetryOutcome, PreparedPreviewAppearance,
    PreviewCandidateIdentity, PreviewDiagnostic, PreviewPublicationError,
    PreviewPublicationRequest, PreviewPublicationResult, PreviewSequence, PreviewSource,
    PublicationFailureClass, StalePublicationReason, StopPreviewResult,
};

impl AppearanceCoordinator {
    pub(super) fn begin_durable(
        &mut self,
        identity: DurablePublicationIdentity,
    ) -> Result<super::DurablePublicationRequest, DurablePublicationError> {
        if self.is_publication_thread() {
            return Err(DurablePublicationError::WindowSet(
                AppearancePublicationFailure::Reentrant,
            ));
        }
        if let Some(home) = durable_identity_home(&identity)
            && home != self.home
        {
            self.record_stale(PublicationFailureClass::Stale);
            return Err(DurablePublicationError::Stale(
                StalePublicationReason::ForeignService,
            ));
        }
        let attempt = self
            .last_durable_attempt
            .checked_add(1)
            .ok_or(DurablePublicationError::AttemptExhausted)?;
        self.last_durable_attempt = attempt;
        self.latest_durable_attempt = Some(attempt);
        Ok(super::DurablePublicationRequest {
            attempt,
            home: self.home,
            durable_generation: self.durable.number(),
            current_generation: self.current.number(),
            window_epoch: self.window_epoch(),
            preview_sequence: self.last_preview_sequence,
            identity,
        })
    }

    pub(super) fn begin_preview_request(
        &mut self,
        source: PreviewSource,
        candidate: PreviewCandidateIdentity,
    ) -> Result<PreviewPublicationRequest, PreviewPublicationError> {
        if self.is_publication_thread() {
            return Err(PreviewPublicationError::WindowSet(
                AppearancePublicationFailure::Reentrant,
            ));
        }
        if let PreviewCandidateIdentity::Document(document) = &candidate
            && document.manifest().home() != self.home
        {
            self.last_failure = Some(PublicationFailureClass::CandidateMismatch);
            return Err(PreviewPublicationError::CandidateMismatch);
        }
        let sequence = self
            .next_preview_sequence()
            .map_err(|_| PreviewPublicationError::SequenceExhausted)?;
        self.last_preview_sequence = Some(sequence);
        self.pending_preview = Some(PreviewDiagnostic {
            source: source.kind(),
            sequence,
        });
        Ok(PreviewPublicationRequest {
            home: self.home,
            durable_generation: self.durable.number(),
            current_generation: self.current.number(),
            window_epoch: self.window_epoch(),
            sequence,
            source,
            candidate,
        })
    }

    pub fn publish_durable(
        &mut self,
        request: super::DurablePublicationRequest,
        prepared: PreparedThemeAppearance,
    ) -> Result<DurablePublicationOutcome, DurablePublicationError> {
        self.publish_durable_inner(request, prepared, true)
    }

    pub(super) fn publish_external_durable(
        &mut self,
        request: super::DurablePublicationRequest,
        prepared: PreparedThemeAppearance,
    ) -> Result<DurablePublicationOutcome, DurablePublicationError> {
        self.publish_durable_inner(request, prepared, false)
    }

    fn publish_durable_inner(
        &mut self,
        request: super::DurablePublicationRequest,
        prepared: PreparedThemeAppearance,
        retain_rejected: bool,
    ) -> Result<DurablePublicationOutcome, DurablePublicationError> {
        if self.is_publication_thread() {
            return Err(DurablePublicationError::WindowSet(
                AppearancePublicationFailure::Reentrant,
            ));
        }
        self.validate_durable_request(&request)?;
        if !durable_candidate_matches(&request.identity, &prepared, self.home) {
            self.latest_durable_attempt = None;
            self.last_failure = Some(PublicationFailureClass::CandidateMismatch);
            return Err(DurablePublicationError::CandidateMismatch);
        }

        let generation = self
            .allocate_generation(prepared, AppearancePublication::Durable)
            .map_err(|_| {
                self.last_failure = Some(PublicationFailureClass::GenerationExhausted);
                DurablePublicationError::GenerationExhausted
            })?;

        if self.current.is_preview() && !request.identity.ends_preview() {
            self.validate_durable_request(&request)?;
            self.durable = Arc::clone(&generation);
            self.pending_durable_application = false;
            self.pending_durable_ends_preview = false;
            self.latest_durable_attempt = None;
            self.last_failure = None;
            return Ok(DurablePublicationOutcome::HiddenBaseReplaced(generation));
        }

        self.validate_durable_request(&request)?;

        let preview_end_sequence = if request.identity.ends_preview()
            && (self.current.is_preview() || self.pending_preview.is_some())
        {
            Some(self.next_preview_sequence().map_err(|_| {
                self.last_failure = Some(PublicationFailureClass::PreviewSequenceExhausted);
                DurablePublicationError::PreviewSequenceExhausted
            })?)
        } else {
            None
        };

        if let Err(failure) = self.publish_window_set(request.window_epoch, Arc::clone(&generation))
        {
            if retain_rejected {
                self.durable = generation;
                self.pending_durable_application = true;
                self.pending_durable_ends_preview = request.identity.ends_preview();
            }
            self.latest_durable_attempt = None;
            self.last_failure = Some(failure.into());
            return Err(durable_window_set_error(failure));
        }
        self.durable = Arc::clone(&generation);
        self.current = Arc::clone(&generation);
        self.pending_durable_application = false;
        self.pending_durable_ends_preview = false;
        self.pending_preview = None;
        self.latest_durable_attempt = None;
        if let Some(sequence) = preview_end_sequence {
            self.last_preview_sequence = Some(sequence);
        }
        self.last_failure = None;
        Ok(DurablePublicationOutcome::Published(generation))
    }

    pub fn retry_durable_publication(
        &mut self,
    ) -> Result<DurableRetryOutcome, DurablePublicationError> {
        if self.is_publication_thread() {
            return Err(DurablePublicationError::WindowSet(
                AppearancePublicationFailure::Reentrant,
            ));
        }
        if !self.pending_durable_application {
            return Ok(DurableRetryOutcome::NotPending);
        }
        if self.current.is_preview() && !self.pending_durable_ends_preview {
            self.pending_durable_application = false;
            self.last_failure = None;
            return Ok(DurableRetryOutcome::HiddenBaseRetained(Arc::clone(
                &self.durable,
            )));
        }
        let generation = Arc::clone(&self.durable);
        let preview_end_sequence = if self.pending_durable_ends_preview
            && (self.current.is_preview() || self.pending_preview.is_some())
        {
            Some(self.next_preview_sequence().map_err(|_| {
                self.last_failure = Some(PublicationFailureClass::PreviewSequenceExhausted);
                DurablePublicationError::PreviewSequenceExhausted
            })?)
        } else {
            None
        };
        self.publish_window_set(self.window_epoch(), Arc::clone(&generation))
            .map_err(|failure| {
                self.last_failure = Some(failure.into());
                durable_window_set_error(failure)
            })?;
        self.current = Arc::clone(&generation);
        self.pending_preview = None;
        self.pending_durable_application = false;
        self.pending_durable_ends_preview = false;
        if let Some(sequence) = preview_end_sequence {
            self.last_preview_sequence = Some(sequence);
        }
        self.last_failure = None;
        Ok(DurableRetryOutcome::Published(generation))
    }

    pub fn publish_preview(
        &mut self,
        request: PreviewPublicationRequest,
        completion: PreparedPreviewAppearance,
    ) -> Result<PreviewPublicationResult, PreviewPublicationError> {
        if self.is_publication_thread() {
            return Err(PreviewPublicationError::WindowSet(
                AppearancePublicationFailure::Reentrant,
            ));
        }
        self.validate_preview_request(&request)?;
        if request.candidate != completion.candidate
            || !preview_candidate_matches(&completion.candidate, &completion.prepared, self.home)
        {
            self.finish_failed_preview(
                request.sequence,
                PublicationFailureClass::CandidateMismatch,
            );
            return Err(PreviewPublicationError::CandidateMismatch);
        }
        let publication = AppearancePublication::Preview {
            source: request.source,
            candidate: request.candidate.clone(),
            sequence: request.sequence,
        };
        let generation = self
            .allocate_generation(completion.prepared, publication)
            .map_err(|_| {
                self.finish_failed_preview(
                    request.sequence,
                    PublicationFailureClass::GenerationExhausted,
                );
                PreviewPublicationError::GenerationExhausted
            })?;
        self.validate_preview_request(&request)?;
        self.publish_window_set(request.window_epoch, Arc::clone(&generation))
            .map_err(|failure| {
                self.finish_failed_preview(request.sequence, failure.into());
                preview_window_set_error(failure)
            })?;
        self.current = Arc::clone(&generation);
        self.pending_preview = None;
        self.last_failure = None;
        Ok(PreviewPublicationResult(generation))
    }

    pub fn stop_preview(&mut self) -> Result<StopPreviewResult, PreviewPublicationError> {
        if self.is_publication_thread() {
            return Err(PreviewPublicationError::WindowSet(
                AppearancePublicationFailure::Reentrant,
            ));
        }
        let sequence = self
            .next_preview_sequence()
            .map_err(|_| PreviewPublicationError::SequenceExhausted)?;
        self.last_preview_sequence = Some(sequence);
        self.pending_preview = None;
        if !self.current.is_preview() {
            self.last_failure = None;
            return Ok(StopPreviewResult::AlreadyStopped);
        }

        let restoration = if self.durable.number() > self.current.number() {
            Arc::clone(&self.durable)
        } else {
            self.allocate_generation(
                self.durable.prepared().clone(),
                AppearancePublication::Durable,
            )
            .map_err(|_| {
                self.last_failure = Some(PublicationFailureClass::GenerationExhausted);
                PreviewPublicationError::GenerationExhausted
            })?
        };
        self.publish_window_set(self.window_epoch(), Arc::clone(&restoration))
            .map_err(|failure| {
                self.last_failure = Some(failure.into());
                preview_window_set_error(failure)
            })?;
        self.current = Arc::clone(&restoration);
        self.durable = Arc::clone(&restoration);
        self.pending_durable_application = false;
        self.pending_durable_ends_preview = false;
        self.last_failure = None;
        Ok(StopPreviewResult::Restored(restoration))
    }

    fn validate_durable_request(
        &mut self,
        request: &super::DurablePublicationRequest,
    ) -> Result<(), DurablePublicationError> {
        let reason = if request.home != self.home {
            Some(StalePublicationReason::ForeignService)
        } else if self.latest_durable_attempt != Some(request.attempt) {
            Some(StalePublicationReason::DurableAttempt)
        } else if request.durable_generation != self.durable.number() {
            Some(StalePublicationReason::DurableGeneration)
        } else if request.current_generation != self.current.number() {
            Some(StalePublicationReason::CurrentGeneration)
        } else if request.window_epoch != self.window_epoch() {
            Some(StalePublicationReason::WindowSetEpoch)
        } else if request.preview_sequence != self.last_preview_sequence {
            Some(StalePublicationReason::PreviewSequence)
        } else {
            None
        };
        if let Some(reason) = reason {
            self.record_stale(PublicationFailureClass::Stale);
            return Err(DurablePublicationError::Stale(reason));
        }
        Ok(())
    }

    fn validate_preview_request(
        &mut self,
        request: &PreviewPublicationRequest,
    ) -> Result<(), PreviewPublicationError> {
        let reason = if request.home != self.home {
            Some(StalePublicationReason::ForeignService)
        } else if request.durable_generation != self.durable.number() {
            Some(StalePublicationReason::DurableGeneration)
        } else if request.current_generation != self.current.number() {
            Some(StalePublicationReason::CurrentGeneration)
        } else if request.window_epoch != self.window_epoch() {
            Some(StalePublicationReason::WindowSetEpoch)
        } else if self.last_preview_sequence != Some(request.sequence)
            || self.pending_preview.map(|pending| pending.sequence()) != Some(request.sequence)
        {
            Some(StalePublicationReason::PreviewSequence)
        } else {
            None
        };
        if let Some(reason) = reason {
            self.record_stale(PublicationFailureClass::Stale);
            if self.pending_preview.map(|pending| pending.sequence()) == Some(request.sequence) {
                self.pending_preview = None;
            }
            return Err(PreviewPublicationError::Stale(reason));
        }
        Ok(())
    }

    fn allocate_generation(
        &mut self,
        prepared: PreparedThemeAppearance,
        publication: AppearancePublication,
    ) -> Result<Arc<AppearanceGeneration>, super::GenerationExhausted> {
        let number = self.last_generation.checked_next()?;
        self.last_generation = number;
        Ok(Arc::new(AppearanceGeneration::new(
            number,
            prepared,
            publication,
        )))
    }

    fn next_preview_sequence(&self) -> Result<PreviewSequence, super::PreviewSequenceExhausted> {
        self.last_preview_sequence.map_or(
            Ok(PreviewSequence::initial()),
            PreviewSequence::checked_next,
        )
    }

    fn publish_window_set(
        &self,
        epoch: super::WindowSetEpoch,
        generation: Arc<AppearanceGeneration>,
    ) -> Result<(), AppearancePublicationFailure> {
        if let Some(target) = &self.publication_target {
            target.publish(epoch, Arc::clone(&self.current), generation)?;
        }
        Ok(())
    }

    fn finish_failed_preview(
        &mut self,
        sequence: PreviewSequence,
        failure: PublicationFailureClass,
    ) {
        if self.pending_preview.map(|pending| pending.sequence()) == Some(sequence) {
            self.pending_preview = None;
        }
        self.last_failure = Some(failure);
    }

    fn record_stale(&mut self, failure: PublicationFailureClass) {
        self.stale_rejections = self.stale_rejections.saturating_add(1);
        self.last_failure = Some(failure);
    }
}

fn durable_window_set_error(failure: AppearancePublicationFailure) -> DurablePublicationError {
    match failure {
        AppearancePublicationFailure::Adapter { adapter, class } => {
            DurablePublicationError::Adapter { adapter, class }
        }
        AppearancePublicationFailure::Stale(reason) => DurablePublicationError::Stale(reason),
        failure => DurablePublicationError::WindowSet(failure),
    }
}

fn preview_window_set_error(failure: AppearancePublicationFailure) -> PreviewPublicationError {
    match failure {
        AppearancePublicationFailure::Adapter { adapter, class } => {
            PreviewPublicationError::Adapter { adapter, class }
        }
        AppearancePublicationFailure::Stale(reason) => PreviewPublicationError::Stale(reason),
        failure => PreviewPublicationError::WindowSet(failure),
    }
}

fn durable_identity_home(identity: &DurablePublicationIdentity) -> Option<ThemeHomeIdentity> {
    match identity {
        DurablePublicationIdentity::ActiveDocument(document) => Some(document.manifest().home()),
        DurablePublicationIdentity::RepositoryRefresh(ThemeAppearanceSource::Installed(
            document,
        )) => Some(document.manifest().home()),
        DurablePublicationIdentity::RepositoryRefresh(ThemeAppearanceSource::BuiltinFallback(
            _,
        )) => None,
        DurablePublicationIdentity::Settings { committed, .. } => Some(committed.home()),
    }
}

fn durable_candidate_matches(
    identity: &DurablePublicationIdentity,
    prepared: &PreparedThemeAppearance,
    home: ThemeHomeIdentity,
) -> bool {
    if prepared.home() != home {
        return false;
    }
    match identity {
        DurablePublicationIdentity::ActiveDocument(document) => {
            prepared.source() == &ThemeAppearanceSource::Installed(document.clone())
        }
        DurablePublicationIdentity::RepositoryRefresh(source) => prepared.source() == source,
        DurablePublicationIdentity::Settings { committed, .. } => prepared.settings() == *committed,
    }
}

fn preview_candidate_matches(
    identity: &PreviewCandidateIdentity,
    prepared: &PreparedThemeAppearance,
    home: ThemeHomeIdentity,
) -> bool {
    if prepared.home() != home {
        return false;
    }
    match identity {
        PreviewCandidateIdentity::Document(document) => {
            prepared.source() == &ThemeAppearanceSource::Installed(document.clone())
        }
        PreviewCandidateIdentity::Draft { .. } | PreviewCandidateIdentity::Digest(_) => true,
    }
}
