use super::*;
use crate::main_window::{MainWindowCreationServices, RestoredWindowActivationSource};
use beryl_model::RuntimeId;
use std::sync::Weak;
mod flights;

#[derive(Clone)]
pub(crate) struct PublishedRuntimeSetupServices {
    service: Arc<RuntimeSetupService>,
    creation: Arc<MainWindowCreationServices>,
    activation: RestoredWindowActivationSource,
    lifetime: Weak<()>,
    observer: beryl_home_store::HomeMutationObserver,
    generation: beryl_home_store::HomeGeneration,
}

pub(crate) struct PublishedRuntimeSetupObservation {
    observation: beryl_home_store::HomeMutationObservation,
    generation: beryl_home_store::HomeGeneration,
}

impl crate::app_services::ProcessServiceOwner {
    pub(crate) fn runtime_setup_services(&self) -> Option<PublishedRuntimeSetupServices> {
        let inputs = self
            .runtime_setup_inputs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()?;
        let graph = self.graph()?;
        if graph.shutdown.is_some() {
            return None;
        }
        let lifetime = Arc::downgrade(graph.restore_lifetime.as_ref()?);
        let activation = inputs.restored_activation_source;
        let creation = self
            .build_creation_services(
                inputs.request_source,
                inputs.activation_source,
                inputs.configurator_source,
            )
            .ok()?;
        let generation = creation.store.health().generation()?;
        Some(PublishedRuntimeSetupServices {
            service: graph.runtime_setup.clone(),
            creation,
            activation,
            lifetime,
            observer: graph.cas().home_mutation_observer(),
            generation,
        })
    }
}

impl PublishedRuntimeSetupServices {
    pub(crate) fn current(&self) -> bool {
        self.lifetime.upgrade().is_some()
            && !self.service.retired.load(Ordering::Acquire)
            && self.creation.validate_prepared_source().is_ok()
    }

    pub(crate) fn catalog_store(&self) -> Arc<HomeServiceReference> {
        self.creation.store.clone()
    }
    pub(crate) fn state(&self) -> BerylState {
        self.creation.state.clone()
    }

    pub(crate) fn observe(&self) -> Result<PublishedRuntimeSetupObservation, String> {
        if !self.current() {
            return Err("runtime setup services are unavailable".into());
        }
        let observation = self.observer.observe().map_err(|error| error.to_string())?;
        Ok(PublishedRuntimeSetupObservation {
            observation,
            generation: self.generation,
        })
    }

    pub(crate) fn elect<T>(
        &self,
        observation: &PublishedRuntimeSetupObservation,
        apply: impl FnOnce() -> T,
    ) -> Result<T, String> {
        let _publication = self
            .service
            .publication_gate
            .try_lock()
            .map_err(|_| "runtime setup publication is busy")?;
        if !self.current() || observation.generation != self.generation {
            return Err("runtime setup publication belongs to retired services".into());
        }
        self.creation
            .store
            .try_elect_observed_coherent(&observation.observation, observation.generation, apply)
            .map_err(|error| error.to_string())
    }

    pub(crate) fn capture_source(
        &self,
        window: WindowId,
    ) -> Result<RuntimeAdmissionSource, String> {
        if !self.current() {
            return Err("runtime setup services are unavailable".into());
        }
        self.service.capture_source(window)
    }

    pub(crate) fn start_add_runtime(
        &self,
        source: RuntimeAdmissionSource,
        members: Vec<WindowId>,
        path: PathBuf,
        form: RuntimeLaunchForm,
    ) -> Result<Arc<RuntimeSetupFlight>, String> {
        if !self.current() {
            return Err("runtime setup services are unavailable".into());
        }
        self.service.start_add_runtime(source, members, path, form)
    }

    pub(crate) fn start_add_root(
        &self,
        source: RuntimeAdmissionSource,
        members: Vec<WindowId>,
        runtime: RuntimeId,
        path: PathBuf,
    ) -> Result<Arc<RuntimeSetupFlight>, String> {
        self.start(move |admission, cancellation| {
            admission.add_root(source, &members, runtime, &path, cancellation)
        })
    }

    pub(crate) fn start_add_runtime_for_window(
        &self,
        window: WindowId,
        members: Vec<WindowId>,
        path: PathBuf,
        form: RuntimeLaunchForm,
    ) -> Result<Arc<RuntimeSetupFlight>, String> {
        self.start(
            move |admission, cancellation| match admission.capture_source(window) {
                Ok(source) => admission.add_runtime(source, &members, &path, form, cancellation),
                Err(error) => RuntimeAdmissionOutcome::NotCommitted { error },
            },
        )
    }

    pub(crate) fn start_add_root_for_window(
        &self,
        window: WindowId,
        members: Vec<WindowId>,
        runtime: RuntimeId,
        path: PathBuf,
    ) -> Result<Arc<RuntimeSetupFlight>, String> {
        self.start(
            move |admission, cancellation| match admission.capture_source(window) {
                Ok(source) => admission.add_root(source, &members, runtime, &path, cancellation),
                Err(error) => RuntimeAdmissionOutcome::NotCommitted { error },
            },
        )
    }

