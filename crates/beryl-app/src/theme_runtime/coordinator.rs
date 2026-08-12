use std::{num::NonZeroUsize, sync::Arc};

use beryl_state::{PreparedThemeAppearance, ThemeHomeIdentity};

use super::{
    AdapterRegistrationError, AppearanceGeneration, AppearanceGenerationNumber,
    AppearancePublication, AppearanceWindowAdapter, DurablePublicationError,
    DurablePublicationIdentity, PreviewCandidateIdentity, PreviewPublicationError, PreviewSequence,
    PreviewSource, PreviewSourceKind, PublicationFailureClass, WindowAdapterId,
    WindowEpochExhausted, WindowSetEpoch,
};

/// Fixed process-local adapter capacity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AppearanceCoordinatorConfig {
    adapter_capacity: NonZeroUsize,
}

impl AppearanceCoordinatorConfig {
    #[must_use]
    pub const fn new(adapter_capacity: NonZeroUsize) -> Self {
        Self { adapter_capacity }
    }

    #[must_use]
    pub const fn adapter_capacity(self) -> NonZeroUsize {
        self.adapter_capacity
    }
}

/// Content-free current or pending preview diagnostic.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PreviewDiagnostic {
    pub(super) source: PreviewSourceKind,
    pub(super) sequence: PreviewSequence,
}

impl PreviewDiagnostic {
    #[must_use]
    pub const fn source(self) -> PreviewSourceKind {
        self.source
    }

    #[must_use]
    pub const fn sequence(self) -> PreviewSequence {
        self.sequence
    }
}

/// Fixed-size content-free coordinator diagnostics.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AppearanceDiagnostics {
    current_generation: AppearanceGenerationNumber,
    durable_generation: AppearanceGenerationNumber,
    window_epoch: WindowSetEpoch,
    adapter_count: usize,
    adapter_capacity: NonZeroUsize,
    current_preview: Option<PreviewDiagnostic>,
    pending_preview: Option<PreviewDiagnostic>,
    pending_durable_application: bool,
    stale_rejections: u64,
    last_failure: Option<PublicationFailureClass>,
}

impl AppearanceDiagnostics {
    #[must_use]
    pub const fn current_generation(self) -> AppearanceGenerationNumber {
        self.current_generation
    }

    #[must_use]
    pub const fn durable_generation(self) -> AppearanceGenerationNumber {
        self.durable_generation
    }

    #[must_use]
    pub const fn window_epoch(self) -> WindowSetEpoch {
        self.window_epoch
    }

    #[must_use]
    pub const fn adapter_count(self) -> usize {
        self.adapter_count
    }

    #[must_use]
    pub const fn adapter_capacity(self) -> NonZeroUsize {
        self.adapter_capacity
    }

    #[must_use]
    pub const fn current_preview(self) -> Option<PreviewDiagnostic> {
        self.current_preview
    }

    #[must_use]
    pub const fn pending_preview(self) -> Option<PreviewDiagnostic> {
        self.pending_preview
    }

    #[must_use]
    pub const fn pending_durable_application(self) -> bool {
        self.pending_durable_application
    }

    #[must_use]
    pub const fn stale_rejections(self) -> u64 {
        self.stale_rejections
    }

    #[must_use]
    pub const fn last_failure(self) -> Option<PublicationFailureClass> {
        self.last_failure
    }
}

/// Move-only exact fence for one durable preparation completion.
#[derive(Debug)]
pub struct DurablePublicationRequest {
    pub(super) attempt: u64,
    pub(super) home: ThemeHomeIdentity,
    pub(super) durable_generation: AppearanceGenerationNumber,
    pub(super) current_generation: AppearanceGenerationNumber,
    pub(super) window_epoch: WindowSetEpoch,
    pub(super) preview_sequence: Option<PreviewSequence>,
    pub(super) identity: DurablePublicationIdentity,
}

