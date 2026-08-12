use std::sync::Arc;

use beryl_state::{PreparedThemeAppearance, ThemeAppearanceSource, ThemeHomeIdentity};

use super::{
    AdapterFailureClass, AppearanceCoordinator, AppearanceGeneration, AppearancePublication,
    DurablePublicationError, DurablePublicationIdentity, DurablePublicationOutcome,
    DurableRetryOutcome, PreparedPreviewAppearance, PreparedWindowAppearance,
    PreviewCandidateIdentity, PreviewDiagnostic, PreviewPublicationError,
    PreviewPublicationRequest, PreviewPublicationResult, PreviewSequence, PreviewSource,
    PublicationFailureClass, StalePublicationReason, StopPreviewResult,
};

impl AppearanceCoordinator {
    pub(super) fn begin_durable(
        &mut self,
        identity: DurablePublicationIdentity,
    ) -> Result<super::DurablePublicationRequest, DurablePublicationError> {
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
            window_epoch: self.window_epoch,
            preview_sequence: self.last_preview_sequence,
            identity,
        })
    }

    pub(super) fn begin_preview_request(
        &mut self,
        source: PreviewSource,
        candidate: PreviewCandidateIdentity,
    ) -> Result<PreviewPublicationRequest, PreviewPublicationError> {
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
            window_epoch: self.window_epoch,
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

        let adapter_publications = match self.prepare_adapters(Arc::clone(&generation)) {
            Ok(publications) => publications,
            Err((adapter, class)) => {
                if retain_rejected {
                    self.durable = generation;
                    self.pending_durable_application = true;
                    self.pending_durable_ends_preview = request.identity.ends_preview();
                }
                self.latest_durable_attempt = None;
                self.last_failure = Some(class.into());
                return Err(DurablePublicationError::Adapter { adapter, class });
            }
        };
        self.validate_durable_request(&request)?;
        self.durable = Arc::clone(&generation);
        self.pending_durable_application = true;
        self.pending_durable_ends_preview = request.identity.ends_preview();

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

        commit_adapters(adapter_publications);
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
        let adapter_publications =
            self.prepare_adapters(Arc::clone(&generation))
                .map_err(|(adapter, class)| {
                    self.last_failure = Some(class.into());
                    DurablePublicationError::Adapter { adapter, class }
                })?;
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
        commit_adapters(adapter_publications);
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
        let adapter_publications =
            self.prepare_adapters(Arc::clone(&generation))
                .map_err(|(adapter, class)| {
                    self.finish_failed_preview(request.sequence, class.into());
                    PreviewPublicationError::Adapter { adapter, class }
                })?;
        self.validate_preview_request(&request)?;
        commit_adapters(adapter_publications);
        self.current = Arc::clone(&generation);
        self.pending_preview = None;
        self.last_failure = None;
        Ok(PreviewPublicationResult(generation))
    }

    pub fn stop_preview(&mut self) -> Result<StopPreviewResult, PreviewPublicationError> {
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
        let adapter_publications =
            self.prepare_adapters(Arc::clone(&restoration))
                .map_err(|(adapter, class)| {
                    self.last_failure = Some(class.into());
                    PreviewPublicationError::Adapter { adapter, class }
                })?;
        commit_adapters(adapter_publications);
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
        } else if request.window_epoch != self.window_epoch {
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
        } else if request.window_epoch != self.window_epoch {
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

    fn prepare_adapters(
        &self,
        generation: Arc<AppearanceGeneration>,
    ) -> Result<Vec<Box<dyn PreparedWindowAppearance>>, (super::WindowAdapterId, AdapterFailureClass)>
    {
        let mut prepared = Vec::with_capacity(self.adapters.len());
        for adapter in &self.adapters {
            match adapter.prepare(Arc::clone(&generation)) {
                Ok(publication) => prepared.push(publication),
                Err(class) => return Err((adapter.id(), class)),
            }
        }
        Ok(prepared)
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

fn commit_adapters(publications: Vec<Box<dyn PreparedWindowAppearance>>) {
    for publication in publications {
        publication.commit();
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
