use std::{num::NonZeroUsize, sync::Arc};

use beryl_home_store::{CommandCancellation, HomeCandidateError, HomeOpenPublication, HomeStore};
use beryl_model::BerylHomeId;
use beryl_state::BerylState;
use syndic_storage::{SyndicStorage, SyndicTimestamp};

use crate::{
    activity_service::{ActivityPreparationError, ActivityService, ActivityServiceLimits},
    cas_projection::{
        CasPreparationError, ProjectionConnectionService, ProjectionServiceConfig,
        RuntimeInterestConfig, RuntimeSessionPreparationConfig, RuntimeTokenDirectory,
        ScheduledExecutionSessions, ScheduledOrdinaryRequestPolicy,
    },
    composer_marker_seal::{
        DraftMarkerSealService, DraftMarkerSealServiceLimits,
        initial_preparation::MarkerPreparationError,
    },
    discussion_handoff_limits::HandoffScanLimits,
    discussion_settlement::{
        DiscussionSettlementOperations, HandoffCandidateConvergenceError,
        coordinator::HandoffCoordinator,
    },
    lifecycle_attention::ProcessLifecycleAttentionPool,
    process_admission::{ProcessAdmissionError, ProcessAdmissionFence, ProcessAdmissionGate},
    runtime_activity_enrollment::{
        ActivityEnrollmentCustodyError, RuntimeActivityEnrollmentOperations,
    },
    theme_runtime::{PreparedThemeRuntime, ThemeRuntimeConfig, ThemeRuntimeStartError},
};

mod attempt;
mod failed_retirement;
mod initial_disposal;
mod preparation;
mod published;
mod running_threads;
pub(crate) mod runtime_setup;
pub(crate) use running_threads::{
    PublishedRunningThreadsReader, PublishedSameWindowThreadCurrent,
    PublishedSameWindowThreadOperation, PublishedSameWindowThreadPreparation,
    PublishedSameWindowThreadPreparationFailure,
};
pub(crate) use runtime_setup::{PublishedRuntimeSetupObservation, PublishedRuntimeSetupServices};
pub(crate) mod recovery_composer;
mod recovery_failed_residents;
pub(crate) mod recovery_graph;
pub(crate) mod recovery_preparation;
mod recovery_retirement;
pub(crate) use recovery_retirement::{RetiredHomeRecoveryError, RetiredProcessWorkError};
pub(crate) mod recovery_threadless;
mod shutdown;
mod window_services;
use attempt::InitialServiceAttemptState;
pub(crate) use shutdown::{
    AppServiceCloseError, AppServiceFinalizationError, AppServiceShutdownProgress,
    PreparedShutdownObservation,
};
#[cfg(feature = "test-faults")]
pub(crate) use window_services::request_published_exact_soft_stop_for_test;
pub(crate) use window_services::{
    CloseConfirmationPreparationError, MainWindowServiceInputs, PublishedExactStopWorker,
    PublishedMainWindowServices,
};

