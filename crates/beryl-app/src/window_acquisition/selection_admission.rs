use super::*;
use crate::process_admission::{ProcessAdmissionError, ProcessAdmissionReservation};

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub(crate) enum WindowSelectionAdmissionError {
    #[error(transparent)]
    Process(#[from] ProcessAdmissionError),
    #[error("window selection lifecycle is busy")]
    Busy,
    #[error("window selection source membership changed")]
    WindowSetChanged,
    #[error("window selection registry is unavailable")]
    Unavailable,
}

pub(crate) struct WindowSelectionLease {
    registry: Arc<Mutex<AcquisitionFlights>>,
    process: crate::process_admission::ProcessAdmissionGate,
    membership: Arc<()>,
    owner: Arc<()>,
    invoking: WindowId,
    reservation: Option<ProcessAdmissionReservation>,
    retired: bool,
}

pub(crate) struct RetiredWindowSelectionLease {
    registry: Arc<Mutex<AcquisitionFlights>>,
    process: crate::process_admission::ProcessAdmissionGate,
    membership: Arc<()>,
    owner: Arc<()>,
    invoking: WindowId,
    home: beryl_model::BerylHomeId,
    generation: beryl_home_store::HomeGeneration,
    path: std::path::PathBuf,
}

pub(crate) struct PreparedRetiredWindowSelectionRelease<'a> {
    registry: std::sync::MutexGuard<'a, AcquisitionFlights>,
}

impl PreparedRetiredWindowSelectionRelease<'_> {
    pub(crate) fn apply(mut self) {
        self.registry.selection_owner = None;
    }
}

impl RuntimeBackedWindowProcessRegistry {
    pub(crate) fn admit_selection(
        &self,
        resident_windows: &[WindowId],
        invoking: WindowId,
    ) -> Result<WindowSelectionLease, WindowSelectionAdmissionError> {
        if resident_windows.is_empty() || resident_windows.len() > MAX_RESTORABLE_WINDOWS {
            return Err(WindowSelectionAdmissionError::WindowSetChanged);
        }
        let members: HashSet<_> = resident_windows.iter().copied().collect();
        if members.len() != resident_windows.len() || !members.contains(&invoking) {
            return Err(WindowSelectionAdmissionError::WindowSetChanged);
        }
        let (reservation, (membership, owner)) = self
            .process_admission
            .try_reserve_with(|| {
                let mut registry = self.flights.try_lock().map_err(|error| match error {
                    std::sync::TryLockError::WouldBlock => WindowSelectionAdmissionError::Busy,
                    std::sync::TryLockError::Poisoned(_) => {
                        WindowSelectionAdmissionError::Unavailable
                    }
                })?;
                if registry.close_owner.is_some()
                    || registry.selection_owner.is_some()
                    || !registry.active.is_empty()
                {
                    return Err(WindowSelectionAdmissionError::Busy);
                }
                if registry.main_window_reservations != members {
                    return Err(WindowSelectionAdmissionError::WindowSetChanged);
                }
                let owner = Arc::new(());
                registry.selection_owner = Some(owner.clone());
                Ok((registry.membership_revision.clone(), owner))
            })
            .ok_or(WindowSelectionAdmissionError::Busy)???;
        Ok(WindowSelectionLease {
            registry: self.flights.clone(),
            process: self.process_admission.clone(),
            membership,
            owner,
            invoking,
            reservation: Some(reservation),
            retired: false,
        })
    }
}

impl WindowSelectionLease {
    pub(crate) fn retire_failed_home(
        mut self: Arc<Self>,
        home: &beryl_home_store::HomeStore,
        generation: beryl_home_store::HomeGeneration,
        invoking: WindowId,
    ) -> Result<RetiredWindowSelectionLease, (Arc<Self>, String)> {
        let health = home.health();
        if health.state() != beryl_home_store::HomeHealthState::Failed
            || health.generation() != Some(generation)
            || self.invoking != invoking
        {
            return Err((
                self,
                "selection retirement does not match its failed home transfer".into(),
            ));
        }
        let validation = self
            .registry
            .try_lock()
            .map_err(|_| "selection retirement registry is busy".to_owned())
            .and_then(|registry| self.validate(&registry).map_err(|error| error.to_string()));
        if let Err(error) = validation {
            return Err((self, error));
        }
        let Some(lease) = Arc::get_mut(&mut self) else {
            return Err((
                self,
                "selection retirement still has original owners".into(),
            ));
        };
        let retired = RetiredWindowSelectionLease {
            registry: lease.registry.clone(),
            process: lease.process.clone(),
            membership: lease.membership.clone(),
            owner: lease.owner.clone(),
            invoking,
            home: home.home_id(),
            generation,
            path: home.canonical_path().to_owned(),
        };
        lease.retired = true;
        drop(lease.reservation.take());
        Ok(retired)
    }
    pub(crate) fn invoking(&self) -> WindowId {
        self.invoking
    }

    fn validate(&self, registry: &AcquisitionFlights) -> Result<(), WindowSelectionAdmissionError> {
        if registry.close_owner.is_some()
            || !registry.active.is_empty()
            || !registry
                .selection_owner
                .as_ref()
                .is_some_and(|owner| Arc::ptr_eq(owner, &self.owner))
            || !Arc::ptr_eq(&registry.membership_revision, &self.membership)
            || !registry.main_window_reservations.contains(&self.invoking)
        {
            return Err(WindowSelectionAdmissionError::WindowSetChanged);
        }
        Ok(())
    }

