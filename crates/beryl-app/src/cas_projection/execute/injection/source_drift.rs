use super::*;

#[cfg(all(test, feature = "test-faults"))]
mod tests {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/injection_source_drift.rs"
    ));
}

impl CasProjectionCoordinator {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn source_failure_before_injection(
        &self,
        home: &HomeStore,
        storage: &SyndicStorage,
        session: &AdmittedProjectionSession,
        projection: RecoveryProjection,
        attachment: ConnectionAttachmentIdentity,
        cancellation: &ProjectionCancellationToken,
        error: syndic_storage::RecoveryProjectionError,
    ) -> ProjectionExecutionError {
        if !matches!(
            error,
            syndic_storage::RecoveryProjectionError::ConcurrentChange
        ) {
            return error.into();
        }
        self.witness_injection_source_advance(
            home,
            storage,
            session,
            projection,
            attachment,
            cancellation,
            error.into(),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn source_failure_without_dispatch(
        &self,
        home: &HomeStore,
        storage: &SyndicStorage,
        session: &AdmittedProjectionSession,
        projection: RecoveryProjection,
        attachment: ConnectionAttachmentIdentity,
        cancellation: &ProjectionCancellationToken,
        primary: ProjectionExecutionError,
    ) -> ProjectionExecutionError {
        let source_drift = match &primary {
            ProjectionExecutionError::InjectionNotDispatched { source, .. } => matches!(
                source.as_ref(),
                beryl_backend::ManagedBackendError::ThreadInjectionSource {
                    method,
                    source: ThreadInjectionSourceError::RevisionDrift,
                    transport_bytes_written: false,
                } if method == "thread/inject_items"
            ),
            _ => false,
        };
        if !source_drift {
            return primary;
        }
        self.witness_injection_source_advance(
            home,
            storage,
            session,
            projection,
            attachment,
            cancellation,
            primary,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn witness_injection_source_advance(
        &self,
        home: &HomeStore,
        storage: &SyndicStorage,
        session: &AdmittedProjectionSession,
        projection: RecoveryProjection,
        attachment: ConnectionAttachmentIdentity,
        cancellation: &ProjectionCancellationToken,
        primary: ProjectionExecutionError,
    ) -> ProjectionExecutionError {
        if let Err(failure) =
            self.ensure_injection_source_authority(home, session, attachment, cancellation)
        {
            return failure;
        }
        witness_source_revision(
            home,
            storage,
            projection.thread_id(),
            projection.source_revision(),
            primary,
        )
    }

    pub(super) fn source_failure_after_abandonment(
        &self,
        home: &HomeStore,
        session: &AdmittedProjectionSession,
        attachment: ConnectionAttachmentIdentity,
        cancellation: &ProjectionCancellationToken,
        failure: ProjectionExecutionError,
    ) -> ProjectionExecutionError {
        if matches!(
            failure,
            ProjectionExecutionError::ProjectionSourceAdvancedBeforeInjection { .. }
        ) {
            if let Err(error) =
                self.ensure_injection_source_authority(home, session, attachment, cancellation)
            {
                return error;
            }
        }
        failure
    }

    fn ensure_injection_source_authority(
        &self,
        home: &HomeStore,
        session: &AdmittedProjectionSession,
        attachment: ConnectionAttachmentIdentity,
        cancellation: &ProjectionCancellationToken,
    ) -> Result<(), ProjectionExecutionError> {
        self.ensure_home(home)?;
        if cancellation.is_cancelled() {
            return Err(ProjectionExecutionError::Cancelled);
        }
        if session.connection().projection_source_attachment()? != attachment {
            return Err(
                ProjectionCoordinatorError::ProjectionConnectionUnavailable {
                    runtime_id: session.runtime_id(),
                    process_generation: session.process_generation(),
                }
                .into(),
            );
        }
        Ok(())
    }
}

fn witness_source_revision(
    home: &HomeStore,
    storage: &SyndicStorage,
    thread_id: beryl_model::SyndicThreadId,
    expected: beryl_model::DomainRevision,
    primary: ProjectionExecutionError,
) -> ProjectionExecutionError {
    match storage.revision(home) {
        Ok(actual) if actual != expected => {
            ProjectionExecutionError::ProjectionSourceAdvancedBeforeInjection {
                thread_id,
                expected,
                actual,
            }
        }
        Ok(_) => primary,
        Err(error) => syndic_storage::SyndicReadError::Read(error).into(),
    }
}
