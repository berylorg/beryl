use std::num::NonZeroU16;
use std::time::{Duration, Instant};

use beryl_home_store::{CommandCancellation, HomeGeneration, HomeStore};
use beryl_model::BerylHomeId;
use beryl_state::AssetState;
use syndic_storage::{DraftPieceOperationIdV1, SyndicTimestamp};

use crate::composer_marker_seal::DraftMarkerSealService;

use super::publication::{PendingPublication, PublicationStage};
use super::{
    ComposerHostBinding, ComposerHostDisposalCompletion, ComposerHostDisposalTicket,
    ComposerHostError, ComposerHostMarkerSealAuthority, ComposerHostPublicationCapture,
    ComposerHostPublicationCompletion, ComposerHostPublicationDrive, ComposerHostPublicationLane,
    ComposerHostPublicationTicket, ComposerHostPublicationUnavailable, SyndicComposerHost,
};

mod autosave;
mod flush;
mod service;
mod settlement;

use settlement::{
    PublicationStep, error_failure, publication_failure, recoverable_error, stale_callback_error,
};

pub const DEFAULT_DRAFT_AUTOSAVE_INTERVAL_SECONDS: u16 = 30;
pub const MIN_DRAFT_AUTOSAVE_INTERVAL_SECONDS: u16 = 5;
pub const MAX_DRAFT_AUTOSAVE_INTERVAL_SECONDS: u16 = 300;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ComposerHostAutosaveInterval(NonZeroU16);

impl ComposerHostAutosaveInterval {
    pub const fn new(seconds: u16) -> Option<Self> {
        if seconds < MIN_DRAFT_AUTOSAVE_INTERVAL_SECONDS
            || seconds > MAX_DRAFT_AUTOSAVE_INTERVAL_SECONDS
        {
            return None;
        }
        match NonZeroU16::new(seconds) {
            Some(seconds) => Some(Self(seconds)),
            None => None,
        }
    }

    pub const fn seconds(self) -> u16 {
        self.0.get()
    }

