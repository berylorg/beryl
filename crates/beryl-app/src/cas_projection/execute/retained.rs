use super::*;

impl CasProjectionCoordinator {
    pub(super) fn take_current_projection(
        &self,
        home: &HomeStore,
        session: &AdmittedProjectionSession,
        request: &CasProjectionRequest,
        plan: &NativeProjectionPlan,
        retained: &mut Option<LoadedCasProjection>,
    ) -> Result<Option<LoadedCasProjection>, ProjectionExecutionError> {
        let Some(projection) = retained.as_ref() else {
            return Ok(None);
        };
        let current = match plan {
            NativeProjectionPlan::Current { source, .. } => {
                projection.home_id() == home.home_id()
                    && projection.home_generation() == self.home_generation()
                    && projection.syndic_thread_id() == request.thread_id()
                    && projection.execution_binding() == request.execution_binding()
                    && projection.belongs_to_session(session)
                    && projection.binding_revision() == source.binding_revision()
                    && projection.cas_thread_id() == source.binding().cas_thread_id()
                    && projection.lineage_proof() == source.binding().lineage()
                    && recovered_process_matches(
                        projection.lineage_proof(),
                        projection.loaded_session_generation(),
                    )
                    && projection.is_live()?
            }
            _ => false,
        };
        if current {
            Ok(retained.take())
        } else {
            retained
                .take()
                .expect("validated retained projection remains owned")
                .release()
                .map_err(|error| {
                    ProjectionExecutionError::RetainedProjectionReleaseBeforeAcquisition(Box::new(
                        error,
                    ))
                })?;
            Ok(None)
        }
    }
}
