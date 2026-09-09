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
        let metadata = projection
            .observed_thread_metadata()?
            .ok_or(OrdinaryTurnExecutionError::BackendDefaultPolicyUnavailable)?;
        let model = metadata
            .model
            .ok_or(OrdinaryTurnExecutionError::BackendDefaultPolicyUnavailable)?;
        let instructions = record
            .as_ref()
            .and_then(|record| record.value().as_developer_instructions())
            .filter(|instructions| !instructions.trim().is_empty())
            .map(str::to_owned);
        Ok(
            TurnStartOptions::default().with_developer_instructions_context(
                instructions,
                model,
                metadata.reasoning_effort,
            ),
        )
    }
}
