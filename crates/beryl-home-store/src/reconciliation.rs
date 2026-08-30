use std::{
    error::Error,
    fmt,
    sync::{Arc, Condvar, Mutex, MutexGuard, Weak},
};

use beryl_model::DomainRevision;
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::{
    CommitReceipt, DomainCallbackSource, ReadError,
    command::RetainedReconciliationDescriptor,
    domain::callback::ErasedCallbackError,
    health::{ClassifiedFjallError, FailureSeverity},
    store::HomeStore,
    successor::{DerivedReadFact, SuccessorExecution, SuccessorRoleKind, SuccessorRoleResult},
};

pub(crate) mod reader;
mod registry;
pub use reader::{DomainReconciliation, ReconciliationReader, ReconciliationRecord};
pub(crate) use registry::{ReconciliationRegistry, ReconciliationSlot};
use registry::{
    RegistryInner, RegistryState, RetryHandle, ScopeState, release_retained_core_if_idle,
};

pub(crate) const RECONCILIATION_SCOPE_CAPACITY: usize = 1_024;
const RECONCILIATION_WORKER_CAPACITY: usize = 4;
const COLLISION_DOMAIN_BYTES: usize = 32;
const COLLISION_RECORD_BYTES: usize = 160;

#[derive(Debug)]
pub(crate) enum ReconciliationReservationError {
    DescriptorTooLarge { requested: usize, limit: usize },
    Capacity,
}

/// Terminal targeted classification shared by every trigger for one exact scope.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReconciliationResolution {
    ExactOld,
    ExactNew { receipt: CommitReceipt },
    ExactSuccessor { receipt: CommitReceipt },
    Collision,
}

/// Cloneable retained failure from one targeted reconciliation worker.
#[derive(Clone)]
pub struct ReconciliationFailure(Arc<ReconciliationFailureInner>);

#[derive(Debug, Error)]
enum ReconciliationFailureInner {
    #[error("reconciliation handle does not belong to this home registry")]
    ForeignScope,
    #[error("the reconciliation scope is no longer retained")]
    StaleScope,
    #[error("the Beryl-home generation lock is poisoned")]
    GenerationPoisoned,
    #[error("reconciliation snapshot failed: {0}")]
    Snapshot(#[source] Box<dyn Error + Send + Sync>),
    #[error("reconciliation hook for `{domain}` could not read exact natural records: {source}")]
    HookAccess {
        domain: &'static str,
        #[source]
        source: DomainCallbackSource,
    },
    #[error("reconciliation hook for `{domain}` rejected exact natural records: {source}")]
    HookRejected {
        domain: &'static str,
        #[source]
        source: Box<dyn Error + Send + Sync>,
    },
}

impl fmt::Debug for ReconciliationFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}
impl fmt::Display for ReconciliationFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}
impl Error for ReconciliationFailure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.0.source()
    }
}

impl From<crate::HealthGateError> for ReconciliationFailure {
    fn from(error: crate::HealthGateError) -> Self {
        failure(ReconciliationFailureInner::HookAccess {
            domain: "home",
            source: DomainCallbackSource::Read(ReadError::HealthGate(error)),
        })
    }
}

type SharedResult = Result<ReconciliationResolution, ReconciliationFailure>;

enum FlightState {
    Idle,
    Running,
    Complete(SharedResult),
}
struct ReconciliationFlight {
    state: Mutex<FlightState>,
    completed: Condvar,
}

impl ReconciliationFlight {
    fn new() -> Self {
        Self {
            state: Mutex::new(FlightState::Idle),
            completed: Condvar::new(),
        }
    }
}

/// Opaque cloneable capability naming one exact retained reconciliation scope.
#[derive(Clone)]
pub struct ReconciliationHandle {
    registry: Weak<RegistryInner>,
    index: usize,
    token: u64,
    flight: Arc<ReconciliationFlight>,
}

impl fmt::Debug for ReconciliationHandle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ReconciliationHandle")
            .field("scope", &self.token)
            .finish_non_exhaustive()
    }
}

struct SealedCollision {
    _domains: Vec<SealedDomain>,
    _receipt_revision: u64,
    charged_bytes: usize,
    _successor: Option<SealedSuccessor>,
}
struct SealedDomain {
    _domain_slot: usize,
    _intended_revision: DomainRevision,
    _side: DomainReconciliation,
    _records: Vec<SealedRecord>,
}
struct SealedRecord {
    _family_slot: usize,
    _key: Box<[u8]>,
    _old_digest: Option<[u8; 32]>,
    _new_digest: Option<[u8; 32]>,
}
struct SealedSuccessor {
    _protocol: &'static str,
    _correlation_digest: Option<[u8; 32]>,
    _roles: Vec<SealedSuccessorRole>,
}
struct SealedSuccessorRole {
    _domain_slot: usize,
    _kind: SuccessorRoleKind,
    _result: SuccessorRoleResult,
    _correlation_digest: Option<[u8; 32]>,
    _derived: Vec<DerivedReadFact>,
}

