use std::{
    num::{NonZeroU64, NonZeroUsize},
    time::Duration,
};

use beryl_home_store::HomeStore;
use beryl_model::DomainRevision;
use beryl_state::{
    InstalledThemeId, PreparedThemeAppearance, SettingRecord, ThemeChangeHint,
    ThemeChangeSubscription, ThemeDocumentIdentity, ThemeDocumentRevision, ThemeDraftIdentity,
    ThemeDraftRevision, ThemeLoadFailure, ThemeManifestGeneration, ThemeManifestReadLimits,
    ThemePageLimits, ThemeReconciliation, ThemeReferenceSnapshotProvider, ThemeRepositoryCommand,
    ThemeRepositoryCommit, ThemeRepositoryObservation, ThemeRepositoryOperationOutcome,
    ThemeService, ThemeServiceDiagnostics, ThemeSettingsIdentity, ThemeStartupOutcome,
};

use super::{
    AdapterRegistrationError, AppearanceCoordinator, AppearanceCoordinatorConfig,
    AppearanceDiagnostics, AppearanceGeneration, AppearanceWindowAdapter,
    DurablePublicationIdentity, DurablePublicationOutcome, DurableRetryOutcome,
    PreparedPreviewAppearance, PreviewCandidateIdentity, PreviewPublicationError,
    PreviewPublicationRequest, PreviewPublicationResult, PreviewSource, StopPreviewResult,
    WindowAdapterId, WindowEpochExhausted,
};
use std::sync::Arc;

mod lifecycle;
mod load;
mod operations;
mod publication;

use load::{
    installed_identity, load_observed, load_prepared, map_live_failure, map_live_load_failure,
    map_publication_error, map_startup_failure, prepared_matches_active, start_error,
};

/// Explicit fixed bounds for one process-wide theme runtime.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ThemeRuntimeConfig {
    appearance: AppearanceCoordinatorConfig,
    max_manifest_bytes: NonZeroU64,
    manifest_read: ThemeManifestReadLimits,
    page: ThemePageLimits,
    watch_interval: Duration,
    watch_queue_capacity: NonZeroUsize,
    watch_max_entries_per_poll: NonZeroUsize,
    watch_max_file_bytes: NonZeroU64,
    max_hints_per_drain: NonZeroUsize,
}

impl ThemeRuntimeConfig {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        appearance: AppearanceCoordinatorConfig,
        max_manifest_bytes: NonZeroU64,
        manifest_read: ThemeManifestReadLimits,
        page: ThemePageLimits,
        watch_interval: Duration,
        watch_queue_capacity: NonZeroUsize,
        watch_max_entries_per_poll: NonZeroUsize,
        watch_max_file_bytes: NonZeroU64,
        max_hints_per_drain: NonZeroUsize,
    ) -> Self {
        Self {
            appearance,
            max_manifest_bytes,
            manifest_read,
            page,
            watch_interval,
            watch_queue_capacity,
            watch_max_entries_per_poll,
            watch_max_file_bytes,
            max_hints_per_drain,
        }
    }
}

/// Content-free terminal or retained runtime failure category.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ThemeRuntimeFailureClass {
    Identity,
    Repository,
    DocumentMissing,
    DocumentUnreadable,
    DocumentInvalid,
    Resolution,
    Subscription,
    Publication,
    Settings,
    Reconciliation,
    Retired,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ThemeRuntimeStartError {
    class: ThemeRuntimeFailureClass,
}

impl ThemeRuntimeStartError {
    #[must_use]
    pub const fn class(self) -> ThemeRuntimeFailureClass {
        self.class
    }
}

/// A confirmed Settings commit or reconciled exact-new result.
#[derive(Clone, Debug, PartialEq)]
pub struct ConfirmedSettingsTheme {
    draft: ThemeDraftIdentity,
    revision: ThemeDraftRevision,
    committed: ThemeSettingsIdentity,
    active: Option<InstalledThemeId>,
    prepared: PreparedThemeAppearance,
}

impl ConfirmedSettingsTheme {
    #[must_use]
    pub fn new(
        draft: ThemeDraftIdentity,
        revision: ThemeDraftRevision,
        committed: ThemeSettingsIdentity,
        active: Option<InstalledThemeId>,
        prepared: PreparedThemeAppearance,
    ) -> Self {
        Self {
            draft,
            revision,
            committed,
            active,
            prepared,
        }
    }
}

