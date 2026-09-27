use super::*;
use crate::composer_host::{ComposerHostFlushTicket, ComposerHostRetiredClose};
use crate::main_window::MainWindowConversationComposerCloseTicket;

#[derive(Debug, Eq, PartialEq)]
pub struct MainWindowComposerRetiredClose {
    selection: MainWindowComposerSelectionIdentity,
    close: MainWindowConversationComposerCloseTicket,
    host: ComposerHostRetiredClose,
}

impl MainWindowComposerRetiredClose {
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
    pub fn test_begin_window_close_gate(
        &mut self,
        close: MainWindowConversationComposerCloseTicket,
    ) -> Result<(), MainWindowComposerSlotError> {
        self.begin_window_close_gate(close)
    }
}
