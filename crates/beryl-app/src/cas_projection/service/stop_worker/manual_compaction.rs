use super::*;
use crate::cas_projection::{ContextCompactionFeedback, ContextCompactionFeedbackState};

#[derive(Clone)]
pub struct ManualCompactionEligibility {
    pub(in crate::cas_projection) inner: Arc<ManualCompactionAuthority>,
}

impl ManualCompactionEligibility {
    pub(crate) fn same_origin(&self, other: &Self) -> bool {
        self.inner.candidate == other.inner.candidate
            && self.inner.claim == other.inner.claim
            && self.inner.publication.ptr_eq(&other.inner.publication)
            && self.inner.connection.ptr_eq(&other.inner.connection)
            && self.inner.projection == other.inner.projection
            && self.inner.coordinator.ptr_eq(&other.inner.coordinator)
    }
}

pub enum ManualCompactionAvailability {
    Eligible(ManualCompactionEligibility),
    Unavailable(&'static str),
}

pub(crate) struct PreparedManualCompaction {
    authority: Option<Arc<ManualCompactionAuthority>>,
    feedback: ContextCompactionFeedback,
}

impl PreparedManualCompaction {
    pub(crate) fn feedback(&self) -> ContextCompactionFeedback {
        self.feedback.clone()
    }

    pub(crate) fn execute(self) -> ContextCompactionFeedback {
        let Some(authority) = self.authority else {
            return self.feedback;
        };
        let result = authority
            .coordinator
            .upgrade()
            .ok_or(crate::cas_projection::ContextCompactionError::Unavailable)
            .and_then(|coordinator| {
                coordinator.compact_selected(
                    authority.clone(),
                    authority.settings.clone(),
                    &self.feedback,
                )
            });
        if let Err(error) = result {
            match error {
                crate::cas_projection::ContextCompactionError::CommandIndeterminate { .. } => {}
                crate::cas_projection::ContextCompactionError::CommandCommitted { .. } => {
                    self.feedback.settle(ContextCompactionFeedbackState::Failed)
                }
                _ => self
                    .feedback
                    .settle(ContextCompactionFeedbackState::Rejected),
            }
        }
        self.feedback
    }
}

pub(in crate::cas_projection) struct ManualCompactionAuthority {
    pub(in crate::cas_projection) candidate: syndic_storage::CompactionAdmissionCandidate,
    pub(in crate::cas_projection) session: beryl_state::SessionState,
    settings: beryl_state::SettingsState,
    pub(in crate::cas_projection) claim: beryl_state::ThreadClaimCatalogSource,
    pub(in crate::cas_projection) publication: Weak<()>,
    pub(in crate::cas_projection) connection: Weak<ProjectionConnection>,
    pub(in crate::cas_projection) projection:
        crate::cas_projection::connection::registry::ManualProjectionStamp,
    coordinator: Weak<crate::cas_projection::context_compaction::ContextCompactionCoordinator>,
    consumed: std::sync::Mutex<
        Option<Weak<crate::cas_projection::context_compaction::feedback::FeedbackRecord>>,
    >,
}

impl ManualCompactionAuthority {
    pub(in crate::cas_projection) fn elect_local(&self) -> bool {
        self.publication.strong_count() != 0
            && self
                .connection
                .upgrade()
                .is_some_and(|connection| connection.manual_projection_current(&self.projection))
    }
}

impl ExactStopWorker {
    #[cfg(feature = "test-faults")]
    pub fn test_selected_compaction_availability(
        &self,
        session: &beryl_state::SessionState,
        window: beryl_model::WindowId,
        selection: beryl_state::WindowClaimSelection,
        publication: Weak<()>,
    ) -> ManualCompactionAvailability {
        self.selected_compaction_availability(session, window, selection, publication)
    }

    #[cfg(feature = "test-faults")]
    pub fn test_reserve_compaction_feedback(
        &self,
    ) -> Result<ContextCompactionFeedback, ExactStopRequestError> {
        self.context_compaction
            .upgrade()
            .ok_or(ExactStopRequestError::Revoked)?
            .reserve_feedback()
    }