/// Exact Settings terminal result consumed by the runtime.
#[derive(Clone, Debug, PartialEq)]
pub enum SettingsThemeOutcome {
    NotCommitted,
    Indeterminate,
    ReconciledExactOld,
    ReconciliationCollision,
    Committed(ConfirmedSettingsTheme),
    ReconciledExactNew(ConfirmedSettingsTheme),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SettingsThemeResult {
    Retained,
    Published,
    HiddenBaseReplaced,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RepositoryAppearanceResult {
    Unchanged,
    Published,
    HiddenBaseReplaced,
    Retained(ThemeRuntimeFailureClass),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ThemeRepositoryRequestOrigin {
    Feature,
    DynamicTool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ThemeRepositoryRequest {
    origin: ThemeRepositoryRequestOrigin,
    command: ThemeRepositoryCommand,
}

impl ThemeRepositoryRequest {
    #[must_use]
    pub const fn new(
        origin: ThemeRepositoryRequestOrigin,
        command: ThemeRepositoryCommand,
    ) -> Self {
        Self { origin, command }
    }

    #[must_use]
    pub const fn origin(&self) -> ThemeRepositoryRequestOrigin {
        self.origin
    }
}

#[derive(Debug)]
pub enum ThemeRepositoryRequestResult {
    NotCommitted,
    Committed {
        publication: ThemeRepositoryCommit,
        appearance: RepositoryAppearanceResult,
    },
    Indeterminate {
        operation: NonZeroU64,
    },
}

#[derive(Debug)]
pub enum ThemeRepositoryReconciliationResult {
    ExactOld,
    ExactNew {
        publication: ThemeRepositoryCommit,
        appearance: RepositoryAppearanceResult,
    },
    Collision,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ThemeWatchDrainResult {
    received: usize,
    more_pending: bool,
    appearance: RepositoryAppearanceResult,
}

impl ThemeWatchDrainResult {
    #[must_use]
    pub const fn received(self) -> usize {
        self.received
    }
    #[must_use]
    pub const fn more_pending(self) -> bool {
        self.more_pending
    }
    #[must_use]
    pub const fn appearance(self) -> RepositoryAppearanceResult {
        self.appearance
    }
}

/// Fixed-size, content-free diagnostics for app and state theme ownership.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ThemeRuntimeDiagnostics {
    home_generation_present: bool,
    repository_generation_present: bool,
    repository_generation: Option<ThemeManifestGeneration>,
    active_document_revision: Option<ThemeDocumentRevision>,
    refresh_reread_needed: bool,
    retired: bool,
    worker_count: usize,
    pages_read: u64,
    watch_batches: u64,
    watch_hints: u64,
    app_coalesced_hints: u64,
    app_overflow_hints: u64,
    settings_outcomes: u64,
    repository_requests: u64,
    last_failure: Option<ThemeRuntimeFailureClass>,
    appearance: Option<AppearanceDiagnostics>,
    state: ThemeServiceDiagnostics,
}

impl ThemeRuntimeDiagnostics {
    #[must_use]
    pub const fn home_generation_present(self) -> bool {
        self.home_generation_present
    }
    #[must_use]
    pub const fn repository_generation_present(self) -> bool {
        self.repository_generation_present
    }
    #[must_use]
    pub const fn repository_generation(self) -> Option<ThemeManifestGeneration> {
        self.repository_generation
    }
    #[must_use]
    pub const fn active_document_revision(self) -> Option<ThemeDocumentRevision> {
        self.active_document_revision
    }
    #[must_use]
    pub const fn refresh_reread_needed(self) -> bool {
        self.refresh_reread_needed
    }
    #[must_use]
    pub const fn retired(self) -> bool {
        self.retired
    }
    #[must_use]
    pub const fn worker_count(self) -> usize {
        self.worker_count
    }
    #[must_use]
    pub const fn pages_read(self) -> u64 {
        self.pages_read
    }
    #[must_use]
    pub const fn watch_batches(self) -> u64 {
        self.watch_batches
    }
    #[must_use]
    pub const fn watch_hints(self) -> u64 {
        self.watch_hints
    }
    #[must_use]
    pub const fn app_coalesced_hints(self) -> u64 {
        self.app_coalesced_hints
    }
    #[must_use]
    pub const fn app_overflow_hints(self) -> u64 {
        self.app_overflow_hints
    }
    #[must_use]
    pub const fn settings_outcomes(self) -> u64 {
        self.settings_outcomes
    }
    #[must_use]
    pub const fn repository_requests(self) -> u64 {
        self.repository_requests
    }
    #[must_use]
    pub const fn last_failure(self) -> Option<ThemeRuntimeFailureClass> {
        self.last_failure
    }
    #[must_use]
    pub const fn appearance(self) -> Option<AppearanceDiagnostics> {
        self.appearance
    }
    #[must_use]
    pub const fn state(self) -> ThemeServiceDiagnostics {
        self.state
    }
}

/// One process-wide, generation-fenced theme service composition.
pub struct ThemeRuntime {
    config: ThemeRuntimeConfig,
    service: Option<ThemeService>,
    retired_state: ThemeServiceDiagnostics,
    repository: Option<ThemeRepositoryObservation>,
    subscription: Option<ThemeChangeSubscription>,
    appearance: Option<AppearanceCoordinator>,
    settings: ThemeSettingsIdentity,
    active: Option<InstalledThemeId>,
    last_observed_document: Option<ThemeDocumentIdentity>,
    refresh_reread_needed: bool,
    pages_read: u64,
    watch_batches: u64,
    watch_hints: u64,
    app_coalesced_hints: u64,
    app_overflow_hints: u64,
    settings_outcomes: u64,
    repository_requests: u64,
    last_failure: Option<ThemeRuntimeFailureClass>,
}

impl ThemeRuntime {
    pub fn drain_change_hints(
        &mut self,
        store: &HomeStore,
    ) -> Result<ThemeWatchDrainResult, ThemeRuntimeFailureClass> {
        self.ensure_running()?;
        let mut hints = Vec::with_capacity(self.config.max_hints_per_drain.get());
        while hints.len() < self.config.max_hints_per_drain.get() {
            let hint = match self
                .subscription
                .as_ref()
                .expect("running subscription")
                .try_recv()
            {
                Ok(Some(hint)) => hint,
                Ok(None) => break,
                Err(_) => {
                    self.retire_with(ThemeRuntimeFailureClass::Subscription);
                    return Err(ThemeRuntimeFailureClass::Subscription);
                }
            };
            hints.push(hint);
        }
        let received = hints.len();
        self.consume_change_hints_inner(
            store,
            &hints,
            received == self.config.max_hints_per_drain.get(),
        )
    }

    /// Consumes one caller-supplied state hint batch with the same bounded coalescing used by the
    /// owned subscription. Hints are advisory; every resulting reread revalidates exact state.
    pub fn consume_change_hints(
        &mut self,
        store: &HomeStore,
        hints: &[ThemeChangeHint],
    ) -> Result<ThemeWatchDrainResult, ThemeRuntimeFailureClass> {
        self.ensure_running()?;
        let received = hints.len().min(self.config.max_hints_per_drain.get());
        self.consume_change_hints_inner(
            store,
            &hints[..received],
            hints.len() > self.config.max_hints_per_drain.get(),
        )
    }

    fn consume_change_hints_inner(
        &mut self,
        store: &HomeStore,
        hints: &[ThemeChangeHint],
        more_pending: bool,
    ) -> Result<ThemeWatchDrainResult, ThemeRuntimeFailureClass> {
        let mut manifest = false;
        let mut active_document = false;
        let mut overflow = false;
        for hint in hints {
            self.watch_hints = self.watch_hints.saturating_add(1);
            match hint {
                ThemeChangeHint::Overflow => {
                    if overflow || manifest || active_document {
                        self.note_app_coalesced();
                    }
                    overflow = true;
                    self.app_overflow_hints = self.app_overflow_hints.saturating_add(1);
                }
                ThemeChangeHint::ManifestChanged => {
                    if manifest || overflow {
                        self.note_app_coalesced();
                    }
                    manifest = true;
                }
                ThemeChangeHint::DocumentChanged(id) if self.active.as_ref() == Some(id) => {
                    if active_document || manifest || overflow {
                        self.note_app_coalesced();
                    }
                    active_document = true;
                }
                ThemeChangeHint::DocumentChanged(_) => {}
            }
        }
        self.watch_batches = self.watch_batches.saturating_add(1);
        let appearance = if overflow || manifest {
            self.refresh_repository(store)
        } else if active_document {
            self.reload_active_document(store)
        } else {
            RepositoryAppearanceResult::Unchanged
        };
        Ok(ThemeWatchDrainResult {
            received: hints.len(),
            more_pending,
            appearance,
        })
    }

    fn note_app_coalesced(&mut self) {
        self.app_coalesced_hints = self.app_coalesced_hints.saturating_add(1);
    }

    fn ensure_running(&self) -> Result<(), ThemeRuntimeFailureClass> {
        if self.appearance.is_none() || self.service.is_none() {
            Err(ThemeRuntimeFailureClass::Retired)
        } else {
            Ok(())
        }
    }

    fn retire_with(&mut self, failure: ThemeRuntimeFailureClass) {
        if let Some(subscription) = self.subscription.take() {
            subscription.shutdown();
        }
        if let Some(appearance) = self.appearance.take() {
            appearance.retire();
        }
        self.repository = None;
        self.last_observed_document = None;
        self.retired_state = self
            .service
            .take()
            .map_or(self.retired_state, |service| service.diagnostics());
        self.last_failure = Some(failure);
    }
}

impl Drop for ThemeRuntime {
    fn drop(&mut self) {
        if let Some(subscription) = self.subscription.take() {
            subscription.shutdown();
        }
    }
}
