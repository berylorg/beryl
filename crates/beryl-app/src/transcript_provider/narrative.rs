use super::*;
use crate::syndic_transcript::{NarrativePreparationResponse, NarrativeTranscriptDemand};

#[derive(Clone, Debug)]
pub(crate) struct TranscriptNarrativeRequest {
    pub(crate) attachment: TranscriptAttachmentRequest,
    pub(crate) source: TranscriptAttachmentSourceIdentity,
    pub(crate) demand: NarrativeTranscriptDemand,
    pub(crate) allowance: usize,
}

pub(crate) struct PreparedTranscriptNarrative {
    request: TranscriptNarrativeRequest,
    response: NarrativePreparationResponse,
    observation: HomeMutationObservation,
    owner: Weak<TranscriptProviderState>,
    _reservation: PendingRead,
}

impl PreparedTranscriptNarrative {
    pub(crate) fn response(&self) -> NarrativePreparationResponse {
        self.response.clone()
    }
}

impl TranscriptProviderReader {
    pub(crate) fn narrative_source_current(
        &self,
        source: &TranscriptAttachmentSourceIdentity,
    ) -> bool {
        let Ok(published) = self.state.published.lock() else {
            return false;
        };
        published.as_ref().is_some_and(|(authority, observation)| {
            *authority == source.authority
                && self
                    .home
                    .try_elect_observed_coherent(observation, self.home_generation, || ())
                    .is_ok()
        })
    }
    fn check_narrative_request(
        &self,
        request: &TranscriptNarrativeRequest,
        cancelled: &AtomicBool,
    ) -> Result<(), TranscriptAttachmentError> {
        self.check_request(&request.attachment, cancelled)?;
        if self.home.home_id() != self.home_id
            || self.home.generation_identity().ok() != Some(self.home_identity)
        {
            return Err(TranscriptAttachmentError::Retired);
        }
        if request.attachment.purpose != TranscriptAttachmentPurpose::Narrative
            || request.source.authority.thread_id() != request.attachment.thread_id
            || request.demand.view_id.0 != request.attachment.thread_id.to_string()
            || request.allowance > ATTACHMENT_MAX_BYTES
            || !matches!(
                request.demand.source.as_ref(),
                syndic_storage::ProjectionPayload::InlineMarkdown { .. }
            )
        {
            return Err(TranscriptAttachmentError::Identity);
        }
        Ok(())
    }

    pub(crate) fn prepare_narrative(
        &self,
        request: TranscriptNarrativeRequest,
        cancelled: &AtomicBool,
    ) -> Result<PreparedTranscriptNarrative, TranscriptAttachmentError> {
        self.check_narrative_request(&request, cancelled)?;
        self.state
            .pending
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| TranscriptAttachmentError::Busy)?;
        let reservation = PendingRead {
            state: self.state.clone(),
        };
        let observation = {
            let published = self
                .state
                .published
                .lock()
                .map_err(|_| TranscriptAttachmentError::Unavailable)?;
            let (authority, observation) =
                published.as_ref().ok_or(TranscriptAttachmentError::Stale)?;
            if *authority != request.source.authority {
                return Err(TranscriptAttachmentError::Stale);
            }
            observation.clone()
        };
        self.home
            .try_elect_observed_coherent(&observation, self.home_generation, || ())
            .map_err(|_| TranscriptAttachmentError::Stale)?;
        let source = &request.demand.source;
        let preparation_allowance = request
            .allowance
            .saturating_sub(2 * std::mem::size_of::<usize>());
        let response =
            if crate::syndic_transcript::narrative_source_bytes(source) > preparation_allowance {
                NarrativePreparationResponse::Unavailable {
                    source: source.clone(),
                    reason: syndic_storage::narrative::NarrativePreparationErrorKind::Capacity,
                }
            } else {
                match syndic_storage::narrative::prepare_narrative(
                    source.as_ref().clone(),
                    preparation_allowance,
                ) {
                    Ok(prepared) => NarrativePreparationResponse::Ready(Arc::new(prepared)),
                    Err(error) => {
                        let reason = error.kind();
                        drop(error);
                        NarrativePreparationResponse::Unavailable {
                            source: source.clone(),
                            reason,
                        }
                    }
                }
            };
        self.check_narrative_request(&request, cancelled)?;
        self.home
            .try_elect_observed_coherent(&observation, self.home_generation, || ())
            .map_err(|_| TranscriptAttachmentError::Stale)?;
        Ok(PreparedTranscriptNarrative {
            request,
            response,
            observation,
            owner: Arc::downgrade(&self.state),
            _reservation: reservation,
        })
    }

    pub(crate) fn publish_narrative_if_current<T>(
        &self,
        prepared: PreparedTranscriptNarrative,
        expected: &TranscriptNarrativeRequest,
        cancelled: &AtomicBool,
        publish: impl FnOnce(NarrativePreparationResponse) -> T,
    ) -> Result<T, TranscriptAttachmentError> {
        self.check_narrative_request(expected, cancelled)?;
        if !prepared.owner.ptr_eq(&Arc::downgrade(&self.state))
            || !same_request(&prepared.request.attachment, &expected.attachment)
            || prepared.request.source != expected.source
            || prepared.request.demand != expected.demand
            || prepared.request.allowance != expected.allowance
        {
            return Err(TranscriptAttachmentError::Identity);
        }
        self.home
            .try_elect_observed_coherent(&prepared.observation, self.home_generation, || {
                self.check_request(&expected.attachment, cancelled)?;
                Ok(publish(prepared.response))
            })
            .map_err(|_| TranscriptAttachmentError::Stale)?
    }
}
