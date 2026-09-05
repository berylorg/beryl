use std::sync::Arc;

use beryl_home_store::{
    CommandCancellation, CommitReceipt, HomeGeneration, HomeStore, ReconciliationHandle,
};
use beryl_state::WindowClaimSelection;
use syndic_storage::{
    DraftEditorCandidateSessionV1, DraftPieceOperationIdV1,
    PreparedDraftEditorCandidateSessionAbandonFreshV1, PreparedDraftEditorCandidateSessionOpenV1,
    SyndicStorage,
};

use super::{
    MainWindowComposerMarkerMetadataAuthority, MainWindowConversationComposerPreparedSelection,
    MainWindowConversationComposerService, MainWindowShellUnpublished,
};
use crate::composer_host::{ComposerHostActivationRequest, SyndicComposerHost};
use crate::window_acquisition::{
    RuntimeBackedWindowAcquisition, RuntimeBackedWindowAcquisitionService,
    RuntimeBackedWindowMainWindowReservation,
};

mod activation;
mod retirement;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MainWindowInitialComposerProgress {
    Retry,
    Pending,
    Activated,
}

pub struct MainWindowInitialComposerAdmissionFailure {
    pub acquisition: RuntimeBackedWindowAcquisition,
    pub reservation: RuntimeBackedWindowMainWindowReservation,
    pub error: String,
}

pub struct MainWindowInitialComposerFailure {
    pub custody: MainWindowInitialComposer,
    pub error: String,
}

pub enum MainWindowInitialComposerRetirement {
    Retired(MainWindowShellUnpublished),
    Pending(MainWindowInitialComposerFailure),
}

pub struct MainWindowInitialComposerPrepared {
    prepared: MainWindowConversationComposerPreparedSelection,
    custody: MainWindowInitialComposer,
}

impl MainWindowInitialComposerPrepared {
    pub fn into_shell(
        self,
        composer_configurator: super::MainWindowShellComposerConfigurator,
        marker_seals: crate::composer_marker_seal::DraftMarkerSealService,
        submission_request_source: super::MainWindowComposerSubmissionRequestSource,
        appearance: Arc<crate::theme_runtime::AppearanceGeneration>,
    ) -> Result<super::MainWindowShellPrepared, MainWindowInitialComposerFailure> {
        let Self { prepared, custody } = self;
        let validation = (|| {
            custody.validate_source()?;
            custody
                .candidate
                .acquisition_service
                .validate_shell_selection(&custody.acquisition, prepared.selection_identity())?;
            let binding = prepared.selection_identity().binding();
            let home = appearance.prepared().home();
            if home.home_id() != binding.home_id()
                || home.home_generation() != binding.home_generation()
            {
                return Err(
                    "initial shell appearance belongs to a different home generation".to_owned(),
                );
            }
            Ok(())
        })();
        if let Err(error) = validation {
            return Err(MainWindowInitialComposerFailure { custody, error });
        }
        let MainWindowInitialComposer {
            acquisition,
            reservation,
            candidate,
        } = custody;
        Ok(super::MainWindowShellPrepared::from_initial_composer(
            acquisition,
            reservation,
            prepared,
            composer_configurator,
            marker_seals,
            submission_request_source,
            appearance,
            Box::new(candidate),
        ))
    }

    pub fn into_parts(
        self,
    ) -> (
        MainWindowConversationComposerPreparedSelection,
        MainWindowInitialComposer,
    ) {
        (self.prepared, self.custody)
    }

    pub fn retire(self, cancellation: CommandCancellation) -> MainWindowInitialComposerRetirement {
        drop(self.prepared);
        self.custody.retire(cancellation)
    }
}

pub struct MainWindowInitialComposer {
    acquisition: RuntimeBackedWindowAcquisition,
    reservation: RuntimeBackedWindowMainWindowReservation,
    candidate: InitialComposerCandidate,
}

pub(in crate::main_window) struct InitialComposerCandidate {
    acquisition_service: RuntimeBackedWindowAcquisitionService,
    store: Arc<HomeStore>,
    storage: SyndicStorage,
    home_generation: HomeGeneration,
    claim: WindowClaimSelection,
    request: ComposerHostActivationRequest,
    retirement_operation: DraftPieceOperationIdV1,
    marker_authority: Option<MainWindowComposerMarkerMetadataAuthority>,
    host: Option<SyndicComposerHost>,
    service: Option<Arc<MainWindowConversationComposerService>>,
    open: Option<PreparedDraftEditorCandidateSessionOpenV1>,
    open_reconciliation: Option<ReconciliationHandle>,
    open_receipt: Option<CommitReceipt>,
    opened: Option<DraftEditorCandidateSessionV1>,
    open_terminal: bool,
    activated: bool,
    retirement_started: bool,
    preparation_started: bool,
    abandonment: Option<PreparedDraftEditorCandidateSessionAbandonFreshV1>,
    abandonment_reconciliation: Option<ReconciliationHandle>,
    abandonment_receipt: Option<CommitReceipt>,
    #[cfg(feature = "test-faults")]
    before_open: Option<Box<dyn FnOnce(&HomeStore, SyndicStorage) + Send>>,
    #[cfg(feature = "test-faults")]
    before_retirement: Option<Box<dyn FnOnce(&HomeStore, SyndicStorage) + Send>>,
    #[cfg(feature = "test-faults")]
    before_open_classification: Option<Box<dyn FnOnce(&HomeStore, SyndicStorage) + Send>>,
}

