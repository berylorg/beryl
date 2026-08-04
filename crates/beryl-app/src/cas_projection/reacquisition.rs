use std::fmt;

use beryl_backend::{ThreadLoadOptions, ThreadStatus};
use beryl_home_store::{HomeGeneration, HomeStore};
use beryl_model::{BerylHomeId, BindingRevision, CasThreadId, ExecutionBinding, SyndicThreadId};
use syndic_storage::{BindingState, CasLineageProof, SyndicStorage};

use super::connection::{
    LoadedLeaseRecoveryObservation, LocalLoadedRegistryDispositionOwner,
    QuarantinedProjectionAnchor,
};
use super::execute::native_retry::{NativeCallFailure, call_native_with_retry};
use super::execute::support::point_limit;
use super::{
    AdmittedProjectionSession, CasProjectionCoordinator, LoadedCasProjection,
    LoadedProjectionReleaseError, LoadedProjectionReleaseOutcome, ProjectionCancellationToken,
    ProjectionExecutionError,
};
use crate::conversation_tools::ConversationToolRegistry;

/// Non-cloneable authority to replace one quarantined capture connection without changing CAS lineage.
pub struct SameNativeReacquisitionAnchor {
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    syndic_thread_id: SyndicThreadId,
    binding_revision: BindingRevision,
    execution_binding: ExecutionBinding,
    cas_thread_id: CasThreadId,
    lineage_proof: CasLineageProof,
    lease: Option<QuarantinedProjectionAnchor>,
}

impl SameNativeReacquisitionAnchor {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        home_id: BerylHomeId,
        home_generation: HomeGeneration,
        syndic_thread_id: SyndicThreadId,
        binding_revision: BindingRevision,
        execution_binding: ExecutionBinding,
        cas_thread_id: CasThreadId,
        lineage_proof: CasLineageProof,
        lease: QuarantinedProjectionAnchor,
    ) -> Self {
        Self {
            home_id,
            home_generation,
            syndic_thread_id,
            binding_revision,
            execution_binding,
            cas_thread_id,
            lineage_proof,
            lease: Some(lease),
        }
    }

    #[must_use]
    pub const fn home_id(&self) -> BerylHomeId {
        self.home_id
    }

    #[must_use]
    pub const fn home_generation(&self) -> HomeGeneration {
        self.home_generation
    }

    #[must_use]
    pub const fn syndic_thread_id(&self) -> SyndicThreadId {
        self.syndic_thread_id
    }

    #[must_use]
    pub const fn binding_revision(&self) -> BindingRevision {
        self.binding_revision
    }

    #[must_use]
    pub const fn execution_binding(&self) -> &ExecutionBinding {
        &self.execution_binding
    }

    #[must_use]
    pub const fn cas_thread_id(&self) -> &CasThreadId {
        &self.cas_thread_id
    }

    #[must_use]
    pub const fn lineage_proof(&self) -> CasLineageProof {
        self.lineage_proof
    }

    fn lease(&self) -> &QuarantinedProjectionAnchor {
        self.lease
            .as_ref()
            .expect("reacquisition anchor owns its exact quarantined lease")
    }

    fn lease_mut(&mut self) -> &mut QuarantinedProjectionAnchor {
        self.lease
            .as_mut()
            .expect("reacquisition anchor owns its exact quarantined lease")
    }

    fn take_lease(&mut self) -> QuarantinedProjectionAnchor {
        self.lease
            .take()
            .expect("reacquisition anchor owns its exact quarantined lease")
    }

    pub(in crate::cas_projection) fn recovery_observation(&self) -> LoadedLeaseRecoveryObservation {
        self.lease().recovery_observation()
    }

    pub(in crate::cas_projection) fn into_local_registry_disposition_owner(
        mut self,
    ) -> Result<LocalLoadedRegistryDispositionOwner, Self> {
        match self.take_lease().into_local_registry_disposition_owner() {
            Ok(owner) => Ok(owner),
            Err(lease) => {
                self.lease = Some(lease);
                Err(self)
            }
        }
    }
}

impl fmt::Debug for SameNativeReacquisitionAnchor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SameNativeReacquisitionAnchor")
            .field("home_id", &self.home_id)
            .field("home_generation", &self.home_generation)
            .field("syndic_thread_id", &self.syndic_thread_id)
            .field("binding_revision", &self.binding_revision)
            .field("execution_binding", &self.execution_binding)
            .field("cas_thread_id", &self.cas_thread_id)
            .field("lineage_proof", &self.lineage_proof)
            .field("loaded_generation", &self.lease().generation())
            .finish()
    }
}

