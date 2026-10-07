use super::*;
use crate::discussion_settlement::DiscussionPreparationFailure;

pub(in crate::cas_projection::process_sessions) enum TargetError {
    Closed,
    Read,
    Changed,
    Configuration,
    Unavailable(DiscussionPreparationFailure),
}

impl PreparationContext {
    pub(super) fn revoke_obsolete_retry(&self, runtime_id: RuntimeId) {
        if let Some((thread_id, binding)) = self.owner.pending_retry(runtime_id)
            && self.launch_spec_for(thread_id, &binding).is_err()
        {
            self.owner.revoke_retry(thread_id, &binding);
        }
    }

    pub(super) fn launch_spec(
        &self,
        admission: &ScheduledOrdinaryAdmission,
    ) -> Result<ManagedBackendLaunchSpec, TargetError> {
        self.launch_spec_for(admission.thread_id(), admission.execution_binding())
    }

    pub(in crate::cas_projection::process_sessions) fn launch_spec_for(
        &self,
        thread_id: SyndicThreadId,
        binding: &ExecutionBinding,
    ) -> Result<ManagedBackendLaunchSpec, TargetError> {
        if !self.commands.is_open() {
            return Err(TargetError::Closed);
        }
        let before = self.home.home_revision().map_err(|_| TargetError::Read)?;
        let result = self.read_launch_spec(thread_id, binding);
        if self.home.home_revision().map_err(|_| TargetError::Read)? != before {
            return Err(TargetError::Changed);
        }
        if !self.commands.is_open() {
            return Err(TargetError::Closed);
        }
        result
    }

    fn read_launch_spec(
        &self,
        thread_id: SyndicThreadId,
        binding: &ExecutionBinding,
    ) -> Result<ManagedBackendLaunchSpec, TargetError> {
        let execution = self
            .storage
            .thread_execution(
                &self.home,
                thread_id,
                crate::cas_projection::input_replay::point_limit(),
            )
            .map_err(|_| TargetError::Read)?
            .ok_or(TargetError::Changed)?;
        if execution.execution() != binding {
            return Err(TargetError::Changed);
        }
        let runtime = self
            .config
            .runtime_roots
            .runtime(&self.home, binding.runtime_id())
            .map_err(|_| TargetError::Read)?
            .ok_or(TargetError::Unavailable(
                DiscussionPreparationFailure::Runtime,
            ))?;
        let root = self
            .config
            .runtime_roots
            .root(&self.home, binding.root_id())
            .map_err(|_| TargetError::Read)?
            .ok_or(TargetError::Unavailable(DiscussionPreparationFailure::Root))?;
        if root.runtime_id() != runtime.runtime_id() || root.canonical_path() != binding.root_path()
        {
            return Err(TargetError::Changed);
        }
        let runtime_token_directory = self
            .config
            .token_directory
            .runtime_path(runtime.mode())
            .ok_or(TargetError::Configuration)?;
        let spec = ManagedBackendLaunchSpec::new(
            runtime.runtime_id(),
            runtime.canonical_executable().clone(),
            runtime.mode().clone(),
            runtime.runtime_native_executable().clone(),
            root.canonical_path().clone(),
            self.config.token_directory.host().clone(),
            runtime_token_directory,
        )
        .map_err(|_| TargetError::Configuration)?;
        Ok(match self.config.wsl_supervisor_artifact.as_ref() {
            Some(artifact) => spec.with_wsl_supervisor_artifact(Arc::clone(artifact)),
            None => spec,
        })
    }
}