#[derive(Clone)]
pub(crate) struct AppServiceConfiguration {
    pub(crate) paste_resources: crate::main_window::MainWindowComposerPasteResources,
    pub(crate) projection: ProjectionServiceConfig,
    pub(crate) runtime_interest: RuntimeInterestConfig,
    pub(crate) session_policy: ScheduledOrdinaryRequestPolicy,
    pub(crate) token_directory: RuntimeTokenDirectory,
    pub(crate) wsl_supervisor_artifact: Option<Arc<beryl_backend::WslSupervisorArtifact>>,
    pub(crate) handoff: HandoffScanLimits,
    pub(crate) marker: DraftMarkerSealServiceLimits,
    pub(crate) activity: ActivityServiceLimits,
    pub(crate) theme: ThemeRuntimeConfig,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum AppServiceOpenError {
    #[error("runtime admission preparation failed: {0}")]
    RuntimeSetup(String),
    #[error("a service graph is already installed")]
    AlreadyInstalled,
    #[error("the candidate belongs to another process home")]
    ForeignHome,
    #[error("service graph preparation was cancelled")]
    Cancelled,
    #[error("prepared runtime Activity authority is unavailable")]
    RuntimeUnavailable,
    #[error("prepared recovery handoff authority is unavailable")]
    HandoffUnavailable,
    #[error("prepared worker startup was cancelled")]
    StartupCancelled,
    #[error("initial service reopening failed: {0}")]
    Reopening(ProcessAdmissionError),
    #[error("initial service reopening retains unsettled custody: {0}")]
    UnsettledCustody(AppServiceCloseError),
    #[error(transparent)]
    Candidate(#[from] HomeCandidateError),
    #[error(transparent)]
    Cas(#[from] CasPreparationError),
    #[error(transparent)]
    Enrollment(#[from] ActivityEnrollmentCustodyError),
    #[error(transparent)]
    Settlement(#[from] HandoffCandidateConvergenceError),
    #[error(transparent)]
    Marker(#[from] MarkerPreparationError),
    #[error(transparent)]
    Activity(#[from] ActivityPreparationError),
    #[error("theme preparation failed: {0:?}")]
    Theme(ThemeRuntimeStartError),
}

pub(crate) struct AppServiceOpenFailure {
    pub(crate) error: AppServiceOpenError,
    pub(crate) rejected_candidate: Option<HomeOpenPublication>,
}

impl std::fmt::Debug for AppServiceOpenFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AppServiceOpenFailure")
            .field("error", &self.error)
            .field("rejected_candidate", &self.rejected_candidate.is_some())
            .finish()
    }
}

impl std::fmt::Display for AppServiceOpenFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.error, formatter)
    }
}

impl std::error::Error for AppServiceOpenFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}

impl From<ThemeRuntimeStartError> for AppServiceOpenError {
    fn from(error: ThemeRuntimeStartError) -> Self {
        Self::Theme(error)
    }
}

pub(crate) struct ProcessServiceOwner {
    runtime_setup_inputs: std::sync::Mutex<Option<MainWindowServiceInputs>>,
    first_configurator:
        std::sync::Mutex<Option<crate::main_window::MainWindowCreationConfiguratorSource>>,
    first_cleanups:
        std::sync::Mutex<Vec<crate::main_window::MainWindowInitialComposerRecoveryCleanup>>,
    failed_residents: Vec<recovery_failed_residents::FailedResidentSource>,
    failed_markers: Option<crate::composer_marker_seal::DraftMarkerSealRetainedFlights>,
    graph: Option<PublishedAppServices>,
    failed_close: Option<beryl_home_store::HomeCloseError>,
    failed_retirement: Option<crate::cas_projection::CasRetirementDisposalFailure>,
    failed_cas_close: Option<crate::cas_projection::ProjectionConnectionServiceCloseFailure>,
    closing_graph: Option<PublishedAppServices>,
    closing_handoff_error:
        Option<crate::discussion_settlement::coordinator::HandoffCoordinatorError>,
    recovery_retirement: Option<recovery_retirement::ServiceGraphRetirement>,
    attempt: InitialServiceAttemptState,
    home_id: BerylHomeId,
    process: ProcessAdmissionGate,
    windows: crate::window_acquisition::RuntimeBackedWindowProcessRegistry,
    enrollments: RuntimeActivityEnrollmentOperations,
    settlements: DiscussionSettlementOperations,
    #[cfg(feature = "test-faults")]
    before_initial_publication:
        Option<Box<dyn FnOnce(&mut HomeOpenPublication, &BerylState, &SyndicStorage) + Send>>,
    #[cfg(feature = "test-faults")]
    cancel_initial_worker_release: bool,
    #[cfg(feature = "test-faults")]
    fail_shutdown_completion: bool,
}

pub(crate) struct PublishedAppServices {
    runtime_setup: Arc<runtime_setup::RuntimeSetupService>,
    private_clipboard: crate::main_window::MainWindowPrivateClipboardOwner,
    restore_lifetime: Option<Arc<()>>,
    process: ProcessAdmissionGate,
    shutdown: Option<crate::cas_projection::ShutdownAttemptId>,
    shutdown_ready: bool,
    handoff: Option<HandoffCoordinator>,
    activity: Option<ActivityService>,
    marker: Option<DraftMarkerSealService>,
    theme: Option<PreparedThemeRuntime>,
    loaded_theme: Option<crate::theme_runtime::ThemeRuntime>,
    cas: Option<ProjectionConnectionService>,
    sessions: ScheduledExecutionSessions,
    attention: Arc<ProcessLifecycleAttentionPool>,
    state: BerylState,
    syndic: SyndicStorage,
    home: Option<HomeStore>,
}

impl ProcessServiceOwner {
    pub(crate) fn new(
        home_id: BerylHomeId,
        enrollment_slots: NonZeroUsize,
        settlement_slots: NonZeroUsize,
    ) -> Self {
        let process = ProcessAdmissionGate::new();
        Self {
            runtime_setup_inputs: std::sync::Mutex::new(None),
            first_configurator: std::sync::Mutex::new(None),
            first_cleanups: std::sync::Mutex::new(Vec::new()),
            failed_residents: Vec::new(),
            failed_markers: None,
            graph: None,
            failed_close: None,
            failed_retirement: None,
            failed_cas_close: None,
            closing_graph: None,
            closing_handoff_error: None,
            recovery_retirement: None,
            attempt: InitialServiceAttemptState::Initial,
            home_id,
            enrollments: RuntimeActivityEnrollmentOperations::new(home_id, enrollment_slots),
            settlements: DiscussionSettlementOperations::new(process.clone(), settlement_slots),
            windows: crate::window_acquisition::RuntimeBackedWindowProcessRegistry::new(
                process.clone(),
            ),
            process,
            #[cfg(feature = "test-faults")]
            before_initial_publication: None,
            #[cfg(feature = "test-faults")]
            cancel_initial_worker_release: false,
            #[cfg(feature = "test-faults")]
            fail_shutdown_completion: false,
        }
    }

