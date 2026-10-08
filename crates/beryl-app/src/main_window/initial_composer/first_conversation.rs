use super::*;
use crate::composer_host::{
    ComposerHostInitialDemand, ComposerHostRequestId, ComposerHostRequestPurpose,
};
use crate::main_window::{
    MainWindowComposerSelectionIdentity, MainWindowConversationComposerConfig,
};
use beryl_home_store::HomeHealthState;
use beryl_model::{SyndicDraftId, SyndicThreadId};
use beryl_state::{BerylState, SessionWindowRecord, ThreadClaimState};
use std::num::NonZeroU64;
use syndic_storage::{
    DraftEditorCandidateSessionIdV1, DraftPieceMarkerDemandV1, DraftPieceMarkerDirectionV1,
    DraftPieceMarkerScopeV1, DraftPieceTextDemandV1, SyndicPointReadLimit,
};

pub struct MainWindowFirstConversationPreparation {
    candidate: InitialComposerCandidate,
    state: BerylState,
    window: SessionWindowRecord,
    draft: SyndicDraftId,
}

impl MainWindowFirstConversationPreparation {
    #[cfg(feature = "test-faults")]
    pub fn test_arm_before_open_classification(
        &mut self,
        fault: impl FnOnce(&HomeStore, SyndicStorage) + Send + 'static,
    ) {
        self.candidate.before_open_classification = Some(Box::new(fault));
    }

    pub fn new(
        store: Arc<HomeServiceReference>,
        state: &BerylState,
        storage: SyndicStorage,
        window: SessionWindowRecord,
        thread: SyndicThreadId,
        draft: SyndicDraftId,
        request: ComposerHostActivationRequest,
        disposal: DraftPieceOperationIdV1,
    ) -> Result<Self, String> {
        if request.thread_id() != thread
            || request.restoration().is_some()
            || window
                .selected_thread()
                .is_none_or(|claim| claim.thread_id() != thread)
        {
            return Err("first conversation editor differs from committed admission".into());
        }
        SyndicComposerHost::validate_initial_request(&request).map_err(|e| e.to_string())?;
        let generation = store
            .health()
            .generation()
            .ok_or("first conversation home is unavailable")?;
        let owner = Self {
            candidate: InitialComposerCandidate::new(
                store,
                storage,
                generation,
                window.selected_thread().unwrap(),
                request,
                disposal,
                MainWindowComposerMarkerMetadataAuthority::new(state.assets()),
            ),
            state: state.clone(),
            window,
            draft,
        };
        owner.validate_source()?;
        Ok(owner)
    }

    pub(crate) fn new_fresh(
        store: Arc<HomeServiceReference>,
        state: &BerylState,
        storage: SyndicStorage,
        window: SessionWindowRecord,
        thread: SyndicThreadId,
        draft: SyndicDraftId,
    ) -> Result<Self, String> {
        let (request, disposal) = fresh_request(thread)?;
        Self::new(
            store, state, storage, window, thread, draft, request, disposal,
        )
    }

    pub fn window(&self) -> &SessionWindowRecord {
        &self.window
    }

    fn validate_source(&self) -> Result<(), String> {
        let store = &self.candidate.store;
        if store.health().state() != HomeHealthState::Healthy
            || store.health().generation() != Some(self.candidate.home_generation)
        {
            return Err("first conversation home generation changed".into());
        }
        let revision = store.home_revision().map_err(|e| e.to_string())?;
        let bootstrap = self
            .state
            .session()
            .minimal_bootstrap(store)
            .map_err(|e| e.to_string())?
            .ok_or("first conversation session missing")?;
        let claim = self.window.selected_thread().unwrap();
        let paired = self
            .state
            .session()
            .window_claim_catalog_source(store, self.window.window_id())
            .map_err(|e| e.to_string())?;
        let current = self
            .candidate
            .storage
            .current_draft(
                store,
                claim.thread_id(),
                SyndicPointReadLimit::new(65_536).unwrap(),
            )
            .map_err(|e| e.to_string())?
            .ok_or("first conversation draft missing")?;
        if !bootstrap
            .windows()
            .iter()
            .any(|window| window == &self.window)
            || bootstrap.header().fallback() != self.window.remembered_target()
            || current.draft().id() != self.draft
            || !paired.claim().is_some_and(|actual| {
                actual.thread_id() == claim.thread_id()
                    && actual.generation() == claim.generation()
                    && actual.revision() == claim.revision()
                    && actual.state() == ThreadClaimState::Active
            })
            || store.home_revision().map_err(|e| e.to_string())? != revision
        {
            return Err("first conversation admission facts changed".into());
        }
        Ok(())
    }

    pub fn advance(
        &mut self,
        cancellation: &CommandCancellation,
    ) -> Result<MainWindowInitialComposerProgress, String> {
        self.validate_source()?;
        let progress = self.candidate.advance(
            cancellation,
            self.candidate.request.thread_id(),
            self.draft,
            &|| Ok(()),
        )?;
        self.validate_source()?;
        Ok(progress)
    }

    pub fn prepare(
        &mut self,
        configurator: &mut impl FnMut(
            MainWindowComposerSelectionIdentity,
        ) -> Result<MainWindowConversationComposerConfig, String>,
    ) -> Result<MainWindowConversationComposerPreparedSelection, String> {
        self.validate_source()?;
        let prepared =
            self.candidate
                .prepare_selection(self.window.window_id(), configurator, &|| Ok(()))?;
        self.validate_source()?;
        Ok(prepared)
    }

    pub fn capture_failed_recovery(
        mut self,
    ) -> Result<MainWindowInitialComposerRecoveryCleanup, (Self, String)> {
        if self.candidate.store.health().state() != HomeHealthState::Failed {
            return Err((
                self,
                "first conversation capture requires failed home".into(),
            ));
        }
        if let Err(error) = super::fresh_candidate::retire_runtime(&mut self.candidate) {
            return Err((self, error));
        }
        Ok(self.candidate.into_recovery_cleanup(false))
    }
}

pub(super) fn fresh_request(
    thread: SyndicThreadId,
) -> Result<(ComposerHostActivationRequest, DraftPieceOperationIdV1), String> {
    let mut identities = [0_u8; 48];
    getrandom::fill(&mut identities)
        .map_err(|_| "first conversation editor identity allocation failed")?;
    let session = DraftEditorCandidateSessionIdV1::from_bytes(identities[..16].try_into().unwrap());
    let opening = DraftPieceOperationIdV1::from_bytes(identities[16..32].try_into().unwrap());
    let disposal = DraftPieceOperationIdV1::from_bytes(identities[32..].try_into().unwrap());
    let demands = vec![
        ComposerHostInitialDemand::Text {
            request_id: ComposerHostRequestId::new(NonZeroU64::new(1).unwrap()),
            purpose: ComposerHostRequestPurpose::Geometry,
            demand: DraftPieceTextDemandV1::Forward(0),
            max_bytes: 4096,
        },
        ComposerHostInitialDemand::Markers {
            request_id: ComposerHostRequestId::new(NonZeroU64::new(2).unwrap()),
            purpose: ComposerHostRequestPurpose::Geometry,
            demand: DraftPieceMarkerDemandV1::new(
                DraftPieceMarkerScopeV1::InclusiveRange { start: 0, end: 0 },
                DraftPieceMarkerDirectionV1::Forward,
                None,
                48,
                65_536,
            ),
        },
    ]
    .into_boxed_slice();
    Ok((
        ComposerHostActivationRequest::new(
            thread,
            session,
            opening,
            NonZeroU64::new(1).unwrap(),
            None,
            demands,
        ),
        disposal,
    ))
}
