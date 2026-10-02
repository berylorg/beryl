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
        self.configure_runtime_session_preparation_with_access(sessions, config, attention, None)
    }

    pub(super) fn configure_runtime_session_preparation_with_access(
        &self,
        sessions: &ScheduledExecutionSessions,
        config: RuntimeSessionPreparationConfig,
        attention: &Arc<ProcessLifecycleAttentionPool>,
        candidate: Option<&beryl_home_store::HomeCandidateRecoveryAccess<'_>>,
    ) -> Result<(), RuntimeSessionPreparationError> {
        let owner = self
            .runtime_interest
            .as_ref()
            .ok_or(RuntimeSessionPreparationError::ServiceUnavailable)?;
        let home = self
            .home
            .as_ref()
            .ok_or(RuntimeSessionPreparationError::ServiceUnavailable)?;
        if config.policy.thread_options().is_ephemeral() {
            return Err(RuntimeSessionPreparationError::InvalidConfiguration);
        }
        match candidate {
            Some(access) => config.runtime_roots.has_runtimes_candidate(access).map(|_| ()),
            None => config.runtime_roots.revision(home).map(|_| ()),
        }
        .map_err(|_| RuntimeSessionPreparationError::InvalidConfiguration)?;
        match candidate {
            Some(access) => config.assets.revision_candidate(access),
            None => config.assets.revision(home),
        }
        .map_err(|_| RuntimeSessionPreparationError::InvalidConfiguration)?;
        sessions.configure_preparation(PreparationContext {
            config,
            home: Arc::clone(home),
            storage: self.storage.clone(),
            owner: Arc::clone(owner),
            work_sources: self.work_sources(),
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
