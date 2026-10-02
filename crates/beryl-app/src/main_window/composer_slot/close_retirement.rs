use super::*;
use crate::composer_host::{ComposerHostFlushTicket, ComposerHostRetiredClose};
use crate::main_window::MainWindowConversationComposerCloseTicket;

#[derive(Debug, Eq, PartialEq)]
pub struct MainWindowComposerRetiredClose {
    selection: MainWindowComposerSelectionIdentity,
    close: MainWindowConversationComposerCloseTicket,
    host: ComposerHostRetiredClose,
    last_activation_generation: u64,
}

impl MainWindowComposerRetiredClose {
    pub fn rebind_candidate(
        self,
        access: &beryl_home_store::HomeCandidateRecoveryAccess<'_>,
        storage: SyndicStorage,
        state: &beryl_state::BerylState,
    ) -> Result<
        (
            Box<MainWindowComposerSlot>,
            MainWindowConversationComposerCloseTicket,
            beryl_state::SessionWindowRecord,
        ),
        (Self, MainWindowComposerSlotError),
    > {
        self.rebind_candidate_for_recovery(access, storage, state, None)
    }

    pub(crate) fn rebind_candidate_for_recovery(
        self,
        access: &beryl_home_store::HomeCandidateRecoveryAccess<'_>,
        storage: SyndicStorage,
        state: &beryl_state::BerylState,
        recovered_window: Option<&beryl_state::SessionWindowRemovalEvidence>,
    ) -> Result<
        (
            Box<MainWindowComposerSlot>,
            MainWindowConversationComposerCloseTicket,
            beryl_state::SessionWindowRecord,
        ),
        (Self, MainWindowComposerSlotError),
    > {
        let result = (|| {
            let window = match recovered_window
                .filter(|e| e.window().window_id() == self.selection.window_id())
            {
                Some(evidence) => {
                    if evidence.window().selected_thread() != Some(self.selection.claim())
                        || state
                            .session()
                            .classify_window_removal_candidate(access, evidence)
                            .map_err(|_| MainWindowComposerSlotError::IdentityMismatch)?
                            != beryl_state::SessionWindowRemovalState::Recovered
                    {
                        return Err(MainWindowComposerSlotError::IdentityMismatch);
                    }
                    state
                        .session()
                        .minimal_bootstrap_candidate(access)?
                        .ok_or(MainWindowComposerSlotError::IdentityMismatch)?
                        .windows()
                        .iter()
                        .find(|window| window.window_id() == self.selection.window_id())
                        .cloned()
                        .ok_or(MainWindowComposerSlotError::IdentityMismatch)?
                }
                None => self.selection.validate_candidate_claim(access, state)?,
            };
            let claim = window
                .selected_thread()
                .ok_or(MainWindowComposerSlotError::IdentityMismatch)?;
            let assets = state.assets();
            assets.revision_candidate(access)?;
            let host = self.host.reconstruct_candidate(access, storage.clone())?;
            let (slot, close) = self.reconstruct_slot(
                host,
                storage,
                claim,
                MainWindowComposerMarkerMetadataAuthority::new(assets),
            )?;
            Ok((slot, close, window))
        })();
        result.map_err(|error| (self, error))
    }

    #[inline(never)]
    fn reconstruct_slot(
        &self,
        host: Box<SyndicComposerHost>,
        storage: SyndicStorage,
        claim: WindowClaimSelection,
        marker_authority: MainWindowComposerMarkerMetadataAuthority,
    ) -> Result<
        (
            Box<MainWindowComposerSlot>,
            MainWindowConversationComposerCloseTicket,
        ),
        MainWindowComposerSlotError,
    > {
        let mut slot = Box::new(MainWindowComposerSlot::new(
            self.selection.window_id(),
            claim,
            *host,
            storage,
            marker_authority,
        )?);
        slot.last_activation_generation = self.last_activation_generation;
        let close = self
            .close
            .with_recovered_selection(slot.selected_identity().unwrap());
        slot.window_close = Some(close);
        Ok((slot, close))
    }

    pub const fn selection(&self) -> MainWindowComposerSelectionIdentity {
        self.selection
    }

    pub const fn close_ticket(&self) -> MainWindowConversationComposerCloseTicket {
        self.close
    }

    pub const fn host(&self) -> &ComposerHostRetiredClose {
        &self.host
    }
}