impl Drop for SameNativeReacquisitionAnchor {
    fn drop(&mut self) {
        let Some(lease) = self.lease.take() else {
            return;
        };
        if !lease.has_preactivation_surrender() {
            self.lease = Some(lease);
            return;
        }
        let anchor = Self {
            home_id: self.home_id,
            home_generation: self.home_generation,
            syndic_thread_id: self.syndic_thread_id,
            binding_revision: self.binding_revision,
            execution_binding: self.execution_binding.clone(),
            cas_thread_id: self.cas_thread_id.clone(),
            lineage_proof: self.lineage_proof,
            lease: None,
        };
        lease.settle_recovery_owner_drop(move |retainer, lease| {
            let mut anchor = anchor;
            anchor.lease = Some(lease);
            retainer.retain_reacquisition_anchor_from_exact_settlement(anchor);
        });
    }
}

/// Successful fresh-connection handoff plus independent old-subscription cleanup evidence.
#[derive(Debug)]
pub struct SameNativeReacquisitionSuccess {
    projection: LoadedCasProjection,
    old_subscription_cleanup: Result<LoadedProjectionReleaseOutcome, LoadedProjectionReleaseError>,
}

impl SameNativeReacquisitionSuccess {
    #[must_use]
    pub const fn projection(&self) -> &LoadedCasProjection {
        &self.projection
    }

    pub const fn old_subscription_cleanup(
        &self,
    ) -> Result<LoadedProjectionReleaseOutcome, &LoadedProjectionReleaseError> {
        match &self.old_subscription_cleanup {
            Ok(outcome) => Ok(*outcome),
            Err(error) => Err(error),
        }
    }

    #[must_use]
    pub fn into_projection(self) -> LoadedCasProjection {
        self.projection
    }
}

/// Failed same-native handoff. Retryable failures retain the sole exact subscription anchor.
pub enum SameNativeReacquisitionFailure {
    /// The exact old subscription remains anchored and another fresh connection may retry.
    Retryable {
        /// Sole retained authority for the retry.
        anchor: Box<SameNativeReacquisitionAnchor>,
        /// Number of exhausted automatic backend attempts, when dispatch began.
        failed_attempts: Option<u8>,
        /// Exact failure that prevented this attempt from completing.
        source: Box<ProjectionExecutionError>,
    },
    /// The anchor or exact durable basis was lost, so no exact retry remains.
    Unavailable {
        /// Exact terminal reason for losing reacquisition authority.
        source: Box<ProjectionExecutionError>,
    },
}

impl SameNativeReacquisitionFailure {
    #[must_use]
    pub const fn is_retryable(&self) -> bool {
        matches!(self, Self::Retryable { .. })
    }

    #[must_use]
    pub const fn failed_attempts(&self) -> Option<u8> {
        match self {
            Self::Retryable {
                failed_attempts, ..
            } => *failed_attempts,
            Self::Unavailable { .. } => None,
        }
    }

    #[must_use]
    pub const fn source_error(&self) -> &ProjectionExecutionError {
        match self {
            Self::Retryable { source, .. } | Self::Unavailable { source } => source,
        }
    }

    #[must_use]
    pub fn into_retry_anchor(self) -> Option<SameNativeReacquisitionAnchor> {
        match self {
            Self::Retryable { anchor, .. } => Some(*anchor),
            Self::Unavailable { .. } => None,
        }
    }

    fn retryable(
        anchor: SameNativeReacquisitionAnchor,
        failed_attempts: Option<u8>,
        source: ProjectionExecutionError,
    ) -> Self {
        Self::Retryable {
            anchor: Box::new(anchor),
            failed_attempts,
            source: Box::new(source),
        }
    }

    fn unavailable(source: ProjectionExecutionError) -> Self {
        Self::Unavailable {
            source: Box::new(source),
        }
    }
}

impl fmt::Debug for SameNativeReacquisitionFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Retryable {
                anchor,
                failed_attempts,
                source,
            } => formatter
                .debug_struct("SameNativeReacquisitionFailure::Retryable")
                .field("anchor", anchor)
                .field("failed_attempts", failed_attempts)
                .field("source", source)
                .finish(),
            Self::Unavailable { source } => formatter
                .debug_struct("SameNativeReacquisitionFailure::Unavailable")
                .field("source", source)
                .finish(),
        }
    }
}

