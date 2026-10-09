use super::*;
use crate::composer_host::ComposerHostActivationOutcome;
use crate::main_window::{
    MainWindowComposerSelectionIdentity, MainWindowComposerSlot,
    MainWindowConversationComposerConfig, MainWindowConversationComposerMount,
};
use beryl_home_store::{
    CommandOutcome, HomeCandidateRecoveryAccess, HomeCommand, HomeRecoveryCandidate,
    ReconciliationResolution,
};
use beryl_model::{SyndicDraftId, SyndicThreadId};
use beryl_state::{BerylState, SessionWindowRecord, ThreadClaimState};
use syndic_storage::{
    DraftEditorCandidateSessionOpenOutcomeV1, DraftEditorCandidateSessionOpenRequestV1,
    DraftPieceTextDemandV1, SyndicCurrentDraft, SyndicPointReadLimit,
};

pub struct MainWindowFreshComposerPreparation {
    pub(super) candidate: InitialComposerCandidate,
    state: BerylState,
    window: SessionWindowRecord,
    current: SyndicCurrentDraft,
    kind: FreshComposerKind,
}
mod requests;

#[derive(Clone, Copy, Eq, PartialEq)]
enum FreshComposerKind {
    CreationEmpty,
    OrdinaryClaim,
}

impl MainWindowFreshComposerPreparation {
    pub fn new(
        candidate: &mut HomeRecoveryCandidate,
        state: &BerylState,
        storage: SyndicStorage,
        window: SessionWindowRecord,
        thread: SyndicThreadId,
        draft: SyndicDraftId,
        request: ComposerHostActivationRequest,
        disposal: DraftPieceOperationIdV1,
        marker: MainWindowComposerMarkerMetadataAuthority,
    ) -> Result<Self, String> {
        if request.thread_id() != thread || request.restoration().is_some() {
            return Err(
                "fresh recovery activation has a foreign target or restoration seed".into(),
            );
        }
        SyndicComposerHost::validate_initial_request(&request).map_err(|e| e.to_string())?;
        let store = Arc::new(candidate.service_reference());
        let access = candidate.recovery_access().map_err(|e| e.to_string())?;
        authenticate_window(&access, state, &window)?;
        let current = storage
            .current_draft_candidate(&access, thread, point_limit())
            .map_err(|e| e.to_string())?
            .ok_or("fresh recovery draft is missing")?;
        if current.draft().id() != draft
            || window
                .selected_thread()
                .is_none_or(|claim| claim.thread_id() != thread)
        {
            return Err("fresh recovery draft or thread differs from committed onboarding".into());
        }
        Ok(Self {
            candidate: InitialComposerCandidate::new(
                store,
                storage,
                access.generation(),
                window.selected_thread().unwrap(),
                request,
                disposal,
                marker,
            ),
            state: state.clone(),
            window,
            current,
            kind: FreshComposerKind::CreationEmpty,
        })
    }

    pub(crate) fn new_fresh(
        candidate: &mut HomeRecoveryCandidate,
        state: &BerylState,
        storage: SyndicStorage,
        window: SessionWindowRecord,
        thread: SyndicThreadId,
        draft: SyndicDraftId,
        marker: MainWindowComposerMarkerMetadataAuthority,
    ) -> Result<Self, String> {
        let (request, disposal) = super::first_conversation::fresh_request(thread)?;
        Self::new(
            candidate, state, storage, window, thread, draft, request, disposal, marker,
        )
    }

    pub(crate) fn new_ordinary_claim(
        candidate: &mut HomeRecoveryCandidate,
        state: &BerylState,
        storage: SyndicStorage,
        window: SessionWindowRecord,
        thread: SyndicThreadId,
        draft: SyndicDraftId,
        marker: MainWindowComposerMarkerMetadataAuthority,
    ) -> Result<Self, String> {
        let mut prepared =
            Self::new_fresh(candidate, state, storage, window, thread, draft, marker)?;
        prepared.kind = FreshComposerKind::OrdinaryClaim;
        Ok(prepared)
    }

