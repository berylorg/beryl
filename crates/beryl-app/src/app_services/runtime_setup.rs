use crate::runtime_admission::{
    AdmissionReconciliationOutcome, RuntimeAdmissionOutcome, RuntimeAdmissionService,
    RuntimeAdmissionSource,
    recovery::FirstConversationAdmissionRecovery,
    validation::{RuntimePathValidator, ValidationLimits},
};
use beryl_home_store::{CommandCancellation, HomeServiceReference};
use beryl_model::{RuntimeLaunchForm, WindowId};
use beryl_state::BerylState;
use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
};
use syndic_storage::{DraftEditHistoryPolicyV1, SyndicStorage};
mod healthy;
#[cfg(test)]
mod test_support;
pub(crate) use healthy::{PublishedRuntimeSetupObservation, PublishedRuntimeSetupServices};

pub(crate) struct RuntimeSetupService {
    home: Mutex<Option<Arc<HomeServiceReference>>>,
    admission: Mutex<Option<Arc<RuntimeAdmissionService>>>,
    retired: Arc<AtomicBool>,
    publication_gate: Arc<Mutex<()>>,
    flights: Mutex<Vec<Arc<RuntimeSetupFlight>>>,
}

pub(crate) struct RuntimeSetupFlight {
    home: Mutex<Option<Arc<HomeServiceReference>>>,
    retired: Arc<AtomicBool>,
    publication_gate: Arc<Mutex<()>>,
    cancellation: CommandCancellation,
    pending: AtomicBool,
    worker: Mutex<Option<JoinHandle<()>>>,
    outcome: Mutex<Option<RuntimeAdmissionOutcome>>,
    first: Mutex<Option<crate::main_window::MainWindowFirstConversationPreparation>>,
    failure: Mutex<Option<String>>,
    preparation_error: Mutex<Option<String>>,
    unavailable: Mutex<Option<AdmissionReconciliationOutcome>>,
    revalidation_ready: AtomicBool,
}

impl RuntimeSetupService {
    pub(crate) fn has_pending_flights(&self) -> bool {
        self.flights
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .any(|flight| flight.is_pending())
    }

    pub(crate) fn cancel_pending_flights(&self) {
        for flight in self
            .flights
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
        {
            if flight.is_pending() {
                flight.cancellation.cancel();
            }
        }
    }
    pub(super) fn prepare(
        home: Arc<HomeServiceReference>,
        state: BerylState,
        storage: SyndicStorage,
        windows: crate::window_acquisition::RuntimeBackedWindowProcessRegistry,
        configuration: &super::AppServiceConfiguration,
    ) -> Result<Arc<Self>, String> {
        let validator = RuntimePathValidator::new(
            configuration.token_directory.clone(),
            ValidationLimits::default(),
            configuration.wsl_supervisor_artifact.clone(),
        )
        .map_err(|e| e.to_string())?;
        let history = DraftEditHistoryPolicyV1::new(8 * 1024 * 1024, 1)
            .ok_or("first conversation history policy is unavailable")?;
        Ok(Arc::new(Self {
            home: Mutex::new(Some(home.clone())),
            admission: Mutex::new(Some(Arc::new(RuntimeAdmissionService::new(
                home, state, storage, windows, validator, history,
            )))),
            retired: Arc::new(AtomicBool::new(false)),
            publication_gate: Arc::new(Mutex::new(())),
            flights: Mutex::new(Vec::new()),
        }))
    }

    pub(crate) fn capture_source(
        &self,
        window: WindowId,
    ) -> Result<RuntimeAdmissionSource, String> {
        if self.retired.load(Ordering::Acquire) {
            return Err("runtime admission graph retired".into());
        }
        self.admission
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .ok_or("runtime admission source retired")?
            .capture_source(window)
            .map_err(|e| e.to_string())
    }