impl fmt::Display for SameNativeReacquisitionFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_retryable() {
            write!(
                formatter,
                "same-native CAS reacquisition can be retried: {}",
                self.source_error()
            )
        } else {
            write!(
                formatter,
                "same-native CAS reacquisition is unavailable: {}",
                self.source_error()
            )
        }
    }
}

impl std::error::Error for SameNativeReacquisitionFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.source_error())
    }
}

impl CasProjectionCoordinator {
    /// Replaces a quarantined capture connection while preserving the exact live CAS thread.
    pub fn reacquire_same_native(
        &self,
        home: &HomeStore,
        storage: SyndicStorage,
        replacement: &mut AdmittedProjectionSession,
        mut anchor: SameNativeReacquisitionAnchor,
        cancellation: &ProjectionCancellationToken,
    ) -> Result<SameNativeReacquisitionSuccess, SameNativeReacquisitionFailure> {
        if let Err(error) = self.validate_anchor_home(home, &anchor) {
            return Err(retry_or_unavailable(anchor, None, error));
        }
        let _flight = match self.begin_projection(anchor.syndic_thread_id) {
            Ok(flight) => flight,
            Err(error) => {
                return Err(retry_or_unavailable(anchor, None, error.into()));
            }
        };
        if let Err(error) = validate_current_binding(home, storage, &anchor) {
            return Err(SameNativeReacquisitionFailure::unavailable(error));
        }
        if replacement.runtime_id() != anchor.execution_binding.runtime_id() {
            let error = ProjectionExecutionError::RuntimeMismatch {
                requested: anchor.execution_binding.runtime_id(),
                admitted: replacement.runtime_id(),
            };
            return Err(retry_or_unavailable(anchor, None, error));
        }
        if replacement.process_generation() != anchor.lease().generation().process() {
            let error = ProjectionExecutionError::ProcessGenerationMismatch {
                expected: anchor.lease().generation().process(),
                admitted: replacement.process_generation(),
            };
            return Err(retry_or_unavailable(anchor, None, error));
        }
        if anchor.lease().owns_connection(replacement.connection()) {
            let error = ProjectionExecutionError::ReacquisitionConnectionNotFresh {
                thread_id: anchor.cas_thread_id.clone(),
            };
            return Err(retry_or_unavailable(anchor, None, error));
        }
        if cancellation.is_cancelled() {
            return Err(retry_or_unavailable(
                anchor,
                None,
                ProjectionExecutionError::Cancelled,
            ));
        }
        match anchor.lease().is_live() {
            Ok(true) => {}
            Ok(false) => {
                return Err(SameNativeReacquisitionFailure::unavailable(
                    ProjectionExecutionError::ReacquisitionAnchorLost {
                        thread_id: anchor.cas_thread_id.clone(),
                    },
                ));
            }
            Err(error) => {
                return Err(SameNativeReacquisitionFailure::unavailable(error.into()));
            }
        }
        let reservation = match anchor.lease().reserve_replacement(
            replacement.connection(),
            replacement.preactivation_surrender_issuer(),
        ) {
            Ok(reservation) => reservation,
            Err(error) => return Err(retry_or_unavailable(anchor, None, error.into())),
        };

        let thread_id = anchor.cas_thread_id.clone();
        let resume_thread_id = thread_id.clone();
        let options = ThreadLoadOptions::for_root(std::path::PathBuf::from(
            anchor.execution_binding.root_path().as_str(),
        ));
        let timeout = anchor.lease().unsubscribe_timeout();
        let loaded = match call_native_with_retry(replacement, cancellation, move |backend| {
            backend.resume_thread(&resume_thread_id, &options, timeout)
        }) {
            Ok(loaded) => loaded,
            Err(NativeCallFailure::RetryExhausted {
                failed_attempts,
                last_failure,
            }) => {
                let error = ProjectionExecutionError::Backend(last_failure);
                return Err(retry_or_unavailable(anchor, Some(failed_attempts), error));
            }
            Err(NativeCallFailure::Terminal(error)) => {
                return Err(retry_or_unavailable(anchor, None, error));
            }
        };

        if loaded.status() != &ThreadStatus::Idle {
            replacement.invalidate_connection();
            let error = ProjectionExecutionError::ProjectionThreadNotIdle {
                thread_id,
                status: loaded.status().clone(),
            };
            return Err(retry_or_unavailable(anchor, None, error));
        }
        match anchor.lease().is_live() {
            Ok(true) => {}
            Ok(false) => {
                replacement.invalidate_connection();
                return Err(SameNativeReacquisitionFailure::unavailable(
                    ProjectionExecutionError::ReacquisitionAnchorLost {
                        thread_id: anchor.cas_thread_id.clone(),
                    },
                ));
            }
            Err(error) => {
                replacement.invalidate_connection();
                return Err(SameNativeReacquisitionFailure::unavailable(error.into()));
            }
        }
        match reservation.is_live() {
            Ok(true) => {}
            Ok(false) => {
                replacement.invalidate_connection();
                let error = ProjectionExecutionError::ReacquisitionReservationLost {
                    thread_id: anchor.cas_thread_id.clone(),
                };
                return Err(retry_or_unavailable(anchor, None, error));
            }
            Err(error) => {
                replacement.invalidate_connection();
                return Err(retry_or_unavailable(anchor, None, error.into()));
            }
        }

        let lease = match anchor.lease_mut().transfer_to(reservation) {
            Ok(lease) => lease,
            Err(error) => {
                replacement.invalidate_connection();
                return Err(retry_or_unavailable(anchor, None, error.into()));
            }
        };
        if let Err(error) = validate_current_binding(home, storage, &anchor) {
            let _ = lease.release();
            let _ = anchor.take_lease().cleanup_after_transfer();
            return Err(SameNativeReacquisitionFailure::unavailable(error));
        }
        let projection = LoadedCasProjection::new(
            self,
            anchor.syndic_thread_id,
            anchor.binding_revision,
            anchor.execution_binding.clone(),
            anchor.cas_thread_id.clone(),
            lease,
            anchor.lineage_proof,
        );
        let old_subscription_cleanup = anchor.take_lease().cleanup_after_transfer();
        Ok(SameNativeReacquisitionSuccess {
            projection,
            old_subscription_cleanup,
        })
    }

