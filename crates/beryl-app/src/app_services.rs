use std::{num::NonZeroUsize, sync::Arc};

use beryl_home_store::{CommandCancellation, HomeCandidateError, HomeOpenPublication, HomeStore};
use beryl_model::BerylHomeId;
use beryl_state::BerylState;
use syndic_storage::{SyndicStorage, SyndicTimestamp};

use crate::{
    activity_service::{ActivityPreparationError, ActivityService, ActivityServiceLimits},
    cas_projection::{
        CasPreparationError, ProjectionConnectionService, ProjectionServiceConfig,
        RuntimeInterestConfig, RuntimeSessionPreparationConfig, RuntimeTokenDirectories,
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
    process_admission::ProcessAdmissionGate,
    runtime_activity_enrollment::{
        ActivityEnrollmentCustodyError, RuntimeActivityEnrollmentOperations,
    },
    theme_runtime::{PreparedThemeRuntime, ThemeRuntimeConfig, ThemeRuntimeStartError},
};

mod preparation;
mod published;
mod shutdown;
pub(crate) use shutdown::{AppServiceCloseError, AppServiceShutdownProgress};

pub(crate) struct AppServiceConfiguration {
    pub(crate) projection: ProjectionServiceConfig,
    pub(crate) runtime_interest: RuntimeInterestConfig,
    pub(crate) session_policy: ScheduledOrdinaryRequestPolicy,
    pub(crate) token_directories: Vec<RuntimeTokenDirectories>,
    pub(crate) handoff: HandoffScanLimits,
    pub(crate) marker: DraftMarkerSealServiceLimits,
    pub(crate) activity: ActivityServiceLimits,
    pub(crate) theme: ThemeRuntimeConfig,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum AppServiceOpenError {
    #[error("a service graph is already installed")]
    AlreadyInstalled,
    #[error("the candidate belongs to another process home")]
    ForeignHome,
    #[error("service graph preparation was cancelled")]
    Cancelled,
    #[error("prepared runtime Activity authority is unavailable")]
    RuntimeUnavailable,
    #[error("prepared worker startup was cancelled")]
    StartupCancelled,
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

impl From<ThemeRuntimeStartError> for AppServiceOpenError {
    fn from(error: ThemeRuntimeStartError) -> Self {
        Self::Theme(error)
    }
}

pub(crate) struct ProcessServiceOwner {
    graph: Option<PublishedAppServices>,
    failed_close: Option<beryl_home_store::HomeCloseError>,
    published_once: bool,
    home_id: BerylHomeId,
    process: ProcessAdmissionGate,
    enrollments: RuntimeActivityEnrollmentOperations,
    settlements: DiscussionSettlementOperations,
}

pub(crate) struct PublishedAppServices {
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
            graph: None,
            failed_close: None,
            published_once: false,
            home_id,
            enrollments: RuntimeActivityEnrollmentOperations::new(home_id, enrollment_slots),
            settlements: DiscussionSettlementOperations::new(process.clone(), settlement_slots),
            process,
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
    ) -> Result<(), AppServiceOpenError> {
        if self.published_once || self.graph.is_some() || self.failed_close.is_some() {
            return Err(AppServiceOpenError::AlreadyInstalled);
        }
        if candidate.home_id() != self.home_id {
            return Err(AppServiceOpenError::ForeignHome);
        }
        let prepared = preparation::PreparedAppServices::prepare(
            self,
            candidate,
            state,
            syndic,
            configuration,
            at,
            &cancellation,
        )?;
        let (graph, start) = prepared.publish(&cancellation)?;
        self.graph = Some(graph);
        if !start.release() {
            drop(self.graph.take());
            return Err(AppServiceOpenError::StartupCancelled);
        }
        self.published_once = true;
        Ok(())
    }
}

impl Drop for PublishedAppServices {
    fn drop(&mut self) {
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
        drop(self.home.take());
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