    pub(crate) fn admit_commit<T>(
        &self,
        commit: impl FnOnce() -> T,
    ) -> Result<T, WindowSelectionAdmissionError> {
        self.process.admit(|| {
            let registry = self
                .registry
                .lock()
                .map_err(|_| WindowSelectionAdmissionError::Unavailable)?;
            self.validate(&registry)?;
            Ok(commit())
        })?
    }

    pub(crate) fn validate_publication(&self) -> Result<(), WindowSelectionAdmissionError> {
        self.process
            .try_admit(|| {
                let registry = self.registry.try_lock().map_err(|error| match error {
                    std::sync::TryLockError::WouldBlock => WindowSelectionAdmissionError::Busy,
                    std::sync::TryLockError::Poisoned(_) => {
                        WindowSelectionAdmissionError::Unavailable
                    }
                })?;
                self.validate(&registry)
            })
            .ok_or(WindowSelectionAdmissionError::Busy)?
    }
}

impl Drop for WindowSelectionLease {
    fn drop(&mut self) {
        if self.retired {
            return;
        }
        self.process.settle(|| {
            let mut registry = self
                .registry
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if registry
                .selection_owner
                .as_ref()
                .is_some_and(|owner| Arc::ptr_eq(owner, &self.owner))
            {
                registry.selection_owner = None;
            }
        });
    }
}

impl RetiredWindowSelectionLease {
    pub(crate) fn invoking(&self) -> WindowId {
        self.invoking
    }
    pub(crate) fn validate_candidate(
        &self,
        access: &beryl_home_store::HomeCandidateRecoveryAccess<'_>,
    ) -> Result<(), String> {
        if access.home_id() != self.home
            || access.canonical_path() != self.path
            || access.generation() == self.generation
        {
            return Err("retained selection exclusion has another recovery candidate".into());
        }
        self.validate_owner()
    }
    pub(crate) fn validate_owner(&self) -> Result<(), String> {
        let registry = self
            .registry
            .try_lock()
            .map_err(|_| "retained selection registry is busy")?;
        self.validate_registry(&registry)
    }
    fn validate_registry(&self, registry: &AcquisitionFlights) -> Result<(), String> {
        if registry.close_owner.is_some()
            || !registry.active.is_empty()
            || !registry
                .selection_owner
                .as_ref()
                .is_some_and(|owner| Arc::ptr_eq(owner, &self.owner))
            || !Arc::ptr_eq(&registry.membership_revision, &self.membership)
            || !registry.main_window_reservations.contains(&self.invoking)
        {
            return Err("retained selection exclusion membership changed".into());
        }
        Ok(())
    }
    pub(crate) fn prepare_coherent_release(
        &self,
    ) -> Result<PreparedRetiredWindowSelectionRelease<'_>, String> {
        let registry = self
            .registry
            .try_lock()
            .map_err(|_| "retained selection registry is busy")?;
        self.validate_registry(&registry)?;
        Ok(PreparedRetiredWindowSelectionRelease { registry })
    }
    pub(crate) fn release(self) -> Result<(), (Self, String)> {
        match self.validate_owner() {
            Ok(()) => Ok(()),
            Err(error) => Err((self, error)),
        }
    }
}

impl Drop for RetiredWindowSelectionLease {
    fn drop(&mut self) {
        self.process.settle(|| {
            let mut registry = self
                .registry
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if registry
                .selection_owner
                .as_ref()
                .is_some_and(|owner| Arc::ptr_eq(owner, &self.owner))
            {
                registry.selection_owner = None;
            }
        });
    }
}

#[cfg(feature = "test-faults")]
pub struct WindowSelectionAdmissionTestProbe {
    lease: WindowSelectionLease,
}

#[cfg(feature = "test-faults")]
impl RuntimeBackedWindowProcessRegistry {
    pub fn test_admit_selection(
        &self,
        resident_windows: &[WindowId],
        invoking: WindowId,
    ) -> Result<WindowSelectionAdmissionTestProbe, String> {
        self.admit_selection(resident_windows, invoking)
            .map(|lease| WindowSelectionAdmissionTestProbe { lease })
            .map_err(|error| error.to_string())
    }

    pub fn test_close_is_blocked(&self, resident_windows: &[WindowId]) -> bool {
        self.snapshot_for_close(resident_windows).is_err()
    }

    pub fn test_acquisition_is_blocked(&self, window: WindowId) -> bool {
        AcquisitionFlight::acquire(self.flights.clone(), window).is_err()
    }

    pub fn test_process_closing_is_blocked(&self) -> bool {
        self.process_admission.prepare_closing().is_err()
    }
}

#[cfg(feature = "test-faults")]
impl WindowSelectionAdmissionTestProbe {
    pub fn validate_publication(&self) -> Result<(), String> {
        self.lease
            .validate_publication()
            .map_err(|error| error.to_string())
    }

    pub fn admit_commit<T>(&self, commit: impl FnOnce() -> T) -> Result<T, String> {
        self.lease
            .admit_commit(commit)
            .map_err(|error| error.to_string())
    }

    pub fn test_replace_owner(&self) {
        self.lease.registry.lock().unwrap().selection_owner = Some(Arc::new(()));
    }
}
