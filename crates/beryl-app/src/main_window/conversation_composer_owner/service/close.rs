use super::*;
use crate::composer_host::{
    ComposerHostFlushAdmission, ComposerHostFlushAdvance, ComposerHostFlushPurpose,
    ComposerHostFlushTicket,
};
use crate::main_window::MainWindowConversationComposerCloseTicket;
use std::sync::TryLockError;

impl MainWindowConversationComposerService {
    #[cfg(feature = "test-faults")]
    pub fn test_with_close_slot_locked<T>(&self, action: impl FnOnce() -> T) -> T {
        let _slot = self.slot.lock().unwrap();
        action()
    }

    #[cfg(feature = "test-faults")]
    pub fn test_window_close_is_current(
        &self,
        ticket: MainWindowConversationComposerCloseTicket,
    ) -> bool {
        self.window_close_is_current(ticket)
    }

    pub(in crate::main_window) fn begin_window_close_gate(
        &self,
        ticket: MainWindowConversationComposerCloseTicket,
    ) -> Result<(), String> {
        let health = self.store.health();
        if self.store.home_id() != ticket.selection().binding().home_id()
            || health.state() != beryl_home_store::HomeHealthState::Healthy
            || health.generation() != Some(ticket.selection().binding().home_generation())
        {
            return Err("conversation composer close home is unavailable".to_owned());
        }
        let mut current = self
            .window_close
            .lock()
            .map_err(|_| "conversation composer close gate lock failed".to_owned())?;
        if current.is_some() {
            return Err("conversation composer close is already active".to_owned());
        }
        *current = Some(ticket);
        Ok(())
    }

    pub(in crate::main_window) fn window_close_is_current(
        &self,
        ticket: MainWindowConversationComposerCloseTicket,
    ) -> bool {
        self.store.home_id() == ticket.selection().binding().home_id()
            && self.store.health().generation().is_none_or(|generation| {
                generation == ticket.selection().binding().home_generation()
            })
            && self
                .window_close
                .lock()
                .is_ok_and(|current| *current == Some(ticket))
    }

    pub(super) fn ensure_no_window_close(&self) -> Result<(), String> {
        if self
            .window_close
            .lock()
            .map_err(|_| "conversation composer close gate lock failed".to_owned())?
            .is_some()
        {
            return Err("conversation composer is waiting for window close".to_owned());
        }
        Ok(())
    }

    pub(in crate::main_window) fn begin_window_close_flush(
        &self,
        ticket: MainWindowConversationComposerCloseTicket,
    ) -> Result<Option<ComposerHostFlushAdmission>, String> {
        if !self.window_close_is_current(ticket) {
            return Err("conversation composer close ticket is stale".to_owned());
        }
        let mut slot = match self.slot.try_lock() {
            Ok(slot) => slot,
            Err(TryLockError::WouldBlock) => return Ok(None),
            Err(TryLockError::Poisoned(_)) => {
                return Err("conversation composer service lock failed".to_owned());
            }
        };
        if !slot.window_close_is_current(ticket) {
            slot.begin_window_close_gate(ticket)
                .map_err(|error| format!("conversation composer close gate failed: {error}"))?;
        }
        let selection = slot.selected_identity().unwrap();
        slot.begin_selected_flush(selection, ComposerHostFlushPurpose::WindowClose)
            .map(Some)
            .map_err(|error| format!("conversation composer close flush failed: {error}"))
    }

    pub(in crate::main_window) fn advance_window_close_flush(
        &self,
        ticket: MainWindowConversationComposerCloseTicket,
        flush: ComposerHostFlushTicket,
    ) -> Result<ComposerHostFlushAdvance, String> {
        let mut slot = self
            .slot
            .lock()
            .map_err(|_| "conversation composer service lock failed".to_owned())?;
        if !slot.window_close_is_current(ticket) {
            return Ok(ComposerHostFlushAdvance::Stale);
        }
        let selection = slot.selected_identity().unwrap();
        slot.advance_selected_flush(&self.store, selection, flush)
            .map_err(|error| format!("conversation composer close advance failed: {error}"))
    }

    pub(in crate::main_window) fn release_window_close_gate(
        &self,
        ticket: MainWindowConversationComposerCloseTicket,
        flush: Option<ComposerHostFlushTicket>,
    ) -> Result<Option<bool>, String> {
        if !self.window_close_is_current(ticket) {
            return Ok(Some(false));
        }
        let mut slot = match self.slot.try_lock() {
            Ok(slot) => slot,
            Err(TryLockError::WouldBlock) => return Ok(None),
            Err(TryLockError::Poisoned(_)) => {
                return Err("conversation composer service lock failed".to_owned());
            }
        };
        self.release_window_close_gate_in_slot(&mut slot, ticket, flush)
            .map(Some)
    }

    pub(in crate::main_window) fn authorize_window_close_disposal(
        &self,
        ticket: MainWindowConversationComposerCloseTicket,
        flush: ComposerHostFlushTicket,
    ) -> Result<Option<ComposerHostFlushAdvance>, String> {
        if !self.window_close_is_current(ticket) {
            return Ok(Some(ComposerHostFlushAdvance::Stale));
        }
        let mut slot = match self.slot.try_lock() {
            Ok(slot) => slot,
            Err(TryLockError::WouldBlock) => return Ok(None),
            Err(TryLockError::Poisoned(_)) => {
                return Err("conversation composer service lock failed".to_owned());
            }
        };
        slot.authorize_window_close_disposal(&self.store, ticket, flush)
            .map(Some)
            .map_err(|error| format!("conversation composer close disposal failed: {error}"))
    }

    pub(in crate::main_window) fn finish_window_close_gate(
        &self,
        ticket: MainWindowConversationComposerCloseTicket,
    ) {
        if let Ok(mut current) = self.window_close.lock()
            && *current == Some(ticket)
        {
            *current = None;
        }
    }

    pub(in crate::main_window) fn release_window_close_gate_wait(
        &self,
        ticket: MainWindowConversationComposerCloseTicket,
        flush: Option<ComposerHostFlushTicket>,
    ) -> Result<bool, String> {
        if !self.window_close_is_current(ticket) {
            return Ok(false);
        }
        let mut slot = self
            .slot
            .lock()
            .map_err(|_| "conversation composer service lock failed".to_owned())?;
        self.release_window_close_gate_in_slot(&mut slot, ticket, flush)
    }

    pub(super) fn release_window_close_gate_in_slot(
        &self,
        slot: &mut MainWindowComposerSlot,
        ticket: MainWindowConversationComposerCloseTicket,
        flush: Option<ComposerHostFlushTicket>,
    ) -> Result<bool, String> {
        if !self.window_close_is_current(ticket) {
            return Ok(false);
        }
        let mut current = self
            .window_close
            .lock()
            .map_err(|_| "conversation composer close gate lock failed".to_owned())?;
        if *current != Some(ticket) {
            return Ok(false);
        }
        if (flush.is_some() || slot.window_close_is_current(ticket))
            && !slot
                .release_window_close_gate(ticket, flush)
                .map_err(|error| format!("conversation composer close release failed: {error}"))?
        {
            return Ok(false);
        }
        *current = None;
        Ok(true)
    }
}
