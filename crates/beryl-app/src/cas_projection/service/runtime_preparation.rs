use super::*;
use crate::{
    cas_projection::{
        RuntimeSessionPreparationConfig, RuntimeSessionPreparationError,
        ScheduledExecutionSessions, process_sessions::preparation::PreparationContext,
    },
    lifecycle_attention::ProcessLifecycleAttentionPool,
};

impl ProjectionConnectionService {
    pub fn configure_runtime_session_preparation(
        &self,
        sessions: &ScheduledExecutionSessions,
        config: RuntimeSessionPreparationConfig,
        attention: &Arc<ProcessLifecycleAttentionPool>,
    ) -> Result<(), RuntimeSessionPreparationError> {
        self.ensure_current()
            .map_err(|_| RuntimeSessionPreparationError::ServiceUnavailable)?;
        let owner = self
            .runtime_interest
            .as_ref()
            .ok_or(RuntimeSessionPreparationError::ServiceUnavailable)?;
        let home = self
            .home
            .as_ref()
            .ok_or(RuntimeSessionPreparationError::ServiceUnavailable)?;
        if config.policy.thread_options().is_ephemeral()
            || config.token_directories.len() > owner.configuration().runtime_capacity()
        {
            return Err(RuntimeSessionPreparationError::InvalidConfiguration);
        }
        for (index, tokens) in config.token_directories.iter().enumerate() {
            let runtime = config
                .runtime_roots
                .runtime(home, tokens.runtime_id)
                .map_err(|_| RuntimeSessionPreparationError::InvalidConfiguration)?
                .ok_or(RuntimeSessionPreparationError::InvalidConfiguration)?;
            if tokens.runtime.mode() != runtime.mode()
                || (matches!(runtime.mode(), beryl_model::RuntimeMode::Host)
                    && tokens.host.as_str() != tokens.runtime.as_str())
                || config.token_directories[..index]
                    .iter()
                    .any(|previous| previous.runtime_id == tokens.runtime_id)
            {
                return Err(RuntimeSessionPreparationError::InvalidConfiguration);
            }
        }
        config
            .assets
            .revision(home)
            .map_err(|_| RuntimeSessionPreparationError::InvalidConfiguration)?;
        sessions.configure_preparation(PreparationContext {
            config,
            home: Arc::clone(home),
            storage: self.storage.clone(),
            owner: Arc::clone(owner),
            admission: self
                .admission_context()
                .map_err(|_| RuntimeSessionPreparationError::ServiceUnavailable)?,
            workers: self.workers.clone(),
            commands: self.command_authorizer.clone(),
            tools: self.ordinary_dynamic_tool_authority(attention),
            timeout: owner.configuration().admission_timeout(),
        })
    }
}