    pub(crate) fn selected_compaction_availability(
        &self,
        session: &beryl_state::SessionState,
        window: beryl_model::WindowId,
        selection: beryl_state::WindowClaimSelection,
        publication: Weak<()>,
    ) -> ManualCompactionAvailability {
        use ManualCompactionAvailability as Availability;
        let mut reason = "Runtime is unavailable.";
        let mut prepare = || {
            let _command = self.command_authorizer.authorize().ok()?;
            if publication.strong_count() == 0 {
                return None;
            }
            let read = self.read()?;
            let home = read.home.as_deref()?;
            let revision = home.home_revision().ok()?;
            let coordinator = self.context_compaction.upgrade()?;
            reason = "The selected thread claim is no longer current.";
            let claim = session
                .thread_claim_catalog_source(home, selection.thread_id())
                .ok()?;
            let current = claim.claim()?;
            if current.window_id() != window
                || current.thread_id() != selection.thread_id()
                || current.generation() != selection.generation()
                || current.revision() != selection.revision()
            {
                return None;
            }
            let candidate = match self
                .storage
                .compaction_admission_read(
                    home,
                    selection.thread_id(),
                    SyndicPointReadLimit::new(1_000_000).ok()?,
                )
                .ok()?
            {
                syndic_storage::CompactionAdmissionRead::Admissible(candidate) => *candidate,
                syndic_storage::CompactionAdmissionRead::Existing(_) => {
                    reason = "Context compaction is already in progress.";
                    return None;
                }
                syndic_storage::CompactionAdmissionRead::Ineligible(ineligible) => {
                    reason = match ineligible {
                        syndic_storage::CompactionAdmissionIneligibility::Busy { .. } => "The selected thread is busy.",
                        syndic_storage::CompactionAdmissionIneligibility::AcceptedNextEffective { .. } => "Accepted queued work takes precedence over compaction.",
                        syndic_storage::CompactionAdmissionIneligibility::NoValidBinding { .. } => "The selected thread requires repair before compaction.",
                        syndic_storage::CompactionAdmissionIneligibility::MissingThread => "The selected thread is unavailable.",
                    };
                    return None;
                }
            };
            reason = "Runtime is unavailable.";
            let mut exact = None;
            for connection in read.connections.lock().ok()?.iter() {
                if connection.runtime_id() != candidate.runtime_id() {
                    continue;
                }
                if let Some(projection) = connection.manual_projection(
                    candidate.thread_id(),
                    candidate.cas_thread_id(),
                    candidate.binding_revision(),
                ) {
                    if exact.is_some() {
                        return None;
                    }
                    exact = Some((Arc::downgrade(connection), projection));
                }
            }
            let (connection, projection) = exact?;
            reason = "The exact compaction authority changed; reopen the menu.";
            let authority = ManualCompactionAuthority {
                candidate,
                session: session.clone(),
                settings: beryl_state::BerylState::reacquire(home).ok()?.settings(),
                claim,
                publication,
                connection,
                projection,
                coordinator: Arc::downgrade(&coordinator),
                consumed: std::sync::Mutex::new(None),
            };
            (home.home_revision().ok()? == revision && authority.elect_local()).then_some(
                ManualCompactionEligibility {
                    inner: Arc::new(authority),
                },
            )
        };
        let eligibility = prepare();
        eligibility.map_or(Availability::Unavailable(reason), Availability::Eligible)
    }

    pub fn request_manual_compaction(
        &self,
        eligibility: &ManualCompactionEligibility,
    ) -> Result<ContextCompactionFeedback, ExactStopRequestError> {
        Ok(self.prepare_manual_compaction(eligibility)?.execute())
    }

    pub(crate) fn prepare_manual_compaction(
        &self,
        eligibility: &ManualCompactionEligibility,
    ) -> Result<PreparedManualCompaction, ExactStopRequestError> {
        let _command = self
            .command_authorizer
            .authorize()
            .map_err(|_| ExactStopRequestError::Revoked)?;
        let authority = &eligibility.inner;
        let mut consumed = authority
            .consumed
            .lock()
            .map_err(|_| ExactStopRequestError::Revoked)?;
        if let Some(destination) = consumed.as_ref() {
            return Ok(PreparedManualCompaction {
                authority: None,
                feedback: ContextCompactionFeedback::from_destination(destination)
                    .ok_or(ExactStopRequestError::Revoked)?,
            });
        }
        if !authority.elect_local() {
            return Err(ExactStopRequestError::Revoked);
        }
        let coordinator = self
            .context_compaction
            .upgrade()
            .filter(|coordinator| authority.coordinator.ptr_eq(&Arc::downgrade(coordinator)))
            .ok_or(ExactStopRequestError::Revoked)?;
        let feedback = coordinator.reserve_feedback()?;
        *consumed = Some(feedback.destination());
        drop(consumed);
        Ok(PreparedManualCompaction {
            authority: Some(authority.clone()),
            feedback,
        })
    }
}
