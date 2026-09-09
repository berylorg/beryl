use super::*;
use crate::cas_projection::compaction_work::source::{
    CompactionCommandObservation, CompactionWorkHandle, CompactionWorkSource,
    ContinuationObservation,
};

pub(in crate::cas_projection) struct CompactionCustodyPool {
    occupied: AtomicUsize,
    pub(in crate::cas_projection) source: Arc<CompactionWorkSource>,
}

impl CompactionCustodyPool {
    #[cfg(any(test, feature = "test-faults"))]
    pub(in crate::cas_projection) fn in_use(&self) -> usize {
        self.occupied.load(Ordering::Acquire)
    }

    pub(in crate::cas_projection) fn new() -> Arc<Self> {
        Arc::new(Self {
            occupied: AtomicUsize::new(0),
            source: CompactionWorkSource::new(
                COMPACTION_QUEUE_CAPACITY + 2 * COMPACTION_WORKER_CAPACITY,
            ),
        })
    }

    #[cfg(any(test, feature = "test-faults"))]
    pub(in crate::cas_projection) fn reserve(
        self: &Arc<Self>,
    ) -> Option<CompactionCustodyReservation> {
        self.reserve_inner(None)
    }

    pub(in crate::cas_projection) fn reserve_continuation(
        self: &Arc<Self>,
        thread: SyndicThreadId,
        turn: SyndicTurnId,
    ) -> Option<CompactionCustodyReservation> {
        self.reserve_inner(Some((thread, Some(turn))))
    }

    fn reserve_inner(
        self: &Arc<Self>,
        identity: Option<(SyndicThreadId, Option<SyndicTurnId>)>,
    ) -> Option<CompactionCustodyReservation> {
        self.occupied
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |occupied| {
                (occupied < COMPACTION_QUEUE_CAPACITY + COMPACTION_WORKER_CAPACITY)
                    .then_some(occupied + 1)
            })
            .ok()?;
        let observation = identity
            .map_or_else(CompactionWorkHandle::unavailable, |(thread, turn)| {
                self.source.begin(thread, turn)
            });
        Some(CompactionCustodyReservation(Arc::new(
            CompactionCustodySlot {
                pool: Arc::clone(self),
                observation,
            },
        )))
    }
}

pub(in crate::cas_projection) struct CompactionCustodyReservation(Arc<CompactionCustodySlot>);

impl CompactionCustodyReservation {
    pub(in crate::cas_projection) fn prepare_shared_command(&self) -> CompactionPreparationCustody {
        let reservation = Self(Arc::clone(&self.0));
        let observation = self.0.observation.begin_shared_command();
        CompactionPreparationCustody {
            observation,
            _reservation: reservation,
        }
    }

    pub(in crate::cas_projection) fn continuation_observation(&self) -> ContinuationObservation {
        self.0.observation.continuation_observation()
    }

    pub(super) fn prepare_command(self) -> CompactionPreparationCustody {
        let observation = self.0.observation.command_observation();
        CompactionPreparationCustody {
            observation,
            _reservation: self,
        }
    }
}

struct CompactionCustodySlot {
    pool: Arc<CompactionCustodyPool>,
    observation: CompactionWorkHandle,
}

impl Drop for CompactionCustodySlot {
    fn drop(&mut self) {
        self.observation.retire_slot();
        self.pool.occupied.fetch_sub(1, Ordering::AcqRel);
    }
}

pub(super) struct CompactionCommandCustody {
    pub(super) command: LiveCommandPermit,
    pub(super) preparation: CompactionPreparationCustody,
}

pub(in crate::cas_projection) struct CompactionPreparationCustody {
    observation: CompactionCommandObservation,
    _reservation: CompactionCustodyReservation,
}

impl CompactionCommandCustody {
    pub(super) fn observation(&self) -> CompactionWorkHandle {
        self.preparation.observation.handle()
    }
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
        thread: SyndicThreadId,
    ) -> Result<CompactionCommandCustody, ContextCompactionError> {
        let reservation = self
            .custody
            .reserve_inner(Some((thread, None)))
            .ok_or_else(|| {
                increment_bounded(&self.denied_admissions);
                ContextCompactionError::Unavailable
            })?;
        Ok(CompactionCommandCustody {
            command,
            preparation: reservation.prepare_command(),
        })
    }
}
