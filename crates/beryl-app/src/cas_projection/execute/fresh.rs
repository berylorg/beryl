use std::path::Path;

use beryl_backend::{FreshLoadedThreadSession, ThreadStatus};
use beryl_home_store::HomeStore;
use beryl_model::CasNativeTurnCount;
use syndic_storage::{
    CasLineageProof, NativeCasLineage, NativeProjectionBasis, PublishValidBinding, SyndicStorage,
};

use super::{cleanup::StaleObservation, point_limit};
use crate::cas_projection::{
    AdmittedProjectionSession, CasProjectionCoordinator, CasProjectionRequest, LoadedCasProjection,
    ProjectionCancellationToken, ProjectionExecutionError, publication,
};

impl CasProjectionCoordinator {
    pub(super) fn start_fresh_native(
        &self,
        home: &HomeStore,
        storage: &SyndicStorage,
        session: &mut AdmittedProjectionSession,
        request: &CasProjectionRequest,
        cancellation: &ProjectionCancellationToken,
        basis: NativeProjectionBasis,
    ) -> Result<LoadedCasProjection, ProjectionExecutionError> {
        let lineage = CasLineageProof::native(NativeCasLineage::Fresh, basis.represented_prefix())?;
        if cancellation.is_cancelled() {
            return Err(ProjectionExecutionError::Cancelled);
        }
        let root_path = request.execution_binding().root_path().as_str().to_owned();
        let thread_options = request.thread_options().clone();
        let timeout = request.timeout();
        let fresh = session.call(move |backend| {
            backend.start_thread_with_options(Path::new(&root_path), thread_options, timeout)
        })?;
        self.publish_fresh_native_target(
            home,
            storage,
            session,
            request,
            cancellation,
            basis,
            fresh,
            CasNativeTurnCount::ZERO,
            lineage,
            "fresh CAS thread could not be published",
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn publish_fresh_native_target(
        &self,
        home: &HomeStore,
        storage: &SyndicStorage,
        session: &AdmittedProjectionSession,
        request: &CasProjectionRequest,
        cancellation: &ProjectionCancellationToken,
        basis: NativeProjectionBasis,
        fresh: FreshLoadedThreadSession,
        native_turn_count: CasNativeTurnCount,
        lineage: CasLineageProof,
        stale_reason: &'static str,
    ) -> Result<LoadedCasProjection, ProjectionExecutionError> {
        let cas_thread_id = fresh.thread_id().clone();
        let lease = match session.register_loaded(
            cas_thread_id.clone(),
            fresh.metadata().clone(),
            request.thread_id(),
            request.timeout(),
        ) {
            Ok(lease) => lease,
            Err(error) => {
                session.invalidate_connection();
                let primary = ProjectionExecutionError::Coordinator(error);
                return Err(self.abandon_projection_target(
                    home,
                    storage,
                    session,
                    request,
                    basis,
                    cas_thread_id,
                    StaleObservation::unknown(None),
                    "new CAS thread could not enter the loaded registry",
                    primary,
                    None,
                ));
            }
        };
        if fresh.status() != &ThreadStatus::Idle {
            let primary = ProjectionExecutionError::ProjectionThreadNotIdle {
                thread_id: cas_thread_id.clone(),
                status: fresh.status().clone(),
            };
            return Err(self.abandon_projection_target(
                home,
                storage,
                session,
                request,
                basis,
                cas_thread_id,
                StaleObservation::exact(
                    request.execution_binding().clone(),
                    basis.represented_prefix(),
                    basis.tool_profile(),
                    lineage,
                    native_turn_count,
                    Some(lease.generation()),
                ),
                "new CAS thread was not idle",
                primary,
                Some(lease),
            ));
        }
        let lineage = if let Some(context) = basis.context() {
            let mechanism = match lineage {
                CasLineageProof::Native { mechanism, .. } => mechanism,
                _ => unreachable!("fresh native publication starts from a native mechanism"),
            };
            match context.complete_native(mechanism, lease.generation(), None) {
                Ok(lineage) => lineage,
                Err(error) => {
                    return Err(self.abandon_projection_target(
                        home,
                        storage,
                        session,
                        request,
                        basis,
                        cas_thread_id,
                        StaleObservation::unknown(Some(lease.generation())),
                        "native target context proof did not match its prepared source",
                        ProjectionExecutionError::from(error),
                        Some(lease),
                    ));
                }
            }
        } else {
            lineage
        };
        let current_authority = self
            .ensure_home(home)
            .map_err(ProjectionExecutionError::from)
            .and_then(|()| {
                if cancellation.is_cancelled() {
                    Err(ProjectionExecutionError::Cancelled)
                } else {
                    Ok(())
                }
            });
        if let Err(primary) = current_authority {
            return Err(self.abandon_projection_target(
                home,
                storage,
                session,
                request,
                basis,
                cas_thread_id,
                StaleObservation::exact(
                    request.execution_binding().clone(),
                    basis.represented_prefix(),
                    basis.tool_profile(),
                    lineage,
                    native_turn_count,
                    Some(lease.generation()),
                ),
                "native target Home changed before binding publication",
                primary,
                Some(lease),
            ));
        }
        let publication = PublishValidBinding::from_native(
            basis,
            request.execution_binding().clone(),
            cas_thread_id.clone(),
            native_turn_count,
            lineage,
        );
        match publication::publish_valid(home, storage, &publication, point_limit()) {
            Ok(revision) => Ok(self.capability(request, revision, cas_thread_id, lease, lineage)),
            Err(error) => {
                let primary = ProjectionExecutionError::from(error);
                Err(self.abandon_projection_target(
                    home,
                    storage,
                    session,
                    request,
                    basis,
                    cas_thread_id,
                    StaleObservation::exact(
                        request.execution_binding().clone(),
                        basis.represented_prefix(),
                        basis.tool_profile(),
                        lineage,
                        native_turn_count,
                        Some(lease.generation()),
                    ),
                    stale_reason,
                    primary,
                    Some(lease),
                ))
            }
        }
    }
}
