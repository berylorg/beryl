mod outcome;
pub(crate) mod recovery;
mod transaction;
pub use outcome::*;
pub mod validation;

use crate::window_acquisition::{RuntimeBackedWindowProcessRegistry, WindowSelectionLease};
use beryl_home_store::{
    CommandCancellation, CommandError, CommandOutcome, CommitReceipt, CommittedLocalFinalization,
    CursorReadLimits, HomeCommand, HomeGenerationIdentity, HomeServiceReference, HomeStore,
    ReconciliationFailure, ReconciliationHandle, ReconciliationResolution,
};
use beryl_model::{
    BerylHomeId, RootId, RuntimeId, RuntimeLaunchForm, SyndicDraftId, SyndicThreadId, WindowId,
};
use beryl_state::{BerylState, RememberedTarget, SessionExitIntent, SessionWindowRecord};
use std::{
    path::Path,
    sync::{Arc, Condvar, Mutex},
};
use syndic_storage::{DraftEditHistoryPolicyV1, SyndicStorage};
pub use transaction::OnboardingFacts;

#[derive(Debug, thiserror::Error)]
pub enum AdmissionError {
    #[error("runtime/root admission is busy or retired")]
    Busy,
    #[error("runtime/root admission source was rejected: {0}")]
    Source(String),
    #[error("runtime/root validation failed: {0}")]
    Validation(#[from] validation::ValidationError),
    #[error("runtime/root validation retains unsettled cleanup: {0}")]
    Cleanup(ValidationAdmissionCleanup),
    #[error("runtime/root command did not commit: {0}")]
    Command(CommandError),
}

#[derive(Clone, Debug)]
pub struct RuntimeAdmissionSource {
    home_id: BerylHomeId,
    generation: HomeGenerationIdentity,
    window: SessionWindowRecord,
}

#[derive(Clone)]
pub struct RuntimeAdmissionService {
    store: Arc<HomeServiceReference>,
    state: BerylState,
    syndic: SyndicStorage,
    process: RuntimeBackedWindowProcessRegistry,
    validator: validation::RuntimePathValidator,
    history_policy: DraftEditHistoryPolicyV1,
    lifetime: Arc<AdmissionLifetime>,
}

#[derive(Default)]
struct AdmissionLifetime {
    state: Mutex<AdmissionLifetimeState>,
    settled: Condvar,
}

#[derive(Default)]
struct AdmissionLifetimeState {
    retired: bool,
    active: Option<CommandCancellation>,
    cleanup: Option<Arc<Mutex<ValidationCleanupState>>>,
}

struct ValidationCleanupState {
    error: validation::ValidationError,
    lease: Option<WindowSelectionLease>,
}

pub struct ValidationAdmissionCleanup {
    owner: Arc<Mutex<ValidationCleanupState>>,
}

impl ValidationAdmissionCleanup {
    pub fn issue(&self) -> validation::ValidationIssue {
        self.owner
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .error
            .issue()
    }

    pub fn dispose_cleanup(&mut self) -> Result<(), validation::ValidationIssue> {
        let mut owner = self.owner.lock().unwrap_or_else(|error| error.into_inner());
        owner.error.dispose_cleanup()?;
        owner.lease = None;
        Ok(())
    }
}

impl std::fmt::Display for ValidationAdmissionCleanup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let owner = self.owner.lock().unwrap_or_else(|error| error.into_inner());
        std::fmt::Display::fmt(&owner.error, f)
    }
}

impl std::fmt::Debug for ValidationAdmissionCleanup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}

struct AdmissionWorker(Arc<AdmissionLifetime>);

impl Drop for AdmissionWorker {
    fn drop(&mut self) {
        let mut state = self
            .0
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        state.active = None;
        self.0.settled.notify_all();
    }
}

impl RuntimeAdmissionService {
    #[cfg(test)]
    pub(crate) fn test_execute_first(
        &self,
        command: HomeCommand,
        runtime_id: RuntimeId,
        root_id: RootId,
        onboarding: OnboardingFacts,
    ) -> RuntimeAdmissionOutcome {
        let window_id = onboarding.window().window_id();
        let lease = self
            .process
            .admit_selection(&[window_id], window_id)
            .unwrap();
        self.execute(
            transaction::PreparedAdmission {
                command,
                admission: AdmissionFacts {
                    window_id,
                    runtime_id,
                    root_id,
                    onboarding: Some(onboarding),
                },
            },
            lease,
        )
        .unwrap()
    }
    pub fn new(
        store: Arc<HomeServiceReference>,
        state: BerylState,
        syndic: SyndicStorage,
        process: RuntimeBackedWindowProcessRegistry,
        validator: validation::RuntimePathValidator,
        history_policy: DraftEditHistoryPolicyV1,
    ) -> Self {
        Self {
            store,
            state,
            syndic,
            process,
            validator,
            history_policy,
            lifetime: Arc::new(AdmissionLifetime::default()),
        }
    }

