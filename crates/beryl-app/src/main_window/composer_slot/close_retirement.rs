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
        ),
        (Self, MainWindowComposerSlotError),
    > {
        let result = (|| {
            let session = state.session();
            let bootstrap = session
                .minimal_bootstrap_candidate(access)?
                .ok_or(MainWindowComposerSlotError::IdentityMismatch)?;
            let claim = self.selection.claim();
            if !bootstrap.windows().iter().any(|window| {
                window.window_id() == self.selection.window_id()
                    && window.selected_thread() == Some(claim)
            }) {
                return Err(MainWindowComposerSlotError::IdentityMismatch);
            }
            let paired = session
                .window_claim_catalog_source_candidate(access, self.selection.window_id())?;
            if !paired.claim().is_some_and(|paired| {
                paired.thread_id() == claim.thread_id()
                    && paired.generation() == claim.generation()
                    && paired.revision() == claim.revision()
                    && paired.state() == beryl_state::ThreadClaimState::Active
            }) {
                return Err(MainWindowComposerSlotError::IdentityMismatch);
            }
            let assets = state.assets();
            assets.revision_candidate(access)?;
            let host = self.host.reconstruct_candidate(access, storage.clone())?;
            let mut slot = Box::new(MainWindowComposerSlot::new(
                self.selection.window_id(),
                claim,
                *host,
                storage,
                MainWindowComposerMarkerMetadataAuthority::new(assets),
            )?);
            slot.last_activation_generation = self.last_activation_generation;
            let close = self
                .close
                .with_recovered_selection(slot.selected_identity().unwrap());
            slot.window_close = Some(close);
            Ok((slot, close))
        })();
        result.map_err(|error| (self, error))
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

impl MainWindowComposerSlot {
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
