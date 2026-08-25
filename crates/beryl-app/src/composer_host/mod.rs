mod activation;
mod error;
mod history;
mod lifecycle;
mod model;
mod mutation;
mod publication;
mod request;
mod submission;

use std::collections::BTreeMap;
use std::num::NonZeroUsize;

use syndic_storage::{DraftEditorCandidateActivationBindingV1, SyndicStorage};

pub use error::ComposerHostError;
pub use history::{ComposerHostHistoryStatus, ComposerHostRetainedHistoryIntent};
pub use lifecycle::*;
pub use model::*;
use mutation::ComposerHostPendingMutation;
pub use mutation::{
    ComposerHostImageMarkerMetadata, ComposerHostMutationOutcome, ComposerHostMutationStatus,
    ComposerHostRetainedMutationIntent,
};
pub use publication::*;
pub use submission::*;

struct ActiveComposerHost {
    binding: ComposerHostBinding,
    storage_candidate: DraftEditorCandidateActivationBindingV1,
    thread_id: beryl_model::SyndicThreadId,
    initial_responses: Vec<ComposerHostResponse>,
    unavailable: bool,
    durable_selector: syndic_storage::DraftEditorCurrentSelectorV1,
    published_candidate_generation: u64,
    published_pair: syndic_storage::DraftRootHistoryPairV1,
    session_disposed: bool,
}

const DEFAULT_SETTLEMENT_CUSTODY_CAPACITY: usize = 4;

pub struct SyndicComposerHost {
    storage: SyndicStorage,
    active: Option<Box<ActiveComposerHost>>,
    last_generation: Option<ComposerHostGeneration>,
    last_request_id: u64,
    pending: BTreeMap<u64, ComposerHostPendingRequest>,
    pending_mutation: Option<ComposerHostPendingMutation>,
    detached_mutations: Vec<ComposerHostPendingMutation>,
    pending_history: Option<history::ComposerHostPendingHistory>,
    detached_history: Vec<history::ComposerHostPendingHistory>,
    settlement_custody_capacity: usize,
    last_mutation_identity:
        Option<Box<(ComposerHostBinding, mutation::ComposerHostMutationIdentity)>>,
    last_history_identity: Option<Box<(ComposerHostBinding, gpui_text_input::RangeHistoryIntent)>>,
    last_history_outcome: Option<Box<gpui_text_input::RangeHistoryOutcome>>,
    publication: publication::ComposerHostPublicationCoordinator,
    lifecycle: lifecycle::ComposerHostLifecycleCoordinator,
    submission: submission::ComposerHostSubmissionCoordinator,
    #[cfg(feature = "test-faults")]
    activation_after_selector_fault:
        Option<Box<dyn FnOnce(&beryl_home_store::HomeStore, SyndicStorage) + Send>>,
    #[cfg(feature = "test-faults")]
    mutation_before_execute_fault:
        Option<Box<dyn FnOnce(&beryl_home_store::HomeStore, SyndicStorage) + Send>>,
    #[cfg(feature = "test-faults")]
    history_before_execute_fault:
        Option<Box<dyn FnOnce(&beryl_home_store::HomeStore, SyndicStorage) + Send>>,
    #[cfg(feature = "test-faults")]
    history_after_commit_fault:
        Option<Box<dyn FnOnce(&beryl_home_store::HomeStore, SyndicStorage) + Send>>,
    #[cfg(feature = "test-faults")]
    publication_before_execute_fault:
        Option<Box<dyn FnOnce(&beryl_home_store::HomeStore, SyndicStorage) + Send>>,
    #[cfg(feature = "test-faults")]
    submission_before_execute_fault:
        Option<Box<dyn FnOnce(&beryl_home_store::HomeStore, SyndicStorage) + Send>>,
    #[cfg(feature = "test-faults")]
    submission_transition_fault: Option<submission::ComposerHostSubmissionFaultPoint>,
    #[cfg(feature = "test-faults")]
    mutation_transition_limit: usize,
    #[cfg(feature = "test-faults")]
    next_mutation_custody_serial: u64,
}

impl SyndicComposerHost {
    pub const fn new(storage: SyndicStorage) -> Self {
        Self::with_settlement_custody_capacity(
            storage,
            NonZeroUsize::new(DEFAULT_SETTLEMENT_CUSTODY_CAPACITY).unwrap(),
        )
    }

    pub const fn with_settlement_custody_capacity(
        storage: SyndicStorage,
        capacity: NonZeroUsize,
    ) -> Self {
        Self {
            storage,
            active: None,
            last_generation: None,
            last_request_id: 0,
            pending: BTreeMap::new(),
            pending_mutation: None,
            detached_mutations: Vec::new(),
            pending_history: None,
            detached_history: Vec::new(),
            settlement_custody_capacity: capacity.get(),
            last_mutation_identity: None,
            last_history_identity: None,
            last_history_outcome: None,
            publication: publication::ComposerHostPublicationCoordinator::new(),
            lifecycle: lifecycle::ComposerHostLifecycleCoordinator::new(),
            submission: submission::ComposerHostSubmissionCoordinator::new(),
            #[cfg(feature = "test-faults")]
            activation_after_selector_fault: None,
            #[cfg(feature = "test-faults")]
            mutation_before_execute_fault: None,
            #[cfg(feature = "test-faults")]
            history_before_execute_fault: None,
            #[cfg(feature = "test-faults")]
            history_after_commit_fault: None,
            #[cfg(feature = "test-faults")]
            publication_before_execute_fault: None,
            #[cfg(feature = "test-faults")]
            submission_before_execute_fault: None,
            #[cfg(feature = "test-faults")]
            submission_transition_fault: None,
            #[cfg(feature = "test-faults")]
            mutation_transition_limit: mutation::COMPOSER_HOST_MAX_MUTATION_TRANSITIONS,
            #[cfg(feature = "test-faults")]
            next_mutation_custody_serial: 1,
        }
    }

