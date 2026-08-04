use std::num::NonZeroU64;

use beryl_model::{
    BindingRevision, CasLoadedSessionGeneration, CasLoadedThreadGeneration, CasProcessGeneration,
    CasThreadId, ExecutionBinding, SyndicThreadId,
};

use crate::{
    ProviderControlOrdinal, SyndicConnectionGeneration, SyndicTimestamp, SyndicValueError,
    ThreadUsageRevision,
};

/// Exact nonnegative token counters from one compact CAS usage control.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ThreadTokenUsageBreakdown {
    cached_input_tokens: u64,
    input_tokens: u64,
    output_tokens: u64,
    reasoning_output_tokens: u64,
    total_tokens: u64,
}

impl ThreadTokenUsageBreakdown {
    #[must_use]
    pub const fn new(
        cached_input_tokens: u64,
        input_tokens: u64,
        output_tokens: u64,
        reasoning_output_tokens: u64,
        total_tokens: u64,
    ) -> Self {
        Self {
            cached_input_tokens,
            input_tokens,
            output_tokens,
            reasoning_output_tokens,
            total_tokens,
        }
    }

    #[must_use]
    pub const fn cached_input_tokens(self) -> u64 {
        self.cached_input_tokens
    }
    #[must_use]
    pub const fn input_tokens(self) -> u64 {
        self.input_tokens
    }
    #[must_use]
    pub const fn output_tokens(self) -> u64 {
        self.output_tokens
    }
    #[must_use]
    pub const fn reasoning_output_tokens(self) -> u64 {
        self.reasoning_output_tokens
    }
    #[must_use]
    pub const fn total_tokens(self) -> u64 {
        self.total_tokens
    }
}

/// Latest exact selected-thread usage observation and its complete route provenance.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThreadUsageObservation {
    last: ThreadTokenUsageBreakdown,
    total: ThreadTokenUsageBreakdown,
    model_context_window: Option<NonZeroU64>,
    observed_at: SyndicTimestamp,
    execution: ExecutionBinding,
    binding_revision: BindingRevision,
    cas_thread_id: CasThreadId,
    loaded_generation: CasLoadedSessionGeneration,
    connection_generation: SyndicConnectionGeneration,
    provider_control_ordinal: ProviderControlOrdinal,
}

impl ThreadUsageObservation {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        last: ThreadTokenUsageBreakdown,
        total: ThreadTokenUsageBreakdown,
        model_context_window: Option<u64>,
        observed_at: SyndicTimestamp,
        execution: ExecutionBinding,
        binding_revision: BindingRevision,
        cas_thread_id: CasThreadId,
        loaded_generation: CasLoadedSessionGeneration,
        connection_generation: SyndicConnectionGeneration,
        provider_control_ordinal: ProviderControlOrdinal,
    ) -> Result<Self, SyndicValueError> {
        let model_context_window = model_context_window
            .map(|value| {
                NonZeroU64::new(value).ok_or(SyndicValueError::ZeroPositiveValue {
                    kind: "model context window",
                })
            })
            .transpose()?;
        Ok(Self {
            last,
            total,
            model_context_window,
            observed_at,
            execution,
            binding_revision,
            cas_thread_id,
            loaded_generation,
            connection_generation,
            provider_control_ordinal,
        })
    }

    #[must_use]
    pub const fn last(&self) -> ThreadTokenUsageBreakdown {
        self.last
    }
    #[must_use]
    pub const fn total(&self) -> ThreadTokenUsageBreakdown {
        self.total
    }
    #[must_use]
    pub const fn model_context_window(&self) -> Option<u64> {
        match self.model_context_window {
            Some(value) => Some(value.get()),
            None => None,
        }
    }
    #[must_use]
    pub const fn observed_at(&self) -> SyndicTimestamp {
        self.observed_at
    }
    #[must_use]
    pub const fn execution(&self) -> &ExecutionBinding {
        &self.execution
    }
    #[must_use]
    pub const fn binding_revision(&self) -> BindingRevision {
        self.binding_revision
    }
    #[must_use]
    pub const fn cas_thread_id(&self) -> &CasThreadId {
        &self.cas_thread_id
    }
    #[must_use]
    pub const fn process_generation(&self) -> CasProcessGeneration {
        self.loaded_generation.process()
    }
    #[must_use]
    pub const fn loaded_thread_generation(&self) -> CasLoadedThreadGeneration {
        self.loaded_generation.thread()
    }
    #[must_use]
    pub const fn loaded_generation(&self) -> CasLoadedSessionGeneration {
        self.loaded_generation
    }
    #[must_use]
    pub const fn connection_generation(&self) -> SyndicConnectionGeneration {
        self.connection_generation
    }
    #[must_use]
    pub const fn provider_control_ordinal(&self) -> ProviderControlOrdinal {
        self.provider_control_ordinal
    }
}

/// Independently revisioned latest usage authority for one thread.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThreadUsageRecord {
    thread_id: SyndicThreadId,
    revision: ThreadUsageRevision,
    observation: Option<ThreadUsageObservation>,
}

impl ThreadUsageRecord {
    #[must_use]
    pub const fn empty(thread_id: SyndicThreadId) -> Self {
        Self {
            thread_id,
            revision: ThreadUsageRevision::FIRST,
            observation: None,
        }
    }

    #[must_use]
    pub(crate) const fn from_parts(
        thread_id: SyndicThreadId,
        revision: ThreadUsageRevision,
        observation: Option<ThreadUsageObservation>,
    ) -> Self {
        Self {
            thread_id,
            revision,
            observation,
        }
    }

    #[must_use]
    pub const fn thread_id(&self) -> SyndicThreadId {
        self.thread_id
    }
    #[must_use]
    pub const fn revision(&self) -> ThreadUsageRevision {
        self.revision
    }
    #[must_use]
    pub const fn observation(&self) -> Option<&ThreadUsageObservation> {
        self.observation.as_ref()
    }

    pub(crate) fn publish(
        &self,
        observation: ThreadUsageObservation,
    ) -> Result<Self, SyndicValueError> {
        Ok(Self::from_parts(
            self.thread_id,
            self.revision.checked_next()?,
            Some(observation),
        ))
    }
}
