use std::{
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use beryl_backend::ThreadLoadOptions;
use beryl_model::{BindingRevision, CasLoadedSessionGeneration, CasThreadId};
use syndic_storage::{CasLineageProof, SyndicPointReadLimit, SyndicTimestamp};

use crate::cas_projection::{
    CasProjectionCoordinator, CasProjectionRequest, LoadedCasProjection, ProjectionExecutionError,
    connection::LoadedProjectionLease,
};

const POINT_READ_LIMIT: usize = 1_000_000;

pub(super) fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(POINT_READ_LIMIT).expect("projection point-read bound is nonzero")
}

pub(super) fn completion_timestamp() -> Result<SyndicTimestamp, ProjectionExecutionError> {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(ProjectionExecutionError::SystemClockBeforeUnixEpoch)?;
    let unix_millis = u64::try_from(elapsed.as_millis())
        .map_err(|_| ProjectionExecutionError::SystemClockOutOfRange)?;
    Ok(SyndicTimestamp::from_unix_millis(unix_millis))
}

pub(super) fn recovered_generation_matches(
    lineage: CasLineageProof,
    generation: CasLoadedSessionGeneration,
) -> bool {
    match lineage.recovered_loaded_generation() {
        Some(required) => required == generation,
        None => true,
    }
}

pub(super) fn thread_load_options(request: &CasProjectionRequest) -> ThreadLoadOptions {
    let mut options = ThreadLoadOptions::for_root(PathBuf::from(
        request.execution_binding().root_path().as_str(),
    ));
    if let Some(model) = request.thread_options().model() {
        options = options.with_model(model);
    }
    if let Some(provider) = request.thread_options().model_provider() {
        options = options.with_model_provider(provider);
    }
    if let Some(instructions) = request.thread_options().developer_instructions() {
        options = options.with_developer_instructions(instructions);
    }
    if let Some(policy) = request.thread_options().approval_policy() {
        options = options.with_approval_policy(policy);
    }
    if let Some(sandbox) = request.thread_options().sandbox() {
        options = options.with_sandbox(sandbox);
    }
    options
}

impl CasProjectionCoordinator {
    pub(super) fn capability(
        &self,
        request: &CasProjectionRequest,
        binding_revision: BindingRevision,
        cas_thread_id: CasThreadId,
        lease: LoadedProjectionLease,
        lineage: CasLineageProof,
    ) -> LoadedCasProjection {
        LoadedCasProjection::new(
            self,
            request.thread_id(),
            binding_revision,
            request.execution_binding().clone(),
            cas_thread_id,
            lease,
            lineage,
        )
    }
}