    pub fn retire(&self) -> Result<(), validation::ValidationIssue> {
        let mut state = self
            .lifetime
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        state.retired = true;
        if let Some(cancellation) = &state.active {
            cancellation.cancel();
        }
        while state.active.is_some() {
            state = self
                .lifetime
                .settled
                .wait(state)
                .unwrap_or_else(|error| error.into_inner());
        }
        let cleanup = state.cleanup.clone();
        drop(state);
        if let Some(owner) = cleanup {
            ValidationAdmissionCleanup { owner }.dispose_cleanup()?;
        }
        Ok(())
    }

    pub fn capture_source(
        &self,
        window_id: WindowId,
    ) -> Result<RuntimeAdmissionSource, AdmissionError> {
        let before = self.store.home_revision().map_err(preparation)?;
        let generation = self.store.generation_identity().map_err(preparation)?;
        let session = self
            .state
            .session()
            .minimal_bootstrap(&self.store)
            .map_err(preparation)?
            .ok_or_else(|| invalid("admission has no session"))?;
        if session.header().exit_intent() != SessionExitIntent::Running {
            return Err(invalid("admission requires a running session"));
        }
        let window = session
            .windows()
            .iter()
            .find(|window| window.window_id() == window_id)
            .ok_or_else(|| invalid("admission window is not a session member"))?
            .clone();
        if self.store.home_revision().map_err(preparation)? != before {
            return Err(invalid("admission source changed during capture"));
        }
        Ok(RuntimeAdmissionSource {
            home_id: self.store.home_id(),
            generation,
            window,
        })
    }

    pub fn add_runtime(
        &self,
        source: RuntimeAdmissionSource,
        resident_windows: &[WindowId],
        selected: &Path,
        launch_form: RuntimeLaunchForm,
        cancellation: CommandCancellation,
    ) -> RuntimeAdmissionOutcome {
        let operation = || -> Result<RuntimeAdmissionOutcome, AdmissionError> {
            let _worker = self.enter(&cancellation)?;
            let mut lease = Some(
                self.process
                    .admit_selection(resident_windows, source.window.window_id())
                    .map_err(preparation)?,
            );
            self.validate_source(&source, &cancellation)?;
            let resolved = self.finish_validation(
                self.validator.resolve_executable(selected, &cancellation),
                &mut lease,
            )?;
            self.validate_source(&source, &cancellation)?;
            if let Some(existing) = self
                .state
                .runtime_roots()
                .runtime_by_executable(&self.store, resolved.canonical_executable())
                .map_err(preparation)?
            {
                return Ok(RuntimeAdmissionOutcome::Existing {
                    runtime_id: existing.runtime_id(),
                    root_id: None,
                });
            }
            let runtime_id = RuntimeId::from_bytes(transaction::identity()?);
            let admitted = self.finish_validation(
                self.validator
                    .qualify_runtime(runtime_id, resolved, launch_form, &cancellation),
                &mut lease,
            )?;
            self.validate_source(&source, &cancellation)?;
            let root_id = RootId::from_bytes(transaction::identity()?);
            let prepared = transaction::prepare_runtime(
                self,
                &source.window,
                admitted,
                runtime_id,
                root_id,
                &cancellation,
            )?;
            self.execute(
                prepared,
                lease.expect("validated admission retains its selection lease"),
            )
        };
        operation().unwrap_or_else(|error| RuntimeAdmissionOutcome::NotCommitted { error })
    }

    pub fn add_root(
        &self,
        source: RuntimeAdmissionSource,
        resident_windows: &[WindowId],
        runtime_id: RuntimeId,
        selected: &Path,
        cancellation: CommandCancellation,
    ) -> RuntimeAdmissionOutcome {
        let operation = || -> Result<RuntimeAdmissionOutcome, AdmissionError> {
            let _worker = self.enter(&cancellation)?;
            let mut lease = Some(
                self.process
                    .admit_selection(resident_windows, source.window.window_id())
                    .map_err(preparation)?,
            );
            self.validate_source(&source, &cancellation)?;
            let runtime = self
                .state
                .runtime_roots()
                .runtime(&self.store, runtime_id)
                .map_err(preparation)?
                .ok_or_else(|| invalid("root runtime is not registered"))?;
            let root = self.finish_validation(
                self.validator
                    .resolve_root(selected, runtime.mode(), &cancellation),
                &mut lease,
            )?;
            self.validate_source(&source, &cancellation)?;
            if let Some(existing) = self
                .state
                .runtime_roots()
                .root_by_path(&self.store, runtime_id, root.runtime_native_path())
                .map_err(preparation)?
            {
                return Ok(RuntimeAdmissionOutcome::Existing {
                    runtime_id,
                    root_id: Some(existing.root_id()),
                });
            }
            let prepared = transaction::prepare_root(
                self,
                &source.window,
                &runtime,
                root,
                RootId::from_bytes(transaction::identity()?),
                &cancellation,
            )?;
            self.execute(
                prepared,
                lease.expect("validated admission retains its selection lease"),
            )
        };
        operation().unwrap_or_else(|error| RuntimeAdmissionOutcome::NotCommitted { error })
    }

