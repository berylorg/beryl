use std::{
    ops::Range,
    sync::{
        Arc, Weak,
        atomic::{AtomicBool, Ordering},
    },
};

use beryl_home_store::{
    HomeGeneration, HomeGenerationIdentity, HomeMutationObservation, HomeMutationObserver,
    HomeServiceReference,
};
use beryl_model::{BerylHomeId, SyndicThreadId, WindowId};
use syndic_storage::{SyndicStorage, TranscriptViewHeadRecord};

use crate::{
    cas_projection::ProjectionServiceGeneration,
    syndic_transcript::{PreparedTranscriptActivation, TranscriptActivationPlacement},
};

mod selected_title;
mod source;
pub(crate) use source::prepare_candidate_activation;

pub(crate) const ATTACHMENT_MAX_RECORDS: usize = 64;
pub(crate) const ATTACHMENT_MAX_BYTES: usize = 65_536;

#[derive(Clone, Debug)]
pub(crate) struct TranscriptAttachmentRequest {
    pub(crate) window_id: WindowId,
    pub(crate) host: Weak<()>,
    pub(crate) activation: u64,
    pub(crate) thread_id: SyndicThreadId,
    pub(crate) request_id: u64,
    pub(crate) placement: TranscriptActivationPlacement,
    pub(crate) purpose: TranscriptAttachmentPurpose,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TranscriptAttachmentPurpose {
    Attach,
    Refresh,
}

pub(crate) struct PreparedTranscriptAttachment {
    pub(crate) request: TranscriptAttachmentRequest,
    pub(crate) authority: TranscriptAttachmentAuthority,
    pub(crate) range: Range<u64>,
    activation: PreparedTranscriptActivation,
    title: beryl_state::CatalogResolvedTitle,
    observation: HomeMutationObservation,
    owner: Weak<TranscriptProviderState>,
    _reservation: PendingRead,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum TranscriptAttachmentAuthority {
    Published {
        head: TranscriptViewHeadRecord,
        summary: syndic_storage::HistorySummaryRecord,
    },
    CapacityLimited {
        head: TranscriptViewHeadRecord,
        summary: syndic_storage::HistorySummaryRecord,
    },
    Unpublished {
        thread: syndic_storage::ThreadRecord,
        head: Option<TranscriptViewHeadRecord>,
        summary: Option<syndic_storage::HistorySummaryRecord>,
    },
}

impl TranscriptAttachmentAuthority {
    fn thread_id(&self) -> SyndicThreadId {
        match self {
            Self::Published { head, .. } | Self::CapacityLimited { head, .. } => head.thread_id(),
            Self::Unpublished { thread, .. } => thread.id(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TranscriptAttachmentSourceIdentity {
    authority: TranscriptAttachmentAuthority,
    range: Range<u64>,
    placement: TranscriptActivationPlacement,
    title: beryl_state::CatalogResolvedTitle,
}

impl PreparedTranscriptAttachment {
    pub(crate) fn resolved_title(&self) -> &beryl_state::CatalogResolvedTitle {
        &self.title
    }
    pub(crate) fn source_identity(&self) -> TranscriptAttachmentSourceIdentity {
        TranscriptAttachmentSourceIdentity {
            authority: self.authority.clone(),
            range: self.range.clone(),
            placement: self.request.placement,
            title: self.title.clone(),
        }
    }
}

#[derive(Clone)]
pub(crate) struct TranscriptProviderReader {
    home: Arc<HomeServiceReference>,
    syndic: SyndicStorage,
    lifetime: Weak<()>,
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    home_identity: HomeGenerationIdentity,
    service_generation: ProjectionServiceGeneration,
    observer: HomeMutationObserver,
    state: Arc<TranscriptProviderState>,
}

struct TranscriptProviderState {
    retired: AtomicBool,
    pending: AtomicBool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub(crate) enum TranscriptAttachmentError {
    #[error("transcript provider generation retired")]
    Retired,
    #[error("transcript attachment cancelled")]
    Cancelled,
    #[error("transcript attachment request already pending")]
    Busy,
    #[error("transcript source changed")]
    Stale,
    #[error("transcript source unavailable")]
    Unavailable,
    #[error("transcript source exceeded its bounded page allowance")]
    Capacity,
    #[error("transcript source identity invalid")]
    Identity,
}

impl TranscriptProviderReader {
    pub(crate) fn new(
        home: Arc<HomeServiceReference>,
        syndic: SyndicStorage,
        lifetime: Weak<()>,
        service_generation: ProjectionServiceGeneration,
        observer: HomeMutationObserver,
    ) -> Result<Self, TranscriptAttachmentError> {
        let home_generation = home
            .health()
            .generation()
            .ok_or(TranscriptAttachmentError::Unavailable)?;
        let home_identity = home
            .generation_identity()
            .map_err(|_| TranscriptAttachmentError::Unavailable)?;
        Ok(Self {
            home_id: home.home_id(),
            home_generation,
            home_identity,
            home,
            syndic,
            lifetime,
            service_generation,
            observer,
            state: Arc::new(TranscriptProviderState {
                retired: AtomicBool::new(false),
                pending: AtomicBool::new(false),
            }),
        })
    }

    pub(crate) fn service_identity(
        &self,
    ) -> (BerylHomeId, HomeGeneration, ProjectionServiceGeneration) {
        (self.home_id, self.home_generation, self.service_generation)
    }

    pub(crate) fn prepare_attachment(
        &self,
        request: TranscriptAttachmentRequest,
        cancelled: &AtomicBool,
    ) -> Result<PreparedTranscriptAttachment, TranscriptAttachmentError> {
        self.check_request(&request, cancelled)?;
        self.state
            .pending
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| TranscriptAttachmentError::Busy)?;
        let reservation = PendingRead {
            state: Arc::clone(&self.state),
        };
        self.check_request(&request, cancelled)?;
        if self.home.home_id() != self.home_id
            || self.home.generation_identity().ok() != Some(self.home_identity)
        {
            return Err(TranscriptAttachmentError::Retired);
        }
        let observation = self
            .observer
            .observe()
            .map_err(|_| TranscriptAttachmentError::Stale)?;
        let (authority, range, activation) = self.read_activation(&request, cancelled)?;
        let title = self.prepare_selected_title(&request, cancelled)?;
        self.check_request(&request, cancelled)?;
        self.home
            .try_elect_observed_coherent(&observation, self.home_generation, || ())
            .map_err(|_| TranscriptAttachmentError::Stale)?;
        Ok(PreparedTranscriptAttachment {
            request,
            authority,
            range,
            activation,
            title,
            observation,
            owner: Arc::downgrade(&self.state),
            _reservation: reservation,
        })
    }

    pub(crate) fn publish_if_current<T>(
        &self,
        prepared: PreparedTranscriptAttachment,
        expected: &TranscriptAttachmentRequest,
        cancelled: &AtomicBool,
        publish: impl FnOnce(PreparedTranscriptActivation) -> T,
    ) -> Result<T, TranscriptAttachmentError> {
        self.check_request(expected, cancelled)?;
        if !prepared.owner.ptr_eq(&Arc::downgrade(&self.state))
            || !same_request(&prepared.request, expected)
            || prepared.authority.thread_id() != expected.thread_id
        {
            return Err(TranscriptAttachmentError::Identity);
        }
        self.home
            .try_elect_observed_coherent(&prepared.observation, self.home_generation, || {
                self.check_request(expected, cancelled)?;
                Ok(publish(prepared.activation))
            })
            .map_err(|_| TranscriptAttachmentError::Stale)?
    }

    pub(crate) fn revalidate_after_claim(
        &self,
        mut prepared: PreparedTranscriptAttachment,
        expected: &TranscriptAttachmentRequest,
        cancelled: &AtomicBool,
    ) -> Result<PreparedTranscriptAttachment, TranscriptAttachmentError> {
        self.check_request(expected, cancelled)?;
        if !prepared.owner.ptr_eq(&Arc::downgrade(&self.state))
            || !same_request(&prepared.request, expected)
        {
            return Err(TranscriptAttachmentError::Identity);
        }
        let observation = self
            .observer
            .observe()
            .map_err(|_| TranscriptAttachmentError::Stale)?;
        let (authority, range, activation) = self.read_activation(expected, cancelled)?;
        let title = self.prepare_selected_title(expected, cancelled)?;
        if authority != prepared.authority
            || range != prepared.range
            || activation != prepared.activation
            || title != prepared.title
        {
            return Err(TranscriptAttachmentError::Stale);
        }
        self.home
            .try_elect_observed_coherent(&observation, self.home_generation, || ())
            .map_err(|_| TranscriptAttachmentError::Stale)?;
        self.check_request(expected, cancelled)?;
        prepared.observation = observation;
        Ok(prepared)
    }

    pub(crate) fn retire(&self) -> bool {
        self.state.retired.store(true, Ordering::Release);
        !self.state.pending.load(Ordering::Acquire)
    }

    fn check_request(
        &self,
        request: &TranscriptAttachmentRequest,
        cancelled: &AtomicBool,
    ) -> Result<(), TranscriptAttachmentError> {
        if cancelled.load(Ordering::Acquire) {
            return Err(TranscriptAttachmentError::Cancelled);
        }
        if self.state.retired.load(Ordering::Acquire)
            || self.lifetime.upgrade().is_none()
            || request.host.upgrade().is_none()
        {
            return Err(TranscriptAttachmentError::Retired);
        }
        if request.activation == 0 || request.request_id == 0 {
            return Err(TranscriptAttachmentError::Identity);
        }
        Ok(())
    }
}

fn same_request(left: &TranscriptAttachmentRequest, right: &TranscriptAttachmentRequest) -> bool {
    left.window_id == right.window_id
        && left.host.ptr_eq(&right.host)
        && left.activation == right.activation
        && left.thread_id == right.thread_id
        && left.request_id == right.request_id
        && left.placement == right.placement
        && left.purpose == right.purpose
}

struct PendingRead {
    state: Arc<TranscriptProviderState>,
}

impl Drop for PendingRead {
    fn drop(&mut self) {
        self.state.pending.store(false, Ordering::Release);
    }
}

#[cfg(all(test, feature = "test-faults"))]
mod tests {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/transcript_provider.rs"
    ));
}