    pub(crate) fn start_add_runtime(
        self: &Arc<Self>,
        source: RuntimeAdmissionSource,
        members: Vec<WindowId>,
        path: PathBuf,
        form: RuntimeLaunchForm,
    ) -> Result<Arc<RuntimeSetupFlight>, String> {
        let mut flights = self.flights.lock().unwrap_or_else(|e| e.into_inner());
        flights.retain(|flight| flight.has_custody());
        if self.retired.load(Ordering::Acquire)
            || flights.len() >= beryl_state::MAX_RESTORABLE_WINDOWS
        {
            return Err("runtime admission graph is retired or full".into());
        }
        let flight = Arc::new(RuntimeSetupFlight {
            home: Mutex::new(self.home.lock().unwrap_or_else(|e| e.into_inner()).clone()),
            retired: self.retired.clone(),
            publication_gate: self.publication_gate.clone(),
            cancellation: CommandCancellation::new(),
            pending: AtomicBool::new(true),
            worker: Mutex::new(None),
            outcome: Mutex::new(None),
            first: Mutex::new(None),
            failure: Mutex::new(None),
            preparation_error: Mutex::new(None),
            unavailable: Mutex::new(None),
            revalidation_ready: AtomicBool::new(false),
        });
        let retained = flight.clone();
        let admission = self
            .admission
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .ok_or("runtime admission source retired")?
            .clone();
        let worker = std::thread::Builder::new()
            .name("runtime-admission".into())
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    admission.add_runtime(
                        source,
                        &members,
                        &path,
                        form,
                        retained.cancellation.clone(),
                    )
                }));
                match result {
                    Ok(outcome) => {
                        *retained.outcome.lock().unwrap_or_else(|e| e.into_inner()) = Some(outcome)
                    }
                    Err(_) => {
                        *retained.failure.lock().unwrap_or_else(|e| e.into_inner()) =
                            Some("runtime admission worker unwound".into())
                    }
                }
                retained.pending.store(false, Ordering::Release);
            })
            .map_err(|e| e.to_string())?;
        *flight.worker.lock().unwrap_or_else(|e| e.into_inner()) = Some(worker);
        flights.push(flight.clone());
        Ok(flight)
    }

    pub(super) fn capture_failed_admission(
        &self,
    ) -> Result<Option<FirstConversationAdmissionRecovery>, String> {
        let flights = self.flights.lock().unwrap_or_else(|e| e.into_inner());
        for flight in flights.iter() {
            flight.cancellation.cancel();
        }
        if flights.iter().any(|f| f.is_pending()) {
            return Err("original admission is still settling".into());
        }
        let count = flights
            .iter()
            .filter(|flight| {
                flight
                    .outcome
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .as_ref()
                    .is_some_and(|outcome| outcome.first_conversation_facts().is_some())
                    || flight
                        .unavailable
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .as_ref()
                        .is_some_and(|outcome| outcome.first_conversation_facts().is_some())
            })
            .count();
        if count > 1 {
            return Err("multiple original first conversation admissions retained".into());
        }
        for flight in flights.iter() {
            if let Some(error) = flight
                .failure
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .as_ref()
            {
                return Err(error.clone());
            }
        }
        let mut captures = Vec::new();
        for flight in flights.iter() {
            let unavailable = flight
                .unavailable
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .take();
            if let Some(outcome) = unavailable {
                match outcome.into_first_conversation_recovery() {
                    Ok(capture) => captures.push(capture),
                    Err(outcome) => {
                        *flight.unavailable.lock().unwrap_or_else(|e| e.into_inner()) =
                            Some(outcome)
                    }
                }
            }
            let outcome = flight
                .outcome
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .take();
            if let Some(outcome) = outcome {
                match outcome.into_first_conversation_recovery() {
                    Ok(capture) => captures.push(capture),
                    Err(outcome) => {
                        *flight.outcome.lock().unwrap_or_else(|e| e.into_inner()) = Some(outcome)
                    }
                }
            }
        }
        Ok(captures.pop())
    }

    pub(super) fn retire(
        &self,
        cleanups: &mut Vec<crate::main_window::MainWindowInitialComposerRecoveryCleanup>,
    ) -> Result<(), String> {
        {
            let _publication = self
                .publication_gate
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            self.retired.store(true, Ordering::Release);
        }
        let flights = self
            .flights
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        for flight in &flights {
            flight.cancellation.cancel();
        }
        for flight in &flights {
            if flight
                .worker
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .take()
                .is_some_and(|w| w.join().is_err())
            {
                return Err("runtime admission retirement worker failed".into());
            }
            let mut first = flight.first.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(owner) = first.take() {
                if owner.home_failed() {
                    match owner.capture_failed_recovery() {
                        Ok(cleanup) => cleanups.push(cleanup),
                        Err((owner, error)) => {
                            *first = Some(owner);
                            return Err(error);
                        }
                    }
                } else {
                    let mut owner = owner;
                    match owner.retire_healthy() {
                        Ok(true) => {}
                        Ok(false) => {
                            *first = Some(owner);
                            return Err("first conversation cleanup remains pending".into());
                        }
                        Err(error) => {
                            *first = Some(owner);
                            return Err(error);
                        }
                    }
                }
            }
            if flight
                .outcome
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .as_ref()
                .is_some_and(|o| o.first_conversation_facts().is_some())
                && flight.home_failed()
            {
                return Err("original first conversation admission has not transferred".into());
            }
            if flight.home_failed()
                && flight
                    .unavailable
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .as_ref()
                    .is_some_and(|outcome| outcome.first_conversation_facts().is_some())
            {
                return Err("unavailable first conversation admission has not transferred".into());
            }
            flight
                .outcome
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .take();
            flight
                .unavailable
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .take();
            flight.home.lock().unwrap_or_else(|e| e.into_inner()).take();
        }
        let mut admission = self.admission.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(service) = admission.as_ref() {
            service.retire().map_err(|e| e.to_string())?;
        }
        admission.take();
        self.home.lock().unwrap_or_else(|e| e.into_inner()).take();
        self.flights
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
        Ok(())
    }
}

