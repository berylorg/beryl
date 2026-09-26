use super::*;
use crate::process_admission::ProcessAdmissionError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WindowCloseAdmissionError {
    Process(ProcessAdmissionError),
    Busy,
    WindowSetChanged,
    Unavailable,
}

impl From<ProcessAdmissionError> for WindowCloseAdmissionError {
    fn from(error: ProcessAdmissionError) -> Self {
        Self::Process(error)
    }
}

pub(crate) struct WindowCloseSnapshot {
    registry: Arc<Mutex<AcquisitionFlights>>,
    revision: Arc<()>,
    members: HashSet<WindowId>,
}

pub(crate) struct WindowCloseLease {
    snapshot: WindowCloseSnapshot,
    owner: Arc<()>,
    process: crate::process_admission::ProcessAdmissionGate,
}

impl RuntimeBackedWindowProcessRegistry {
    pub(crate) fn snapshot_for_close(
        &self,
        resident_windows: &[WindowId],
    ) -> Result<WindowCloseSnapshot, WindowCloseAdmissionError> {
        if resident_windows.is_empty() || resident_windows.len() > MAX_RESTORABLE_WINDOWS {
            return Err(WindowCloseAdmissionError::WindowSetChanged);
        }
        let members: HashSet<_> = resident_windows.iter().copied().collect();
        if members.len() != resident_windows.len() {
            return Err(WindowCloseAdmissionError::WindowSetChanged);
        }
        self.process_admission
            .admit(|| {
                let registry = self
                    .flights
                    .lock()
                    .map_err(|_| WindowCloseAdmissionError::Unavailable)?;
                if registry.close_owner.is_some() {
                    return Err(WindowCloseAdmissionError::Busy);
                }
                if registry.main_window_reservations != members {
                    return Err(WindowCloseAdmissionError::WindowSetChanged);
                }
                Ok(WindowCloseSnapshot {
                    registry: self.flights.clone(),
                    revision: registry.membership_revision.clone(),
                    members,
                })
            })
            .map_err(WindowCloseAdmissionError::Process)?
    }

    pub(crate) fn admit_close(
        &self,
        snapshot: WindowCloseSnapshot,
    ) -> Result<WindowCloseLease, WindowCloseAdmissionError> {
        if !Arc::ptr_eq(&self.flights, &snapshot.registry) {
            return Err(WindowCloseAdmissionError::WindowSetChanged);
        }
        self.process_admission
            .admit(|| {
                let mut registry = self
                    .flights
                    .lock()
                    .map_err(|_| WindowCloseAdmissionError::Unavailable)?;
                if registry.close_owner.is_some() {
                    return Err(WindowCloseAdmissionError::Busy);
                }
                if !Arc::ptr_eq(&registry.membership_revision, &snapshot.revision) {
                    return Err(WindowCloseAdmissionError::WindowSetChanged);
                }
                let owner = Arc::new(());
                registry.close_owner = Some(owner.clone());
                Ok(WindowCloseLease {
                    snapshot,
                    owner,
                    process: self.process_admission.clone(),
                })
            })
            .map_err(WindowCloseAdmissionError::Process)?
    }
}

impl WindowCloseLease {
    pub(crate) fn validate(&self) -> Result<(), WindowCloseAdmissionError> {
        let registry = self
            .snapshot
            .registry
            .lock()
            .map_err(|_| WindowCloseAdmissionError::Unavailable)?;
        if !registry
            .close_owner
            .as_ref()
            .is_some_and(|owner| Arc::ptr_eq(owner, &self.owner))
            || !Arc::ptr_eq(&registry.membership_revision, &self.snapshot.revision)
        {
            return Err(WindowCloseAdmissionError::WindowSetChanged);
        }
        Ok(())
    }

    pub(crate) fn is_final(&self, window: WindowId) -> Result<bool, WindowCloseAdmissionError> {
        self.validate()?;
        if !self.snapshot.members.contains(&window) {
            return Err(WindowCloseAdmissionError::WindowSetChanged);
        }
        Ok(self.snapshot.members.len() == 1)
    }
}

impl Drop for WindowCloseLease {
    fn drop(&mut self) {
        self.process.settle(|| {
            let mut registry = self
                .snapshot
                .registry
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if registry
                .close_owner
                .as_ref()
                .is_some_and(|owner| Arc::ptr_eq(owner, &self.owner))
            {
                registry.close_owner = None;
            }
        });
    }
}

#[cfg(test)]
#[path = "../../tests/unit/window_close_admission.rs"]
mod tests;