impl DurablePublicationRequest {
    #[must_use]
    pub const fn identity(&self) -> &DurablePublicationIdentity {
        &self.identity
    }
}

/// Move-only exact fence owned by one preview preparation.
#[derive(Debug)]
pub struct PreviewPublicationRequest {
    pub(super) home: ThemeHomeIdentity,
    pub(super) durable_generation: AppearanceGenerationNumber,
    pub(super) current_generation: AppearanceGenerationNumber,
    pub(super) window_epoch: WindowSetEpoch,
    pub(super) sequence: PreviewSequence,
    pub(super) source: PreviewSource,
    pub(super) candidate: PreviewCandidateIdentity,
}

/// Completed preview preparation bound to the exact candidate used as input.
#[derive(Clone, Debug, PartialEq)]
pub struct PreparedPreviewAppearance {
    pub(super) candidate: PreviewCandidateIdentity,
    pub(super) prepared: PreparedThemeAppearance,
}

impl PreparedPreviewAppearance {
    #[must_use]
    pub const fn new(
        candidate: PreviewCandidateIdentity,
        prepared: PreparedThemeAppearance,
    ) -> Self {
        Self {
            candidate,
            prepared,
        }
    }

    #[must_use]
    pub const fn candidate(&self) -> &PreviewCandidateIdentity {
        &self.candidate
    }

    #[must_use]
    pub const fn prepared(&self) -> &PreparedThemeAppearance {
        &self.prepared
    }
}

impl PreviewPublicationRequest {
    #[must_use]
    pub const fn sequence(&self) -> PreviewSequence {
        self.sequence
    }

    #[must_use]
    pub const fn source(&self) -> PreviewSource {
        self.source
    }

    #[must_use]
    pub const fn candidate(&self) -> &PreviewCandidateIdentity {
        &self.candidate
    }
}

#[derive(Clone, Debug)]
pub enum DurablePublicationOutcome {
    Published(Arc<AppearanceGeneration>),
    HiddenBaseReplaced(Arc<AppearanceGeneration>),
}

#[derive(Clone, Debug)]
pub enum DurableRetryOutcome {
    NotPending,
    Published(Arc<AppearanceGeneration>),
    HiddenBaseRetained(Arc<AppearanceGeneration>),
}

#[derive(Clone, Debug)]
pub struct PreviewPublicationResult(pub(super) Arc<AppearanceGeneration>);

impl PreviewPublicationResult {
    #[must_use]
    pub fn generation(&self) -> &Arc<AppearanceGeneration> {
        &self.0
    }
}

#[derive(Clone, Debug)]
pub enum StopPreviewResult {
    AlreadyStopped,
    Restored(Arc<AppearanceGeneration>),
}

/// Pure process-wide coordinator. Callers serialize mutable access.
pub struct AppearanceCoordinator {
    pub(super) config: AppearanceCoordinatorConfig,
    pub(super) home: ThemeHomeIdentity,
    pub(super) current: Arc<AppearanceGeneration>,
    pub(super) durable: Arc<AppearanceGeneration>,
    pub(super) last_generation: AppearanceGenerationNumber,
    pub(super) window_epoch: WindowSetEpoch,
    pub(super) adapters: Vec<Arc<dyn AppearanceWindowAdapter>>,
    pub(super) last_preview_sequence: Option<PreviewSequence>,
    pub(super) pending_preview: Option<PreviewDiagnostic>,
    pub(super) last_durable_attempt: u64,
    pub(super) latest_durable_attempt: Option<u64>,
    pub(super) pending_durable_application: bool,
    pub(super) pending_durable_ends_preview: bool,
    pub(super) stale_rejections: u64,
    pub(super) last_failure: Option<PublicationFailureClass>,
}