    pub const fn binding(&self) -> Option<ComposerHostBinding> {
        match &self.active {
            Some(active) => Some(active.binding),
            None => None,
        }
    }

    pub fn initial_responses(&self) -> &[ComposerHostResponse] {
        match &self.active {
            Some(active) => &active.initial_responses,
            None => &[],
        }
    }

    pub fn take_initial_responses(&mut self) -> Vec<ComposerHostResponse> {
        match &mut self.active {
            Some(active) => std::mem::take(&mut active.initial_responses),
            None => Vec::new(),
        }
    }

    pub fn pending_request_count(&self) -> usize {
        self.pending.len()
    }

    pub const fn settlement_custody_capacity(&self) -> usize {
        self.settlement_custody_capacity
    }

    pub fn settlement_custody_in_use(&self) -> usize {
        usize::from(self.pending_mutation.is_some())
            + self.detached_mutations.len()
            + usize::from(self.pending_history.is_some())
            + self.detached_history.len()
    }

    fn live_operation_pending(&self) -> bool {
        self.pending_mutation.is_some() || self.pending_history.is_some()
    }

    fn submission_pending(&self) -> bool {
        self.submission.pending.is_some()
    }

    fn reserve_settlement_custody(&self) -> Result<(), ComposerHostError> {
        if self.settlement_custody_in_use() >= self.settlement_custody_capacity {
            Err(ComposerHostError::SettlementCustodyLimit)
        } else {
            Ok(())
        }
    }

    fn mark_active_unavailable(&mut self, binding: ComposerHostBinding) {
        if let Some(active) = self.active.as_mut()
            && active.binding == binding
        {
            active.unavailable = true;
        }
    }

    fn mark_active_session_unavailable(&mut self, binding: ComposerHostBinding) {
        if let Some(active) = self.active.as_mut()
            && publication::same_session(active.binding, binding)
        {
            active.unavailable = true;
        }
    }

    pub fn is_unavailable(&self) -> bool {
        self.active
            .as_ref()
            .is_some_and(|active| active.unavailable)
    }

    #[cfg(feature = "test-faults")]
    pub fn test_arm_activation_after_selector_fault(
        &mut self,
        fault: impl FnOnce(&beryl_home_store::HomeStore, SyndicStorage) + Send + 'static,
    ) {
        assert!(self.activation_after_selector_fault.is_none());
        self.activation_after_selector_fault = Some(Box::new(fault));
    }

    #[cfg(feature = "test-faults")]
    pub fn test_set_mutation_transition_limit(&mut self, limit: usize) {
        self.mutation_transition_limit = limit;
    }

    #[cfg(feature = "test-faults")]
    pub fn test_arm_mutation_before_execute_fault(
        &mut self,
        fault: impl FnOnce(&beryl_home_store::HomeStore, SyndicStorage) + Send + 'static,
    ) {
        assert!(self.mutation_before_execute_fault.is_none());
        self.mutation_before_execute_fault = Some(Box::new(fault));
    }

    #[cfg(feature = "test-faults")]
    pub fn test_arm_history_before_execute_fault(
        &mut self,
        fault: impl FnOnce(&beryl_home_store::HomeStore, SyndicStorage) + Send + 'static,
    ) {
        assert!(self.history_before_execute_fault.is_none());
        self.history_before_execute_fault = Some(Box::new(fault));
    }

    #[cfg(feature = "test-faults")]
    pub fn test_arm_history_after_commit_fault(
        &mut self,
        fault: impl FnOnce(&beryl_home_store::HomeStore, SyndicStorage) + Send + 'static,
    ) {
        assert!(self.history_after_commit_fault.is_none());
        self.history_after_commit_fault = Some(Box::new(fault));
    }

    #[cfg(feature = "test-faults")]
    pub fn test_arm_publication_before_execute_fault(
        &mut self,
        fault: impl FnOnce(&beryl_home_store::HomeStore, SyndicStorage) + Send + 'static,
    ) {
        assert!(self.publication_before_execute_fault.is_none());
        self.publication_before_execute_fault = Some(Box::new(fault));
    }

    #[cfg(feature = "test-faults")]
    pub fn test_arm_submission_before_execute_fault(
        &mut self,
        fault: impl FnOnce(&beryl_home_store::HomeStore, SyndicStorage) + Send + 'static,
    ) {
        assert!(self.submission_before_execute_fault.is_none());
        self.submission_before_execute_fault = Some(Box::new(fault));
    }

    #[cfg(feature = "test-faults")]
    pub fn test_arm_submission_transition_fault(
        &mut self,
        point: ComposerHostSubmissionFaultPoint,
    ) {
        assert!(self.submission_transition_fault.is_none());
        self.submission_transition_fault = Some(point);
    }

    #[cfg(feature = "test-faults")]
    pub fn test_last_settlement_identity_custody_count(&self) -> usize {
        usize::from(self.last_mutation_identity.is_some())
            + usize::from(self.last_history_identity.is_some())
            + usize::from(self.last_history_outcome.is_some())
    }

    fn next_generation(&self) -> Result<ComposerHostGeneration, ComposerHostError> {
        match self.last_generation {
            Some(generation) => generation
                .next()
                .ok_or(ComposerHostError::GenerationExhausted),
            None => Ok(ComposerHostGeneration::FIRST),
        }
    }
}