    pub(in crate::main_window) fn is_ordinary_claim(&self) -> bool {
        self.kind == FreshComposerKind::OrdinaryClaim
    }

    pub fn window(&self) -> &SessionWindowRecord {
        &self.window
    }

    pub fn selection(&self) -> Option<MainWindowComposerSelectionIdentity> {
        self.candidate
            .service
            .as_ref()
            .and_then(|service| service.selected_identity())
    }

    pub fn service(&self) -> Option<Arc<MainWindowConversationComposerService>> {
        self.candidate.service.clone()
    }

    pub fn validate(&self, candidate: &mut HomeRecoveryCandidate) -> Result<(), String> {
        let access = candidate.recovery_access().map_err(|e| e.to_string())?;
        self.validate_access(&access)
    }

    pub(crate) fn revalidate_ready(
        &self,
        candidate: &mut HomeRecoveryCandidate,
    ) -> Result<(), String> {
        self.validate(candidate)?;
        if !self.candidate.activated
            || self.candidate.retirement_started
            || self.candidate.open_reconciliation.is_some()
            || self.candidate.service.is_none()
        {
            return Err("fresh recovery editor is not ready for graph publication".into());
        }
        Ok(())
    }

    fn validate_access(&self, access: &HomeCandidateRecoveryAccess<'_>) -> Result<(), String> {
        if access.home_id() != self.candidate.store.home_id()
            || access.canonical_path() != self.candidate.store.canonical_path()
            || access.generation() != self.candidate.home_generation
        {
            return Err("fresh recovery preparation belongs to another home candidate".into());
        }
        let revision = access.home_revision().map_err(|e| e.to_string())?;
        authenticate_window(access, &self.state, &self.window)?;
        let current = self
            .candidate
            .storage
            .current_draft_candidate(access, self.candidate.request.thread_id(), point_limit())
            .map_err(|e| e.to_string())?
            .ok_or("fresh recovery current draft disappeared")?;
        if current != self.current {
            return Err("fresh recovery committed draft facts changed".into());
        }
        if access.home_revision().map_err(|e| e.to_string())? != revision {
            return Err("fresh recovery selection changed during authentication".into());
        }
        Ok(())
    }

