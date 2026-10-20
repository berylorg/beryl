use std::path::Path;

use beryl_home_store::HomeStore;
use syndic_storage::{
    NativeProjectionBasis, RecoveryAssembly, RecoveryProjectionRequest, SyndicStorage,
};

use crate::cas_projection::{
    AdmittedProjectionSession, CasProjectionCoordinator, CasProjectionRequest, LoadedCasProjection,
    ProjectionCancellationToken, ProjectionExecutionError,
};

pub(super) mod source;

impl CasProjectionCoordinator {
    pub(super) fn recover_projection(
        &self,
        home: &HomeStore,
        storage: &SyndicStorage,
        session: &mut AdmittedProjectionSession,
        request: &CasProjectionRequest,
        cancellation: &ProjectionCancellationToken,
        basis: NativeProjectionBasis,
    ) -> Result<LoadedCasProjection, ProjectionExecutionError> {
        let assembly = storage.prepare_recovery_projection_cancellable(
            home,
            RecoveryProjectionRequest::for_pending_selected_turn_parent(
                request.thread_id(),
                request.selected_path(),
                request.model_context_window_tokens(),
            )
            .with_tool_profile(basis.tool_profile()),
            &|| cancellation.is_cancelled(),
        )?;
        let RecoveryAssembly::Ready(projection) = assembly else {
            return Err(ProjectionExecutionError::UnexpectedNativeEmptyRecovery);
        };
        let prepared = self.prepare_projection_injection(session, projection)?;
        if cancellation.is_cancelled() {
            return Err(ProjectionExecutionError::Cancelled);
        }

        let root_path = request.execution_binding().root_path().as_str().to_owned();
        let thread_options = request.thread_options().clone();
        let timeout = request.timeout();
        let fresh = session.call(move |backend| {
            backend.start_thread_with_options(Path::new(&root_path), thread_options, timeout)
        })?;
        self.inject_projection_target(
            home,
            storage,
            session,
            request,
            cancellation,
            basis,
            fresh,
            prepared,
            super::injection::ProjectionInjectionEstablishment::Replay,
        )
    }
}
