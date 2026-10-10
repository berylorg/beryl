use std::{fmt, sync::Arc, time::Duration};

use beryl_backend::TurnStartOptions;
use beryl_home_store::HomeStore;
use beryl_state::{SettingKey, SettingsState};

use super::OrdinaryTurnExecutionRequest;
use crate::cas_projection::{LoadedCasProjection, OrdinaryTurnExecutionError};

#[derive(Clone)]
pub(super) struct BackendDefaultSettings(Arc<SettingsState>);

impl fmt::Debug for BackendDefaultSettings {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("BackendDefaultSettings")
    }
}

impl PartialEq for BackendDefaultSettings {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for BackendDefaultSettings {}

impl OrdinaryTurnExecutionRequest {
    pub(in crate::cas_projection) fn with_pending_model_choice(
        mut self,
        pending: crate::cas_projection::process_sessions::model_selection::PendingThreadModelChoice,
    ) -> Self {
        let instructions = self
            .start_options
            .developer_instructions_context()
            .map(|context| context.developer_instructions().map(str::to_owned));
        self.start_options = self.start_options.with_model(pending.choice.model.as_str());
        self.start_options = self.start_options.with_reasoning_effort(
            pending
                .choice
                .reasoning
                .map(crate::cas_projection::process_sessions::model_selection::reasoning_wire)
                .unwrap_or(""),
        );
        if let Some(instructions) = instructions {
            self.start_options = self.start_options.with_developer_instructions_context(
                instructions,
                pending.choice.model.as_str(),
                pending.choice.reasoning.map(|effort| {
                    crate::cas_projection::process_sessions::model_selection::reasoning_wire(effort)
                        .to_owned()
                }),
            );
        }
        self.pending_model_choice = Some(pending);
        self
    }

    pub(in crate::cas_projection) fn accept_model_choice(&self) {
        if let Some(pending) = &self.pending_model_choice {
            pending.accepted();
        }
    }

    pub fn backend_defaults(settings: SettingsState, request_timeout: Duration) -> Self {
        let mut request = Self::new(TurnStartOptions::default(), request_timeout);
        request.context_compaction_timeout =
            crate::cas_projection::ContextCompactionTimeoutPolicy::applied_settings(
                settings.clone(),
            );
        request.backend_default_settings = Some(BackendDefaultSettings(Arc::new(settings)));
        request
    }

    pub(in crate::cas_projection::ordinary) fn resolve_start_options(
        &self,
        store: &HomeStore,
        projection: &LoadedCasProjection,
    ) -> Result<TurnStartOptions, OrdinaryTurnExecutionError> {
        let Some(settings) = &self.backend_default_settings else {
            if self.start_options.model().is_some()
                || self.start_options.reasoning_effort().is_some()
                || self
                    .start_options
                    .developer_instructions_context()
                    .is_some()
            {
                projection.invalidate_observed_thread_metadata()?;
            }
            return Ok(self.start_options.clone());
        };
        let record = settings
            .0
            .setting(store, SettingKey::DeveloperInstructions)?;
        let metadata = projection.observed_thread_metadata()?;
        let (model, reasoning) = if let Some(pending) = &self.pending_model_choice {
            let model = pending.choice.model.as_str().to_owned();
            let reasoning = pending.choice.reasoning.map(|effort| {
                crate::cas_projection::process_sessions::model_selection::reasoning_wire(effort)
                    .to_owned()
            });
            projection.invalidate_observed_thread_metadata()?;
            (model, reasoning)
        } else {
            let metadata =
                metadata.ok_or(OrdinaryTurnExecutionError::BackendDefaultPolicyUnavailable)?;
            let model = metadata
                .model
                .ok_or(OrdinaryTurnExecutionError::BackendDefaultPolicyUnavailable)?;
            (model, metadata.reasoning_effort)
        };
        let instructions = record
            .as_ref()
            .and_then(|record| record.value().as_developer_instructions())
            .filter(|instructions| !instructions.trim().is_empty())
            .map(str::to_owned);
        Ok(self
            .start_options
            .clone()
            .with_developer_instructions_context(instructions, model, reasoning))
    }
}