    fn validate_anchor_home(
        &self,
        home: &HomeStore,
        anchor: &SameNativeReacquisitionAnchor,
    ) -> Result<(), ProjectionExecutionError> {
        self.ensure_home(home)?;
        if anchor.home_id != self.home_id() || anchor.home_generation != self.home_generation() {
            return Err(ProjectionExecutionError::ReacquisitionHomeMismatch {
                thread_id: anchor.syndic_thread_id,
            });
        }
        Ok(())
    }
}

fn validate_current_binding(
    home: &HomeStore,
    storage: SyndicStorage,
    anchor: &SameNativeReacquisitionAnchor,
) -> Result<(), ProjectionExecutionError> {
    let current = storage
        .current_binding(home, anchor.syndic_thread_id, point_limit())?
        .ok_or_else(|| ProjectionExecutionError::ReacquisitionBindingChanged {
            thread_id: anchor.syndic_thread_id,
        })?;
    let binding = current.binding();
    let BindingState::Valid(usable) = binding.state() else {
        return Err(ProjectionExecutionError::ReacquisitionBindingChanged {
            thread_id: anchor.syndic_thread_id,
        });
    };
    if binding.thread_id() != anchor.syndic_thread_id
        || binding.revision() != anchor.binding_revision
        || usable.execution() != &anchor.execution_binding
        || usable.cas_thread_id() != &anchor.cas_thread_id
        || usable.tool_profile() != ConversationToolRegistry::canonical().profile()
        || usable.lineage() != anchor.lineage_proof
    {
        return Err(ProjectionExecutionError::ReacquisitionBindingChanged {
            thread_id: anchor.syndic_thread_id,
        });
    }
    if anchor
        .lineage_proof
        .recovered_injection_generation()
        .is_some_and(|injection| injection.process() != anchor.lease().generation().process())
    {
        return Err(ProjectionExecutionError::ReacquisitionBindingChanged {
            thread_id: anchor.syndic_thread_id,
        });
    }
    Ok(())
}

fn retry_or_unavailable(
    anchor: SameNativeReacquisitionAnchor,
    failed_attempts: Option<u8>,
    source: ProjectionExecutionError,
) -> SameNativeReacquisitionFailure {
    match anchor.lease().is_live() {
        Ok(true) => SameNativeReacquisitionFailure::retryable(anchor, failed_attempts, source),
        Ok(false) => SameNativeReacquisitionFailure::unavailable(source),
        Err(error) => SameNativeReacquisitionFailure::unavailable(error.into()),
    }
}