    pub(crate) fn graph(&self) -> Option<&PublishedAppServices> {
        self.graph.as_ref()
    }

    pub(crate) fn open_initial(
        &mut self,
        candidate: HomeOpenPublication,
        state: BerylState,
        syndic: SyndicStorage,
        configuration: AppServiceConfiguration,
        at: SyndicTimestamp,
        cancellation: CommandCancellation,
    ) -> Result<(), AppServiceOpenFailure> {
        if let Err(error) = self.admit_initial_attempt(&candidate) {
            return Err(AppServiceOpenFailure {
                error,
                rejected_candidate: Some(candidate),
            });
        }
        let prepared = match preparation::PreparedAppServices::prepare(
            self,
            candidate,
            state,
            syndic,
            configuration,
            at,
            &cancellation,
        ) {
            Ok(prepared) => prepared,
            Err(failure) => return Err(self.dispose_initial_failure(failure)),
        };
        #[cfg(feature = "test-faults")]
        let prepared = {
            let mut prepared = prepared;
            if let Some(hook) = self.before_initial_publication.take() {
                hook(
                    prepared.candidate.as_mut().expect("prepared candidate"),
                    &prepared.state,
                    &prepared.syndic,
                );
            }
            prepared
        };
        let (graph, start) = match prepared.publish(&cancellation) {
            Ok(published) => published,
            Err(failure) => return Err(self.dispose_initial_failure(failure)),
        };
        self.graph = Some(graph);
        #[cfg(feature = "test-faults")]
        if std::mem::take(&mut self.cancel_initial_worker_release) {
            start.gate().cancel();
        }
        if !start.release() {
            let graph = self
                .graph
                .take()
                .expect("unstarted graph retains disposal custody");
            self.retain_initial_close(graph.dispose_unstarted());
            return Err(AppServiceOpenFailure {
                error: AppServiceOpenError::StartupCancelled,
                rejected_candidate: None,
            });
        }
        self.attempt = InitialServiceAttemptState::Published(None);
        Ok(())
    }
}

impl Drop for PublishedAppServices {
    fn drop(&mut self) {
        self.join_components();
        drop(self.home.take());
    }
}

impl PublishedAppServices {
    fn join_components(&mut self) {
        self.private_clipboard.retire();
        drop(self.restore_lifetime.take());
        let _ = self.process.fence();
        drop(self.handoff.take());
        if let Some(activity) = self.activity.take() {
            activity.retire();
        }
        if let Some(marker) = self.marker.take() {
            marker.retire_home_generation();
        }
        drop(self.theme.take());
        if let Some(mut theme) = self.loaded_theme.take() {
            theme.retire();
        }
        drop(self.cas.take());
        self.attention.close();
    }
}

#[cfg(all(test, feature = "test-faults"))]
mod tests {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/app_services.rs"
    ));
}

fn check_cancellation(cancellation: &CommandCancellation) -> Result<(), AppServiceOpenError> {
    if cancellation.is_cancelled() {
        Err(AppServiceOpenError::Cancelled)
    } else {
        Ok(())
    }
}
