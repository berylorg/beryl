mod activation;
mod error;
mod model;
mod request;

use std::collections::BTreeMap;

use syndic_storage::SyndicStorage;

pub use error::ComposerHostError;
pub use model::*;

struct ActiveComposerHost {
    binding: ComposerHostBinding,
    thread_id: beryl_model::SyndicThreadId,
    initial_responses: Vec<ComposerHostResponse>,
}

pub struct SyndicComposerHost {
    storage: SyndicStorage,
    active: Option<ActiveComposerHost>,
    last_generation: Option<ComposerHostGeneration>,
    last_request_id: u64,
    pending: BTreeMap<u64, ComposerHostPendingRequest>,
    #[cfg(feature = "test-faults")]
    activation_after_selector_fault:
        Option<Box<dyn FnOnce(&beryl_home_store::HomeStore, SyndicStorage) + Send>>,
}

impl SyndicComposerHost {
    pub const fn new(storage: SyndicStorage) -> Self {
        Self {
            storage,
            active: None,
            last_generation: None,
            last_request_id: 0,
            pending: BTreeMap::new(),
            #[cfg(feature = "test-faults")]
            activation_after_selector_fault: None,
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

    pub fn release(&mut self) -> bool {
        let released = self.active.take().is_some();
        self.pending.clear();
        self.last_request_id = 0;
        released
    }

    #[cfg(feature = "test-faults")]
    pub fn test_arm_activation_after_selector_fault(
        &mut self,
        fault: impl FnOnce(&beryl_home_store::HomeStore, SyndicStorage) + Send + 'static,
    ) {
        assert!(self.activation_after_selector_fault.is_none());
        self.activation_after_selector_fault = Some(Box::new(fault));
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