    fn duration(self) -> Duration {
        Duration::from_secs(u64::from(self.seconds()))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ComposerHostAutosaveTimer {
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    host_generation: super::ComposerHostGeneration,
    timer_generation: u64,
    settings_generation: u64,
    deadline: Instant,
}

impl ComposerHostAutosaveTimer {
    pub const fn timer_generation(self) -> u64 {
        self.timer_generation
    }

    pub const fn settings_generation(self) -> u64 {
        self.settings_generation
    }

    pub const fn deadline(self) -> Instant {
        self.deadline
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComposerHostAutosaveSettingsCompletion {
    Published(Option<ComposerHostAutosaveTimer>),
    Stale,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComposerHostAutosaveCapture {
    Stale,
    PublicationPending,
    Clean,
    Cancelled,
    Captured(ComposerHostPublicationTicket),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComposerHostAutosaveAdvance {
    Progress,
    Ready,
    ReconciliationPending,
    Saved { dirty_successor: bool },
    Unsatisfied(ComposerHostFlushFailure),
    Stale,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComposerHostServiceDisposalCompletion {
    Pending,
    Disposed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComposerHostFlushPurpose {
    ThreadSwitch,
    WindowClose,
    ApplicationExit,
    Submission,
    Release,
}

impl ComposerHostFlushPurpose {
    const fn disposes_session(self) -> bool {
        !matches!(self, Self::Submission)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ComposerHostFlushTicket {
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    host_generation: super::ComposerHostGeneration,
    barrier_generation: u64,
}

impl ComposerHostFlushTicket {
    pub const fn barrier_generation(self) -> u64 {
        self.barrier_generation
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComposerHostFlushState {
    CaptureRequired,
    PublicationPending,
    DisposalRequired,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComposerHostFlushAdmission {
    Started {
        ticket: ComposerHostFlushTicket,
        state: ComposerHostFlushState,
    },
    Joined {
        ticket: ComposerHostFlushTicket,
        state: ComposerHostFlushState,
    },
    Satisfied(ComposerHostFlushPurpose),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComposerHostFlushCapture {
    Captured(ComposerHostPublicationTicket),
    State(ComposerHostFlushState),
    Satisfied(ComposerHostFlushPurpose),
    Unsatisfied(ComposerHostFlushFailure),
    Stale,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComposerHostFlushFailure {
    Cancelled,
    NotCommitted,
    Recoverable,
    DurableBaseConflict,
    SessionDisposed,
    IdentityCollision,
    ReconciliationCollision,
    DisposalDirtyConflict,
    GenerationLost,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComposerHostFlushAdvance {
    Progress(ComposerHostFlushState),
    ReconciliationPending,
    Satisfied(ComposerHostFlushPurpose),
    Unsatisfied(ComposerHostFlushFailure),
    Stale,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ComposerHostLifecycleDiagnostics {
    timers: usize,
    barriers: usize,
    joined_publications: usize,
    publication_ready: bool,
    joined_publication_ticket: Option<ComposerHostPublicationTicket>,
    last_publication_completion: Option<ComposerHostPublicationCompletion>,
}

impl ComposerHostLifecycleDiagnostics {
    pub const fn timers(self) -> usize {
        self.timers
    }

    pub const fn barriers(self) -> usize {
        self.barriers
    }

    pub const fn joined_publications(self) -> usize {
        self.joined_publications
    }

    pub const fn publication_ready(self) -> bool {
        self.publication_ready
    }

    pub const fn joined_publication_ticket(self) -> Option<ComposerHostPublicationTicket> {
        self.joined_publication_ticket
    }

    pub const fn last_publication_completion(self) -> Option<ComposerHostPublicationCompletion> {
        self.last_publication_completion
    }
}

pub(super) struct ComposerHostLifecycleCoordinator {
    interval: ComposerHostAutosaveInterval,
    settings_generation: u64,
    timer_generation: u64,
    timer: Option<ComposerHostAutosaveTimer>,
    barrier_generation: u64,
    barrier: Option<PendingFlushBarrier>,
    autosave: Option<PendingSave>,
    dirty_adoption_seen: bool,
    service_disposed: bool,
    last_publication_completion: Option<ComposerHostPublicationCompletion>,
}

struct PendingSave {
    ticket: ComposerHostPublicationTicket,
    timer_generation: Option<u64>,
    failure: Option<ComposerHostFlushFailure>,
}

struct PendingFlushBarrier {
    ticket: ComposerHostFlushTicket,
    purpose: ComposerHostFlushPurpose,
    publication: Option<PendingSave>,
    disposal: Option<ComposerHostDisposalTicket>,
}

impl ComposerHostLifecycleCoordinator {
    pub(super) const fn new() -> Self {
        Self {
            interval: ComposerHostAutosaveInterval::new(DEFAULT_DRAFT_AUTOSAVE_INTERVAL_SECONDS)
                .unwrap(),
            settings_generation: 0,
            timer_generation: 0,
            timer: None,
            barrier_generation: 0,
            barrier: None,
            autosave: None,
            dirty_adoption_seen: false,
            service_disposed: false,
            last_publication_completion: None,
        }
    }

    pub(super) fn activate(&mut self) {
        self.timer = None;
        self.barrier = None;
        self.autosave = None;
        self.dirty_adoption_seen = false;
        self.last_publication_completion = None;
    }

    pub(super) const fn is_service_disposed(&self) -> bool {
        self.service_disposed
    }

    pub(super) const fn has_barrier(&self) -> bool {
        self.barrier.is_some()
    }

    pub(super) fn freezes_admission(&self) -> bool {
        self.barrier
            .as_ref()
            .is_some_and(|barrier| barrier.purpose.disposes_session())
    }

    pub(super) fn adopted(&mut self, binding: ComposerHostBinding, _became_dirty: bool) {
        if !self.dirty_adoption_seen {
            self.dirty_adoption_seen = true;
        }
        if self.timer.is_none() && self.barrier.is_none() && self.autosave.is_none() {
            let _ = self.arm(binding, Instant::now());
        }
    }

    fn arm(
        &mut self,
        binding: ComposerHostBinding,
        anchor: Instant,
    ) -> Option<ComposerHostAutosaveTimer> {
        if self.service_disposed {
            self.timer = None;
            return None;
        }
        let Some(generation) = self.timer_generation.checked_add(1) else {
            self.timer = None;
            return None;
        };
        let Some(deadline) = anchor.checked_add(self.interval.duration()) else {
            self.timer = None;
            return None;
        };
        self.timer_generation = generation;
        let timer = ComposerHostAutosaveTimer {
            home_id: binding.home_id(),
            home_generation: binding.home_generation(),
            host_generation: binding.host_generation(),
            timer_generation: generation,
            settings_generation: self.settings_generation,
            deadline,
        };
        self.timer = Some(timer);
        Some(timer)
    }

    fn timer_matches(&self, timer: ComposerHostAutosaveTimer) -> bool {
        self.timer == Some(timer)
    }

    fn barrier_matches(&self, ticket: ComposerHostFlushTicket) -> bool {
        self.barrier
            .as_ref()
            .is_some_and(|barrier| barrier.ticket == ticket)
    }

    fn callback_store_matches(binding: ComposerHostBinding, store: &HomeStore) -> bool {
        if store.home_id() != binding.home_id() {
            return false;
        }
        let health = store.health();
        health.state() == beryl_home_store::HomeHealthState::Healthy
            && health.generation() == Some(binding.home_generation())
    }

    pub(super) fn clear_runtime(&mut self) {
        self.timer = None;
        self.barrier = None;
        self.autosave = None;
    }
}

impl SyndicComposerHost {
    pub const fn autosave_timer(&self) -> Option<ComposerHostAutosaveTimer> {
        self.lifecycle.timer
    }

    pub const fn autosave_interval(&self) -> ComposerHostAutosaveInterval {
        self.lifecycle.interval
    }

    pub fn lifecycle_diagnostics(&self) -> ComposerHostLifecycleDiagnostics {
        let barrier_publication = self
            .lifecycle
            .barrier
            .as_ref()
            .is_some_and(|barrier| barrier.publication.is_some());
        ComposerHostLifecycleDiagnostics {
            timers: usize::from(self.lifecycle.timer.is_some()),
            barriers: usize::from(self.lifecycle.barrier.is_some()),
            joined_publications: usize::from(
                self.lifecycle.autosave.is_some() || barrier_publication,
            ),
            publication_ready: matches!(
                self.publication.lane.as_deref(),
                Some(ComposerHostPublicationLane::Publication(
                    PendingPublication {
                        stage: PublicationStage::Ready(_),
                        ..
                    }
                ))
            ),
            joined_publication_ticket: self.lifecycle.autosave.as_ref().map(|save| save.ticket).or(
                self.lifecycle
                    .barrier
                    .as_ref()
                    .and_then(|barrier| barrier.publication.as_ref().map(|save| save.ticket)),
            ),
            last_publication_completion: self.lifecycle.last_publication_completion,
        }
    }

    fn validate_lifecycle_binding(
        &self,
        binding: ComposerHostBinding,
    ) -> Result<(), ComposerHostError> {
        let active = self.active.as_ref().ok_or(ComposerHostError::OldBinding)?;
        if self.lifecycle.service_disposed || active.session_disposed {
            return Err(ComposerHostError::PublicationUnavailable);
        }
        if active.binding.host_generation() != binding.host_generation()
            || active.binding.home_id() != binding.home_id()
            || active.binding.home_generation() != binding.home_generation()
        {
            return Err(ComposerHostError::OldBinding);
        }
        Ok(())
    }

    fn validate_timer_binding(
        &self,
        timer: ComposerHostAutosaveTimer,
    ) -> Result<ComposerHostBinding, ComposerHostError> {
        let active = self.active.as_ref().ok_or(ComposerHostError::OldBinding)?;
        if active.binding.home_id() != timer.home_id
            || active.binding.home_generation() != timer.home_generation
            || active.binding.host_generation() != timer.host_generation
        {
            return Err(ComposerHostError::StalePublicationGeneration);
        }
        Ok(active.binding)
    }
}
