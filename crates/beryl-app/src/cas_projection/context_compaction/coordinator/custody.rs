use super::*;

pub(in crate::cas_projection) struct CompactionCustodyPool {
    occupied: AtomicUsize,
}

impl CompactionCustodyPool {
    #[cfg(any(test, feature = "test-faults"))]
    pub(in crate::cas_projection) fn in_use(&self) -> usize {
        self.occupied.load(Ordering::Acquire)
    }

    pub(in crate::cas_projection) fn new() -> Arc<Self> {
        Arc::new(Self {
            occupied: AtomicUsize::new(0),
        })
    }

    pub(in crate::cas_projection) fn reserve(
        self: &Arc<Self>,
    ) -> Option<CompactionCustodyReservation> {
        self.occupied
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |occupied| {
                (occupied < COMPACTION_QUEUE_CAPACITY + COMPACTION_WORKER_CAPACITY)
                    .then_some(occupied + 1)
            })
            .ok()?;
        Some(CompactionCustodyReservation(Arc::new(
            CompactionCustodySlot(Arc::clone(self)),
        )))
    }
}

pub(in crate::cas_projection) struct CompactionCustodyReservation(Arc<CompactionCustodySlot>);

impl CompactionCustodyReservation {
    pub(in crate::cas_projection) fn share(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

struct CompactionCustodySlot(Arc<CompactionCustodyPool>);

impl Drop for CompactionCustodySlot {
    fn drop(&mut self) {
        self.0.occupied.fetch_sub(1, Ordering::AcqRel);
    }
}

pub(super) struct CompactionCommandCustody {
    pub(super) command: LiveCommandPermit,
    pub(super) _reservation: CompactionCustodyReservation,
}

pub(super) struct CompactionAdmissionCustody<'a> {
    coordinator: &'a ContextCompactionCoordinator,
    local: Option<Arc<LocalCompaction>>,
    installed: bool,
}

impl<'a> CompactionAdmissionCustody<'a> {
    pub(super) fn new(
        coordinator: &'a ContextCompactionCoordinator,
        local: &Arc<LocalCompaction>,
    ) -> Self {
        Self {
            coordinator,
            local: Some(Arc::clone(local)),
            installed: false,
        }
    }

    pub(super) fn handoff(mut self) {
        self.local.take();
    }

    pub(super) fn installed(&mut self) {
        self.installed = true;
    }
}

impl Drop for CompactionAdmissionCustody<'_> {
    fn drop(&mut self) {
        if let Some(local) = self.local.take() {
            if self.installed {
                self.coordinator.fail_local(&local);
            }
            local.release_command();
        }
    }
}

impl ContextCompactionCoordinator {
    pub(super) fn reserve_command(
        &self,
        command: LiveCommandPermit,
    ) -> Result<CompactionCommandCustody, ContextCompactionError> {
        let reservation = self.custody.reserve().ok_or_else(|| {
            increment_bounded(&self.denied_admissions);
            ContextCompactionError::Unavailable
        })?;
        Ok(CompactionCommandCustody {
            command,
            _reservation: reservation,
        })
    }
}