impl AppearanceCoordinator {
    #[must_use]
    pub fn new(config: AppearanceCoordinatorConfig, initial: PreparedThemeAppearance) -> Self {
        let home = initial.home();
        let number = AppearanceGenerationNumber::initial();
        let generation = Arc::new(AppearanceGeneration::new(
            number,
            initial,
            AppearancePublication::Durable,
        ));
        Self {
            config,
            home,
            current: Arc::clone(&generation),
            durable: generation,
            last_generation: number,
            window_epoch: WindowSetEpoch::initial(),
            adapters: Vec::with_capacity(config.adapter_capacity().get()),
            last_preview_sequence: None,
            pending_preview: None,
            last_durable_attempt: 0,
            latest_durable_attempt: None,
            pending_durable_application: false,
            pending_durable_ends_preview: false,
            stale_rejections: 0,
            last_failure: None,
        }
    }

    #[must_use]
    pub const fn home(&self) -> ThemeHomeIdentity {
        self.home
    }

    #[must_use]
    pub fn current(&self) -> Arc<AppearanceGeneration> {
        Arc::clone(&self.current)
    }

    #[must_use]
    pub fn durable_base(&self) -> Arc<AppearanceGeneration> {
        Arc::clone(&self.durable)
    }

    pub fn register_adapter(
        &mut self,
        adapter: Arc<dyn AppearanceWindowAdapter>,
    ) -> Result<(), AdapterRegistrationError> {
        if self.adapters.len() == self.config.adapter_capacity().get() {
            return Err(AdapterRegistrationError::CapacityReached);
        }
        let id = adapter.id();
        if self.adapters.iter().any(|existing| existing.id() == id) {
            return Err(AdapterRegistrationError::DuplicateIdentity(id));
        }
        let next_epoch = self
            .window_epoch
            .checked_next()
            .map_err(|_| AdapterRegistrationError::WindowEpochExhausted)?;
        let prepared = adapter
            .prepare(Arc::clone(&self.current))
            .map_err(|class| AdapterRegistrationError::Preparation { adapter: id, class })?;
        prepared.commit();
        self.adapters.push(adapter);
        self.window_epoch = next_epoch;
        Ok(())
    }

    pub fn unregister_adapter(
        &mut self,
        id: WindowAdapterId,
    ) -> Result<bool, WindowEpochExhausted> {
        let Some(index) = self.adapters.iter().position(|adapter| adapter.id() == id) else {
            return Ok(false);
        };
        let next_epoch = self.window_epoch.checked_next()?;
        self.adapters.remove(index);
        self.window_epoch = next_epoch;
        Ok(true)
    }

    #[must_use]
    pub fn diagnostics(&self) -> AppearanceDiagnostics {
        AppearanceDiagnostics {
            current_generation: self.current.number(),
            durable_generation: self.durable.number(),
            window_epoch: self.window_epoch,
            adapter_count: self.adapters.len(),
            adapter_capacity: self.config.adapter_capacity(),
            current_preview: self.current_preview_diagnostic(),
            pending_preview: self.pending_preview,
            pending_durable_application: self.pending_durable_application,
            stale_rejections: self.stale_rejections,
            last_failure: self.last_failure,
        }
    }

    pub fn retire(self) {}

    fn current_preview_diagnostic(&self) -> Option<PreviewDiagnostic> {
        let AppearancePublication::Preview {
            source, sequence, ..
        } = self.current.publication()
        else {
            return None;
        };
        Some(PreviewDiagnostic {
            source: source.kind(),
            sequence: *sequence,
        })
    }

    pub fn begin_durable_publication(
        &mut self,
        identity: DurablePublicationIdentity,
    ) -> Result<DurablePublicationRequest, DurablePublicationError> {
        self.begin_durable(identity)
    }

    pub fn begin_preview(
        &mut self,
        source: PreviewSource,
        candidate: PreviewCandidateIdentity,
    ) -> Result<PreviewPublicationRequest, PreviewPublicationError> {
        self.begin_preview_request(source, candidate)
    }
}