impl MainWindowComposerSelectionIdentity {
    pub(in crate::main_window) fn validate_candidate_claim(
        self,
        access: &beryl_home_store::HomeCandidateRecoveryAccess<'_>,
        state: &beryl_state::BerylState,
    ) -> Result<beryl_state::SessionWindowRecord, MainWindowComposerSlotError> {
        let session = state.session();
        let bootstrap = session
            .minimal_bootstrap_candidate(access)?
            .ok_or(MainWindowComposerSlotError::IdentityMismatch)?;
        let claim = self.claim();
        let window = bootstrap
            .windows()
            .iter()
            .find(|window| {
                window.window_id() == self.window_id() && window.selected_thread() == Some(claim)
            })
            .ok_or(MainWindowComposerSlotError::IdentityMismatch)?;
        let paired = session.window_claim_catalog_source_candidate(access, self.window_id())?;
        if !paired.claim().is_some_and(|paired| {
            paired.thread_id() == claim.thread_id()
                && paired.generation() == claim.generation()
                && paired.revision() == claim.revision()
                && paired.state() == beryl_state::ThreadClaimState::Active
        }) {
            return Err(MainWindowComposerSlotError::IdentityMismatch);
        }
        Ok(window.clone())
    }
}

impl MainWindowComposerSlot {
    pub(in crate::main_window) fn export_detached_source(
        &self,
        store: &beryl_home_store::HomeStore,
        close: MainWindowConversationComposerCloseTicket,
        selection: MainWindowComposerSelectionIdentity,
        pool: &beryl_home_store::TemporaryReadPool,
        cancellation: &beryl_home_store::CommandCancellation,
    ) -> Result<syndic_storage::DetachedDraftReadSourceV1, String> {
        if self.window_close != Some(close) || self.selected_identity() != Some(selection) {
            return Err("detached export lost its exact closed candidate".into());
        }
        self.storage
            .export_detached_draft_read_source(
                store,
                selection.binding().candidate(),
                pool,
                syndic_storage::DetachedDraftReadLimitsV1::default(),
                cancellation,
            )
            .map_err(|error| format!("detached draft export failed: {error:?}"))
    }

    pub fn retire_clean_window_close(
        mut self: Box<Self>,
        close: MainWindowConversationComposerCloseTicket,
        flush: ComposerHostFlushTicket,
    ) -> Result<MainWindowComposerRetiredClose, Box<Self>> {
        self.take_clean_window_close(close, flush).ok_or(self)
    }

    pub(in crate::main_window) fn take_clean_window_close(
        &mut self,
        close: MainWindowConversationComposerCloseTicket,
        flush: ComposerHostFlushTicket,
    ) -> Option<MainWindowComposerRetiredClose> {
        if self.disposed
            || !self.window_close_is_current(close)
            || self.pending.is_some()
            || self.disposal_stage.is_some()
            || self.submission_successor.is_some()
            || self.native_lineage_suspension.is_some()
            || self.selected.as_ref().is_none_or(|selected| {
                !selected.dispatcher.is_drained()
                    || selected.host.binding() != Some(selected.identity.binding())
                    || selected.dispatcher.binding != selected.identity.binding()
            })
        {
            return None;
        }
        let SelectedComposer {
            identity,
            dispatcher,
            draft_state,
            host,
        } = self.selected.take().unwrap();
        match Box::new(host).retire_clean_window_close(flush) {
            Ok(host) => {
                self.disposed = true;
                Some(MainWindowComposerRetiredClose {
                    selection: identity,
                    close,
                    host,
                    last_activation_generation: self.last_activation_generation,
                })
            }
            Err(host) => {
                self.selected = Some(SelectedComposer {
                    identity,
                    dispatcher,
                    draft_state,
                    host: *host,
                });
                None
            }
        }
    }

    #[cfg(feature = "test-faults")]
    pub fn test_release_window_close_gate(
        &mut self,
        close: MainWindowConversationComposerCloseTicket,
        flush: Option<ComposerHostFlushTicket>,
    ) -> Result<bool, MainWindowComposerSlotError> {
        self.release_window_close_gate(close, flush)
    }

    #[cfg(feature = "test-faults")]
    pub fn test_begin_window_close_gate(
        &mut self,
        close: MainWindowConversationComposerCloseTicket,
    ) -> Result<(), MainWindowComposerSlotError> {
        self.begin_window_close_gate(close)
    }
}