struct ReconciliationExecution {
    sides: Vec<DomainReconciliation>,
    successor: Option<SuccessorExecution>,
    receipt: CommitReceipt,
}

impl HomeStore {
    /// Returns a bounded snapshot of installed or collision-closed operation handles.
    #[must_use]
    pub fn pending_reconciliations(&self) -> Vec<ReconciliationHandle> {
        self.reconciliation.handles()
    }

    /// Triggers or joins targeted reconciliation for one exact installed operation scope.
    pub fn reconcile(
        &self,
        handle: &ReconciliationHandle,
    ) -> Result<ReconciliationResolution, ReconciliationFailure> {
        let Some(inner) = handle.registry.upgrade() else {
            return Err(failure(ReconciliationFailureInner::StaleScope));
        };
        if !Arc::ptr_eq(&inner, &self.reconciliation.inner) {
            return Err(failure(ReconciliationFailureInner::ForeignScope));
        }
        let mut flight = handle
            .flight
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        loop {
            match &*flight {
                FlightState::Complete(result) => return result.clone(),
                FlightState::Running => {
                    flight = handle
                        .flight
                        .completed
                        .wait(flight)
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                }
                FlightState::Idle => {
                    *flight = FlightState::Running;
                    break;
                }
            }
        }
        drop(flight);

        let result = self.run_reconciliation(handle, &inner);
        let mut flight = handle
            .flight
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *flight = FlightState::Complete(result.clone());
        handle.flight.completed.notify_all();
        result
    }

    pub fn retry_reconciliation(
        &self,
        handle: &ReconciliationHandle,
    ) -> Result<ReconciliationResolution, ReconciliationFailure> {
        let Some(inner) = handle.registry.upgrade() else {
            return Err(failure(ReconciliationFailureInner::StaleScope));
        };
        if !Arc::ptr_eq(&inner, &self.reconciliation.inner) {
            return Err(failure(ReconciliationFailureInner::ForeignScope));
        }
        match self.reconciliation.retry_handle(handle) {
            Some(RetryHandle::Current(retry)) => self.reconcile(&retry),
            Some(RetryHandle::Terminal) => self.reconcile(handle),
            None => Err(failure(ReconciliationFailureInner::StaleScope)),
        }
    }

    fn run_reconciliation(
        &self,
        handle: &ReconciliationHandle,
        inner: &Arc<RegistryInner>,
    ) -> SharedResult {
        acquire_worker(inner, handle)?;
        let execution = self.execute_reconciliation_hook(handle, inner);
        finish_worker(inner, handle, execution)
    }

    fn execute_reconciliation_hook(
        &self,
        handle: &ReconciliationHandle,
        inner: &Arc<RegistryInner>,
    ) -> Result<ReconciliationExecution, ReconciliationFailure> {
        let admission = self.health.admit().map_err(|error| {
            failure(ReconciliationFailureInner::HookAccess {
                domain: "home",
                source: DomainCallbackSource::Read(ReadError::HealthGate(error)),
            })
        })?;
        let generation = self.generation.read().map_err(|_| {
            admission.fail(FailureSeverity::Structural);
            failure(ReconciliationFailureInner::GenerationPoisoned)
        })?;
        let generation = generation.as_ref().ok_or_else(|| {
            admission.fail(FailureSeverity::Structural);
            failure(ReconciliationFailureInner::GenerationPoisoned)
        })?;
        let snapshot = generation.database.snapshot().map_err(|source| {
            let source = ClassifiedFjallError::direct(source);
            signal_structural(&admission, source.severity());
            failure(ReconciliationFailureInner::Snapshot(Box::new(source)))
        })?;
        let descriptor = {
            let state = inner.lock_state();
            match &state.scopes[handle.index] {
                ScopeState::Verifying {
                    token, descriptor, ..
                } if *token == handle.token => Arc::clone(descriptor),
                _ => return Err(failure(ReconciliationFailureInner::StaleScope)),
            }
        };
        let mut sides = Vec::with_capacity(descriptor.domains.len());
        for domain_descriptor in &descriptor.domains {
            let domain = generation
                .registry
                .get(domain_descriptor.domain_slot)
                .ok_or_else(|| failure(ReconciliationFailureInner::StaleScope))?;
            let side = (domain.reconciler)(&snapshot, domain, domain_descriptor)
                .map_err(|error| map_hook_failure(domain.name, error, &admission))?;
            sides.push(side);
        }
        let unanimous_exact_side = sides
            .iter()
            .all(|side| *side == DomainReconciliation::ExactOld)
            || sides
                .iter()
                .all(|side| *side == DomainReconciliation::ExactNew);
        let successor = match descriptor.successor.as_ref() {
            None => None,
            Some(_) if unanimous_exact_side => None,
            Some(successor) if !descriptor_sides_admit_successor(&descriptor, &sides) => {
                Some(successor.unrun_collision())
            }
            Some(successor) => Some(
                successor
                    .execute(&snapshot, &generation.registry, &descriptor.domains)
                    .map_err(|(domain, error)| map_hook_failure(domain, error, &admission))?,
            ),
        };
        admission.confirm_database(&generation.database, |source| {
            failure(ReconciliationFailureInner::Snapshot(Box::new(source)))
        })?;
        Ok(ReconciliationExecution {
            sides,
            successor,
            receipt: descriptor.receipt.clone(),
        })
    }
}

