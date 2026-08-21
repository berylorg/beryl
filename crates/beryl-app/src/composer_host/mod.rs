mod activation;
mod error;
mod model;
mod mutation;
mod request;

use std::collections::BTreeMap;

use syndic_storage::{DraftEditorCandidateActivationBindingV1, SyndicStorage};

pub use error::ComposerHostError;
pub use model::*;
use mutation::ComposerHostPendingMutation;
pub use mutation::{
    ComposerHostImageMarkerMetadata, ComposerHostMutationOutcome, ComposerHostMutationStatus,
    ComposerHostRetainedMutationIntent,
};

struct ActiveComposerHost {
    binding: ComposerHostBinding,
    storage_candidate: DraftEditorCandidateActivationBindingV1,
    thread_id: beryl_model::SyndicThreadId,
    initial_responses: Vec<ComposerHostResponse>,
}

pub struct SyndicComposerHost {
    storage: SyndicStorage,
    active: Option<ActiveComposerHost>,
    last_generation: Option<ComposerHostGeneration>,
    last_request_id: u64,
    pending: BTreeMap<u64, ComposerHostPendingRequest>,
    pending_mutation: Option<ComposerHostPendingMutation>,
    detached_mutation: Option<ComposerHostPendingMutation>,
    last_mutation_identity: Option<(ComposerHostBinding, mutation::ComposerHostMutationIdentity)>,
    #[cfg(feature = "test-faults")]
    activation_after_selector_fault:
        Option<Box<dyn FnOnce(&beryl_home_store::HomeStore, SyndicStorage) + Send>>,
    #[cfg(feature = "test-faults")]
    mutation_before_execute_fault:
        Option<Box<dyn FnOnce(&beryl_home_store::HomeStore, SyndicStorage) + Send>>,
    #[cfg(feature = "test-faults")]
    mutation_transition_limit: usize,
    #[cfg(feature = "test-faults")]
    next_mutation_custody_serial: u64,
}

impl SyndicComposerHost {
    pub const fn new(storage: SyndicStorage) -> Self {
        Self {
            storage,
            active: None,
            last_generation: None,
            last_request_id: 0,
            pending: BTreeMap::new(),
            pending_mutation: None,
            detached_mutation: None,
            last_mutation_identity: None,
            #[cfg(feature = "test-faults")]
            activation_after_selector_fault: None,
            #[cfg(feature = "test-faults")]
            mutation_before_execute_fault: None,
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

    pub fn release(&mut self) -> Result<bool, ComposerHostError> {
        self.detach_pending_mutation()?;
        let released = self.active.take().is_some();
        self.pending.clear();
        self.last_request_id = 0;
        Ok(released)
    }

    fn detach_pending_mutation(&mut self) -> Result<(), ComposerHostError> {
        if matches!(
            self.pending_mutation,
            Some(ComposerHostPendingMutation::Unavailable(_))
        ) {
            self.pending_mutation = None;
            return Ok(());
        }
        if self.pending_mutation.is_none() {
            return Ok(());
        }
        if self.detached_mutation.is_some() {
            return Err(ComposerHostError::MutationPending);
        }
        let mut pending = self.pending_mutation.take().unwrap();
        pending.detach();
        self.detached_mutation = Some(pending);
        Ok(())
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

    fn next_generation(&self) -> Result<ComposerHostGeneration, ComposerHostError> {
        match self.last_generation {
            Some(generation) => generation
                .next()
                .ok_or(ComposerHostError::GenerationExhausted),
            None => Ok(ComposerHostGeneration::FIRST),
        }
    }
}