    pub fn advance(
        &mut self,
        candidate: &mut HomeRecoveryCandidate,
        cancellation: &CommandCancellation,
    ) -> Result<MainWindowInitialComposerProgress, String> {
        let access = candidate.recovery_access().map_err(|e| e.to_string())?;
        self.validate_access(&access)?;
        let owner = &mut self.candidate;
        if owner.retirement_started || owner.preparation_started || owner.open_terminal {
            return Err("fresh recovery editor has already transferred or retired".into());
        }
        if owner.activated {
            return Ok(MainWindowInitialComposerProgress::Activated);
        }
        if let Some(handle) = owner.open_reconciliation.as_ref() {
            match access.retry_reconciliation(handle) {
                Err(_) => return Ok(MainWindowInitialComposerProgress::Pending),
                Ok(ReconciliationResolution::ExactOld) => {
                    owner.open_reconciliation = None;
                    owner.open_receipt = None;
                }
                Ok(ReconciliationResolution::ExactNew { receipt }) => {
                    owner.open_reconciliation = None;
                    owner.open_receipt = Some(receipt);
                }
                Ok(
                    ReconciliationResolution::ExactSuccessor { .. }
                    | ReconciliationResolution::Collision,
                ) => {
                    owner.open_terminal = true;
                    return Err(
                        "fresh recovery opening reconciliation collision retains custody".into(),
                    );
                }
            }
        }
        if owner.open_receipt.is_none() {
            if cancellation.is_cancelled() {
                return Err("fresh recovery opening cancelled".into());
            }
            if owner.open.is_none() {
                let probe = owner
                    .storage
                    .current_draft_piece_text_demand_candidate(
                        &access,
                        owner.request.thread_id(),
                        DraftPieceTextDemandV1::Validate(0),
                        4,
                    )
                    .map_err(|e| e.to_string())?
                    .ok_or("fresh recovery selector is missing")?;
                owner.open = Some(
                    owner
                        .storage
                        .prepare_open_draft_editor_candidate_session_candidate(
                            &access,
                            DraftEditorCandidateSessionOpenRequestV1::new(
                                probe.selector(),
                                owner.request.session_id(),
                                owner.request.operation_id(),
                            ),
                        )
                        .map_err(|e| e.to_string())?,
                );
            }
            let mut command = HomeCommand::new(access.home_revision().map_err(|e| e.to_string())?)
                .with_cancellation(cancellation.clone());
            command
                .add(
                    owner.storage.open_draft_editor_candidate_session(
                        owner
                            .storage
                            .revision_candidate(&access)
                            .map_err(|e| e.to_string())?,
                        owner.open.as_ref().unwrap().clone(),
                    ),
                )
                .map_err(|e| e.to_string())?;
            match access.execute(command) {
                CommandOutcome::NotCommitted { evidence } => {
                    owner.open_noncommit = Some(evidence);
                    return Ok(MainWindowInitialComposerProgress::Retry);
                }
                CommandOutcome::Indeterminate {
                    reconciliation,
                    failure,
                } => {
                    owner.open_noncommit = None;
                    owner.open_indeterminate_failure = Some(failure);
                    owner.open_reconciliation = Some(reconciliation.install_and_handle());
                    return Ok(MainWindowInitialComposerProgress::Pending);
                }
                CommandOutcome::Committed {
                    receipt,
                    later_failure,
                    local_finalization,
                } => {
                    owner.open_noncommit = None;
                    owner.open_later_failure = later_failure;
                    owner.open_local_finalization = local_finalization;
                    owner.open_receipt = Some(receipt);
                }
            }
        }
        let outcome = owner
            .storage
            .reconcile_fresh_draft_editor_candidate_session_open_candidate(
                &access,
                owner.open.as_ref().unwrap(),
                CommandOutcome::Committed {
                    receipt: owner.open_receipt.as_ref().unwrap().clone(),
                    later_failure: None,
                    local_finalization: None,
                },
            )
            .map_err(|e| e.to_string())?;
        let head = match outcome {
            DraftEditorCandidateSessionOpenOutcomeV1::Opened(head)
            | DraftEditorCandidateSessionOpenOutcomeV1::ExactReplay(head) => head,
            other => {
                owner.open_terminal = true;
                return Err(format!(
                    "fresh recovery opening is not the exact active session: {other:?}"
                ));
            }
        };
        owner.opened = Some(head.clone());
        let result = owner
            .host
            .as_mut()
            .ok_or("fresh recovery host was consumed")?
            .finish_initial_activation_candidate(
                &access,
                owner.request.clone(),
                cancellation,
                owner.open.as_ref().unwrap().request().selector(),
                DraftEditorCandidateSessionOpenOutcomeV1::Opened(head),
            )
            .map_err(|e| e.to_string())?;
        if !matches!(result, ComposerHostActivationOutcome::Activated { .. }) {
            return Err(format!(
                "fresh recovery activation did not become ready: {result:?}"
            ));
        }
        self.validate_access(&access)?;
        drop(access);
        self.build_service(candidate)?;
        self.candidate.activated = true;
        Ok(MainWindowInitialComposerProgress::Activated)
    }

    fn build_service(&mut self, candidate: &HomeRecoveryCandidate) -> Result<(), String> {
        let owner = &mut self.candidate;
        let slot = match MainWindowComposerSlot::new_fresh_candidate(
            self.window.window_id(),
            owner.claim,
            owner.host.take().unwrap(),
            owner.storage.clone(),
            owner.marker_authority.take().unwrap(),
        ) {
            Ok(slot) => slot,
            Err((host, marker, error)) => {
                owner.host = Some(host);
                owner.marker_authority = Some(marker);
                return Err(error);
            }
        };
        owner.service = Some(Arc::new(MainWindowConversationComposerService::new(
            candidate.service_reference(),
            slot,
        )));
        Ok(())
    }