fn failure(inner: ReconciliationFailureInner) -> ReconciliationFailure {
    ReconciliationFailure(Arc::new(inner))
}

fn acquire_worker(
    inner: &Arc<RegistryInner>,
    handle: &ReconciliationHandle,
) -> Result<(), ReconciliationFailure> {
    let mut state = inner.lock_state();
    loop {
        if !matches!(
            state.scopes.get(handle.index),
            Some(ScopeState::Verifying { token, .. }) if *token == handle.token
        ) {
            return Err(failure(ReconciliationFailureInner::StaleScope));
        }
        if state.active_workers < RECONCILIATION_WORKER_CAPACITY {
            state.active_workers += 1;
            return Ok(());
        }
        state = inner
            .worker_released
            .wait(state)
            .unwrap_or_else(|poisoned| poisoned.into_inner());
    }
}

fn finish_worker(
    inner: &Arc<RegistryInner>,
    handle: &ReconciliationHandle,
    execution: Result<ReconciliationExecution, ReconciliationFailure>,
) -> SharedResult {
    let mut state = inner.lock_state();
    let outcome = match execution {
        Err(error) => Err(error),
        Ok(execution) => {
            let ReconciliationExecution {
                sides,
                successor,
                receipt,
            } = execution;
            let all_old = sides
                .iter()
                .all(|side| *side == DomainReconciliation::ExactOld);
            let all_new = sides
                .iter()
                .all(|side| *side == DomainReconciliation::ExactNew);
            let exact_successor = successor.as_ref().is_some_and(|successor| {
                successor.resolved
                    && match &state.scopes[handle.index] {
                        ScopeState::Verifying { descriptor, .. } => {
                            descriptor_sides_admit_successor(descriptor, &sides)
                        }
                        _ => false,
                    }
            });
            if all_old || all_new || exact_successor {
                let charged_bytes = match &state.scopes[handle.index] {
                    ScopeState::Verifying {
                        token,
                        charged_bytes,
                        ..
                    } if *token == handle.token => *charged_bytes,
                    _ => return release_worker_with_stale(inner, state),
                };
                state.scopes[handle.index] = ScopeState::Vacant;
                release_charge(inner, &mut state, charged_bytes);
                release_retained_core_if_idle(&mut state);
                if all_old {
                    Ok(ReconciliationResolution::ExactOld)
                } else if all_new {
                    Ok(ReconciliationResolution::ExactNew { receipt })
                } else {
                    Ok(ReconciliationResolution::ExactSuccessor { receipt })
                }
            } else {
                let (descriptor, charged_bytes, flight) = match &state.scopes[handle.index] {
                    ScopeState::Verifying {
                        token,
                        descriptor,
                        charged_bytes,
                        flight,
                    } if *token == handle.token => {
                        (Arc::clone(descriptor), *charged_bytes, Arc::clone(flight))
                    }
                    _ => return release_worker_with_stale(inner, state),
                };
                let facts = seal_collision(&descriptor, &sides, successor.as_ref());
                debug_assert!(facts.charged_bytes <= charged_bytes);
                state.reserved_bytes = state
                    .reserved_bytes
                    .checked_sub(charged_bytes)
                    .and_then(|bytes| bytes.checked_add(facts.charged_bytes))
                    .expect("collision charge replacement remains within its reservation");
                state.scopes[handle.index] = ScopeState::Closed {
                    token: handle.token,
                    _facts: facts,
                    flight,
                };
                release_retained_core_if_idle(&mut state);
                Ok(ReconciliationResolution::Collision)
            }
        }
    };
    state.active_workers = state
        .active_workers
        .checked_sub(1)
        .expect("one active reconciliation worker owns this completion");
    inner.worker_released.notify_one();
    outcome
}