    fn start(
        &self,
        operation: impl FnOnce(
            Arc<RuntimeAdmissionService>,
            CommandCancellation,
        ) -> RuntimeAdmissionOutcome
        + Send
        + 'static,
    ) -> Result<Arc<RuntimeSetupFlight>, String> {
        if !self.current() {
            return Err("runtime setup services are unavailable".into());
        }
        let mut flights = self
            .service
            .flights
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        flights.retain(|flight| flight.has_custody());
        if self.service.retired.load(Ordering::Acquire)
            || flights.len() >= beryl_state::MAX_RESTORABLE_WINDOWS
        {
            return Err("runtime setup services are retired or full".into());
        }
        let admission = self
            .service
            .admission
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .ok_or("runtime admission source retired")?
            .clone();
        let flight = Arc::new(RuntimeSetupFlight {
            home: Mutex::new(Some(self.creation.store.clone())),
            retired: self.service.retired.clone(),
            publication_gate: self.service.publication_gate.clone(),
            cancellation: CommandCancellation::new(),
            pending: AtomicBool::new(false),
            worker: Mutex::new(None),
            outcome: Mutex::new(None),
            first: Mutex::new(None),
            failure: Mutex::new(None),
            preparation_error: Mutex::new(None),
            unavailable: Mutex::new(None),
            revalidation_ready: AtomicBool::new(false),
        });
        flight.start_worker("runtime-setup-admission", move |flight| {
            let outcome = operation(admission, flight.cancellation.clone());
            *flight.outcome.lock().unwrap_or_else(|e| e.into_inner()) = Some(outcome);
            Ok(())
        })?;
        flights.push(flight.clone());
        Ok(flight)
    }

    pub(crate) fn prepare_first_conversation(
        &self,
        flight: &Arc<RuntimeSetupFlight>,
    ) -> Result<bool, String> {
        self.authenticate_flight(flight)?;
        if flight.is_pending() {
            return Ok(false);
        }
        flight.worker_failure()?;
        if flight
            .first
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .is_some_and(|first| first.is_ready())
        {
            return Ok(true);
        }
        let creation = self.creation.clone();
        let activation = self.activation.clone();
        flight.start_worker("first-conversation-prepare", move |flight| {
            let mut first = flight.first.lock().unwrap_or_else(|e| e.into_inner());
            if first.is_none() {
                let outcome = flight.outcome.lock().unwrap_or_else(|e| e.into_inner());
                let Some(RuntimeAdmissionOutcome::Committed { admission, .. }) = outcome.as_ref()
                else {
                    return Err("first conversation requires retained committed admission".into());
                };
                admission
                    .validate_publication()
                    .map_err(|e| e.to_string())?;
                let facts = admission
                    .facts()
                    .onboarding()
                    .ok_or("admission has no first conversation")?;
                let (request, disposal) = activation(facts.window())?;
                *first = Some(
                    crate::main_window::MainWindowFirstConversationPreparation::new(
                        creation.store.clone(),
                        &creation.state,
                        creation.storage.clone(),
                        facts.window().clone(),
                        facts.thread_id(),
                        facts.draft_id(),
                        request,
                        disposal,
                    )?,
                );
            }
            let _ready = first
                .as_mut()
                .unwrap()
                .prepare_mount(&creation, &flight.cancellation)?;
            Ok(())
        })?;
        Ok(false)
    }

    pub(crate) fn revalidate_first_conversation(
        &self,
        flight: &Arc<RuntimeSetupFlight>,
    ) -> Result<bool, String> {
        self.authenticate_flight(flight)?;
        if flight.is_pending() {
            return Ok(false);
        }
        flight.worker_failure()?;
        if flight.is_first_publication_ready() {
            return Ok(true);
        }
        flight.start_worker("first-conversation-publication", |flight| {
            let outcome = flight.outcome.lock().unwrap_or_else(|e| e.into_inner());
            let Some(RuntimeAdmissionOutcome::Committed { admission, .. }) = outcome.as_ref()
            else {
                return Err("first conversation original admission is unavailable".into());
            };
            admission
                .validate_publication()
                .map_err(|e| e.to_string())?;
            flight
                .first
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .as_mut()
                .ok_or("first conversation preparation is unavailable")?
                .revalidate_publication()?;
            flight.revalidation_ready.store(true, Ordering::Release);
            Ok(())
        })?;
        Ok(false)
    }

    fn authenticate_flight(&self, flight: &Arc<RuntimeSetupFlight>) -> Result<(), String> {
        if !self.current()
            || !self
                .service
                .flights
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .iter()
                .any(|retained| Arc::ptr_eq(retained, flight))
        {
            return Err("runtime setup flight belongs to another or retired graph".into());
        }
        Ok(())
    }
}
