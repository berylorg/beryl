use super::*;

#[cfg(all(test, feature = "test-faults"))]
mod seed_advance_tests {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/native_context_seed_advance.rs"
    ));
}

impl CasProjectionCoordinator {
    pub(super) fn prepare_native_context_seed_after_basis(
        &self,
        home: &HomeStore,
        storage: &SyndicStorage,
        basis: NativeProjectionBasis,
        model_window: Option<u64>,
        cancellation: &ProjectionCancellationToken,
        before_seed: impl FnOnce(),
    ) -> Result<syndic_storage::RecoveryProjection, ProjectionExecutionError> {
        self.ensure_home(home)?;
        if cancellation.is_cancelled() {
            return Err(ProjectionExecutionError::Cancelled);
        }
        let expected = basis
            .context()
            .filter(|context| context.needs_seed())
            .map(|context| context.source_revision());
        before_seed();
        let result =
            storage.prepare_native_context_seed_cancellable(home, basis, model_window, &|| {
                cancellation.is_cancelled()
            });
        if matches!(
            result,
            Err(syndic_storage::RecoveryProjectionError::ConcurrentChange)
        ) && let Some(expected) = expected
        {
            self.ensure_home(home)?;
            if cancellation.is_cancelled() {
                return Err(ProjectionExecutionError::Cancelled);
            }
            let actual = storage
                .revision(home)
                .map_err(syndic_storage::SyndicReadError::Read)?;
            if actual != expected {
                return Err(ProjectionExecutionError::NativeContextSeedSourceAdvanced {
                    thread_id: basis.thread_id(),
                    expected,
                    actual,
                });
            }
        }
        result.map_err(ProjectionExecutionError::from)
    }
}