    pub fn prepare(
        &mut self,
        configurator: &mut impl FnMut(
            MainWindowComposerSelectionIdentity,
        ) -> Result<MainWindowConversationComposerConfig, String>,
    ) -> Result<MainWindowConversationComposerPreparedSelection, String> {
        if !self.candidate.activated
            || self.candidate.retirement_started
            || self.candidate.preparation_started
        {
            return Err("fresh recovery editor is unavailable for GUI preparation".into());
        }
        self.candidate.preparation_started = true;
        MainWindowConversationComposerMount::prepare_selected(
            self.candidate
                .service
                .as_ref()
                .ok_or("fresh recovery service is missing")?
                .clone(),
            configurator,
        )
    }

    pub(in crate::main_window) fn fence(
        &self,
        ticket: crate::main_window::MainWindowConversationComposerCloseTicket,
    ) -> Result<(), String> {
        self.candidate
            .service
            .as_ref()
            .ok_or("fresh recovery service is missing")?
            .fence_fresh_candidate(ticket)
    }

    pub fn capture_cleanup(
        mut self,
    ) -> Result<MainWindowInitialComposerRecoveryCleanup, (Self, String)> {
        self.candidate.retirement_started = true;
        if let Err(error) = retire_runtime(&mut self.candidate) {
            return Err((self, error));
        }
        Ok(self.candidate.into_recovery_cleanup(true))
    }

    pub(crate) fn release_publication(self) -> Result<(), Self> {
        if !self.candidate.activated
            || !self.candidate.preparation_started
            || self.candidate.retirement_started
            || self.candidate.service.is_none()
        {
            return Err(self);
        }
        Ok(())
    }
}

pub(super) fn retire_runtime(owner: &mut InitialComposerCandidate) -> Result<(), String> {
    if let Some(service) = owner.service.take() {
        let Some(selection) = service.selected_identity() else {
            owner.service = Some(service);
            return Err("fresh candidate selected identity is missing".into());
        };
        if let Err((service, error)) =
            service.retire_fresh_candidate_runtime(selection, owner.retirement_operation)
        {
            owner.service = Some(service);
            return Err(error);
        }
    }
    if let Some(host) = owner.host.as_mut() {
        if host.pending_request_count() != 0
            || host.settlement_custody_in_use() != 0
            || host.submission_pending()
            || host.is_dirty()
        {
            return Err("fresh candidate host retains work".into());
        }
        if let Some(ticket) = host.window_close_ticket() {
            host.release_window_close(ticket)
                .map_err(|e| e.to_string())?;
        }
        if host
            .dispose_composer_service(&owner.store)
            .map_err(|e| e.to_string())?
            != crate::composer_host::ComposerHostServiceDisposalCompletion::Disposed
        {
            return Err("fresh candidate host disposal remains pending".into());
        }
        owner.host.take();
    }
    Ok(())
}

fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(65_536).unwrap()
}

fn authenticate_window(
    access: &HomeCandidateRecoveryAccess<'_>,
    state: &BerylState,
    window: &SessionWindowRecord,
) -> Result<(), String> {
    let bootstrap = state
        .session()
        .minimal_bootstrap_candidate(access)
        .map_err(|e| e.to_string())?
        .ok_or("fresh recovery session is missing")?;
    let claim = window
        .selected_thread()
        .ok_or("fresh recovery requires the original selected claim")?;
    let paired = state
        .session()
        .window_claim_catalog_source_candidate(access, window.window_id())
        .map_err(|e| e.to_string())?;
    if !bootstrap.windows().iter().any(|actual| actual == window)
        || bootstrap.header().fallback() != window.remembered_target()
        || !paired.claim().is_some_and(|actual| {
            actual.thread_id() == claim.thread_id()
                && actual.generation() == claim.generation()
                && actual.revision() == claim.revision()
                && actual.state() == ThreadClaimState::Active
        })
    {
        return Err(
            "fresh recovery original window, fallback or paired Active claim changed".into(),
        );
    }
    Ok(())
}