    fn enter(&self, cancellation: &CommandCancellation) -> Result<AdmissionWorker, AdmissionError> {
        let mut state = self
            .lifetime
            .state
            .try_lock()
            .map_err(|_| AdmissionError::Busy)?;
        if let Some(cleanup) = &state.cleanup {
            let settled = cleanup
                .try_lock()
                .map_err(|_| AdmissionError::Busy)?
                .lease
                .is_none();
            if !settled {
                return Err(AdmissionError::Busy);
            }
            state.cleanup = None;
        }
        if state.retired || state.active.is_some() || cancellation.is_cancelled() {
            return Err(AdmissionError::Busy);
        }
        state.active = Some(cancellation.clone());
        Ok(AdmissionWorker(self.lifetime.clone()))
    }

    fn finish_validation<T>(
        &self,
        result: Result<T, validation::ValidationError>,
        lease: &mut Option<WindowSelectionLease>,
    ) -> Result<T, AdmissionError> {
        match result {
            Ok(value) => Ok(value),
            Err(error) if error.has_cleanup_custody() => {
                let owner = Arc::new(Mutex::new(ValidationCleanupState {
                    error,
                    lease: lease.take(),
                }));
                self.lifetime
                    .state
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .cleanup = Some(owner.clone());
                Err(AdmissionError::Cleanup(ValidationAdmissionCleanup {
                    owner,
                }))
            }
            Err(error) => Err(AdmissionError::Validation(error)),
        }
    }

    fn validate_source(
        &self,
        source: &RuntimeAdmissionSource,
        cancellation: &CommandCancellation,
    ) -> Result<(), AdmissionError> {
        if cancellation.is_cancelled() {
            return Err(invalid("admission was cancelled"));
        }
        if source.home_id != self.store.home_id()
            || source.generation != self.store.generation_identity().map_err(preparation)?
        {
            return Err(invalid(
                "admission source belongs to another home generation",
            ));
        }
        let session = self
            .state
            .session()
            .minimal_bootstrap(&self.store)
            .map_err(preparation)?
            .ok_or_else(|| invalid("admission session is unavailable"))?;
        if session.header().exit_intent() != SessionExitIntent::Running {
            return Err(invalid("admission session is no longer running"));
        }
        if session
            .windows()
            .iter()
            .find(|window| window.window_id() == source.window.window_id())
            != Some(&source.window)
        {
            return Err(invalid("admission window source changed"));
        }
        Ok(())
    }

    fn execute(
        &self,
        prepared: transaction::PreparedAdmission,
        lease: WindowSelectionLease,
    ) -> Result<RuntimeAdmissionOutcome, AdmissionError> {
        let generation = self.store.generation_identity().map_err(preparation)?;
        let outcome = lease
            .admit_commit(|| self.store.execute(prepared.command))
            .map_err(preparation)?;
        Ok(match outcome {
            CommandOutcome::NotCommitted { evidence } => RuntimeAdmissionOutcome::NotCommitted {
                error: AdmissionError::Command(evidence),
            },
            CommandOutcome::Committed {
                receipt,
                later_failure,
                local_finalization,
            } => RuntimeAdmissionOutcome::Committed {
                admission: CommittedAdmission {
                    facts: prepared.admission,
                    lease,
                    store: self.store.clone(),
                    generation,
                    lifetime: self.lifetime.clone(),
                },
                receipt,
                later_failure,
                local_finalization,
            },
            CommandOutcome::Indeterminate {
                failure,
                reconciliation,
            } => RuntimeAdmissionOutcome::Indeterminate {
                failure,
                reconciliation: AdmissionReconciliation {
                    admission: CommittedAdmission {
                        facts: prepared.admission,
                        lease,
                        store: self.store.clone(),
                        generation,
                        lifetime: self.lifetime.clone(),
                    },
                    handle: reconciliation.install_and_handle(),
                },
            },
        })
    }
}

fn preparation(error: impl std::fmt::Display) -> AdmissionError {
    AdmissionError::Source(error.to_string())
}
fn invalid(message: &str) -> AdmissionError {
    AdmissionError::Source(message.into())
}