fn release_worker_with_stale(
    inner: &Arc<RegistryInner>,
    mut state: MutexGuard<'_, RegistryState>,
) -> SharedResult {
    state.active_workers = state.active_workers.saturating_sub(1);
    inner.worker_released.notify_one();
    Err(failure(ReconciliationFailureInner::StaleScope))
}

fn release_charge(inner: &RegistryInner, state: &mut RegistryState, charged_bytes: usize) {
    if let Some(remaining) = state.reserved_bytes.checked_sub(charged_bytes) {
        state.reserved_bytes = remaining;
    } else {
        inner.health.signal_failure(FailureSeverity::Structural);
    }
}

fn map_hook_failure(
    domain: &'static str,
    error: ErasedCallbackError,
    admission: &crate::health::HealthAdmission<'_>,
) -> ReconciliationFailure {
    match error {
        ErasedCallbackError::Access(source) => {
            signal_structural(
                admission,
                crate::domain::callback::reconciliation_callback_failure_severity(&source),
            );
            failure(ReconciliationFailureInner::HookAccess { domain, source })
        }
        ErasedCallbackError::Rejected(source) => {
            failure(ReconciliationFailureInner::HookRejected { domain, source })
        }
    }
}

fn signal_structural(
    admission: &crate::health::HealthAdmission<'_>,
    severity: Option<FailureSeverity>,
) {
    if severity == Some(FailureSeverity::Structural) {
        admission.fail(FailureSeverity::Structural);
    }
}

fn seal_collision(
    descriptor: &RetainedReconciliationDescriptor,
    sides: &[DomainReconciliation],
    successor: Option<&SuccessorExecution>,
) -> SealedCollision {
    let mut charged_bytes = 32usize;
    let domains = descriptor
        .domains
        .iter()
        .zip(sides.iter().copied())
        .map(|(domain, side)| {
            charged_bytes = charged_bytes
                .checked_add(COLLISION_DOMAIN_BYTES)
                .expect("reserved collision charge arithmetic");
            let records = domain
                .records
                .iter()
                .map(|record| {
                    charged_bytes = charged_bytes
                        .checked_add(COLLISION_RECORD_BYTES)
                        .and_then(|bytes| bytes.checked_add(record.key.len()))
                        .expect("reserved collision charge arithmetic");
                    SealedRecord {
                        _family_slot: record.family_slot,
                        _key: record.key.clone(),
                        _old_digest: record.old.as_deref().map(digest),
                        _new_digest: record.new.as_deref().map(digest),
                    }
                })
                .collect();
            SealedDomain {
                _domain_slot: domain.domain_slot,
                _intended_revision: domain.intended_revision,
                _side: side,
                _records: records,
            }
        })
        .collect();
    let successor = successor.map(|successor| {
        charged_bytes = charged_bytes
            .checked_add(128)
            .and_then(|bytes| bytes.checked_add(successor.identity.name.len()))
            .expect("reserved successor collision charge arithmetic");
        let roles = successor
            .roles
            .iter()
            .map(|role| {
                charged_bytes = charged_bytes
                    .checked_add(160)
                    .and_then(|bytes| bytes.checked_add(role.derived.len().saturating_mul(96)))
                    .expect("reserved successor role charge arithmetic");
                SealedSuccessorRole {
                    _domain_slot: role.domain_slot,
                    _kind: role.kind,
                    _result: role.result,
                    _correlation_digest: role.correlation_digest,
                    _derived: role.derived.clone(),
                }
            })
            .collect();
        SealedSuccessor {
            _protocol: successor.identity.name,
            _correlation_digest: successor.correlation_digest,
            _roles: roles,
        }
    });
    SealedCollision {
        _domains: domains,
        _receipt_revision: descriptor.receipt.home_revision().get(),
        charged_bytes,
        _successor: successor,
    }
}

fn descriptor_sides_admit_successor(
    descriptor: &RetainedReconciliationDescriptor,
    sides: &[DomainReconciliation],
) -> bool {
    let Some(successor) = &descriptor.successor else {
        return false;
    };
    descriptor.domains.iter().zip(sides).all(|(domain, side)| {
        if successor
            .roles
            .iter()
            .any(|role| role.domain_slot == domain.domain_slot)
        {
            *side != DomainReconciliation::ExactOld
        } else {
            *side == DomainReconciliation::ExactNew
        }
    })
}

fn digest(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}