impl RuntimeSetupFlight {
    pub(crate) fn start_first_preparation(
        self: &Arc<Self>,
        first: crate::main_window::MainWindowFirstConversationPreparation,
    ) -> Result<
        (),
        (
            crate::main_window::MainWindowFirstConversationPreparation,
            String,
        ),
    > {
        if self.pending.swap(true, Ordering::AcqRel) {
            return Err((first, "original admission worker is still settling".into()));
        }
        if self
            .worker
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
            .is_some_and(|worker| worker.join().is_err())
        {
            self.pending.store(false, Ordering::Release);
            return Err((first, "original admission worker did not join".into()));
        }
        if let Err((first, error)) = self.retain_first(first) {
            self.pending.store(false, Ordering::Release);
            return Err((first, error));
        }
        let retained = self.clone();
        let worker = std::thread::Builder::new()
            .name("first-conversation-editor".into())
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let mut first = retained.first.lock().unwrap_or_else(|e| e.into_inner());
                    let _ = first.as_mut().unwrap().advance(&retained.cancellation);
                }));
                if result.is_err() {
                    *retained.failure.lock().unwrap_or_else(|e| e.into_inner()) =
                        Some("original first editor worker unwound".into());
                }
                retained.pending.store(false, Ordering::Release);
            });
        match worker {
            Ok(worker) => {
                *self.worker.lock().unwrap_or_else(|e| e.into_inner()) = Some(worker);
                Ok(())
            }
            Err(error) => {
                self.pending.store(false, Ordering::Release);
                let first = self
                    .first
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .take()
                    .unwrap();
                Err((first, error.to_string()))
            }
        }
    }
    pub(crate) fn retain_first(
        &self,
        first: crate::main_window::MainWindowFirstConversationPreparation,
    ) -> Result<
        (),
        (
            crate::main_window::MainWindowFirstConversationPreparation,
            String,
        ),
    > {
        let mut slot = self.first.lock().unwrap_or_else(|e| e.into_inner());
        if slot.is_some() {
            return Err((
                first,
                "first conversation preparation already retained".into(),
            ));
        }
        *slot = Some(first);
        Ok(())
    }
}

impl super::PublishedAppServices {
    pub(crate) fn runtime_setup(&self) -> Arc<RuntimeSetupService> {
        self.runtime_setup.clone()
    }
    pub(crate) fn capture_first_conversation_recovery(
        &self,
    ) -> Result<Option<FirstConversationAdmissionRecovery>, String> {
        self.runtime_setup.capture_failed_admission()
    }
}

impl super::ProcessServiceOwner {
    pub(crate) fn settle_first_conversation_cleanup(
        &self,
        access: &mut beryl_home_store::HomeCandidateRecoveryAccess<'_>,
        storage: &SyndicStorage,
        cancellation: &CommandCancellation,
    ) -> Result<(), String> {
        let mut cleanups = self
            .first_cleanups
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        for cleanup in cleanups.iter_mut() {
            match cleanup.settle(storage, access, cancellation.clone())? {
                crate::main_window::MainWindowInitialComposerRecoveryProgress::Complete => {}
                crate::main_window::MainWindowInitialComposerRecoveryProgress::Pending => {
                    return Err("original first editor cleanup remains pending".into());
                }
                crate::main_window::MainWindowInitialComposerRecoveryProgress::Unavailable(
                    error,
                ) => return Err(error),
            }
        }
        cleanups.clear();
        Ok(())
    }
}