impl MainWindowInitialComposer {
    pub fn new(
        acquisition: RuntimeBackedWindowAcquisition,
        reservation: RuntimeBackedWindowMainWindowReservation,
        acquisition_service: RuntimeBackedWindowAcquisitionService,
        store: Arc<HomeStore>,
        storage: SyndicStorage,
        claim: WindowClaimSelection,
        request: ComposerHostActivationRequest,
        retirement_operation: DraftPieceOperationIdV1,
        marker_authority: MainWindowComposerMarkerMetadataAuthority,
    ) -> Result<Self, MainWindowInitialComposerAdmissionFailure> {
        let validation = (|| {
            if reservation.window_id() != acquisition.window_id()
                || request.thread_id() != acquisition.thread_id()
            {
                return Err(
                    "initial composer reservation or activation target differs from acquisition"
                        .to_owned(),
                );
            }
            acquisition_service.validate_initial_composer_claim(&acquisition, claim, &store)?;
            SyndicComposerHost::validate_initial_request(&request)
                .map_err(|error| error.to_string())?;
            storage
                .revision(&store)
                .map_err(|error| error.to_string())?;
            store
                .health()
                .generation()
                .ok_or_else(|| "initial composer home is unavailable".to_owned())
        })();
        let home_generation = match validation {
            Ok(generation) => generation,
            Err(error) => {
                return Err(MainWindowInitialComposerAdmissionFailure {
                    acquisition,
                    reservation,
                    error,
                });
            }
        };
        Ok(Self {
            acquisition,
            reservation,
            candidate: InitialComposerCandidate {
                acquisition_service,
                store,
                host: Some(SyndicComposerHost::new(storage.clone())),
                storage,
                home_generation,
                claim,
                request,
                retirement_operation,
                marker_authority: Some(marker_authority),
                service: None,
                open: None,
                open_reconciliation: None,
                open_receipt: None,
                opened: None,
                open_terminal: false,
                activated: false,
                retirement_started: false,
                preparation_started: false,
                abandonment: None,
                abandonment_reconciliation: None,
                abandonment_receipt: None,
                #[cfg(feature = "test-faults")]
                before_open: None,
                #[cfg(feature = "test-faults")]
                before_retirement: None,
                #[cfg(feature = "test-faults")]
                before_open_classification: None,
            },
        })
    }

    pub fn acquisition(&self) -> &RuntimeBackedWindowAcquisition {
        &self.acquisition
    }

    fn validate_source(&self) -> Result<(), String> {
        if self.candidate.store.health().generation() != Some(self.candidate.home_generation) {
            return Err("initial composer home generation changed".to_owned());
        }
        self.candidate
            .acquisition_service
            .validate_initial_composer_claim(
                &self.acquisition,
                self.candidate.claim,
                &self.candidate.store,
            )
    }

    #[cfg(feature = "test-faults")]
    pub fn test_arm_before_open(
        &mut self,
        fault: impl FnOnce(&HomeStore, SyndicStorage) + Send + 'static,
    ) {
        self.candidate.before_open = Some(Box::new(fault));
    }

    #[cfg(feature = "test-faults")]
    pub fn test_arm_after_open(
        &mut self,
        fault: impl FnOnce(&HomeStore, SyndicStorage) + Send + 'static,
    ) {
        self.candidate
            .host
            .as_mut()
            .unwrap()
            .test_arm_activation_after_open_fault(fault);
    }

    #[cfg(feature = "test-faults")]
    pub fn test_arm_before_retirement(
        &mut self,
        fault: impl FnOnce(&HomeStore, SyndicStorage) + Send + 'static,
    ) {
        self.candidate.before_retirement = Some(Box::new(fault));
    }

    #[cfg(feature = "test-faults")]
    pub fn test_arm_before_open_classification(
        &mut self,
        fault: impl FnOnce(&HomeStore, SyndicStorage) + Send + 'static,
    ) {
        self.candidate.before_open_classification = Some(Box::new(fault));
    }
}
