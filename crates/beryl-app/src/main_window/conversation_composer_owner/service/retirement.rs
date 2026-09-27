use super::*;
use crate::composer_host::ComposerHostFlushTicket;
use crate::main_window::{
    MainWindowComposerRetiredClose, MainWindowConversationComposerCloseTicket,
};

impl MainWindowConversationComposerService {
    pub fn retire_clean_window_close(
        mut self: Arc<Self>,
        close: MainWindowConversationComposerCloseTicket,
        flush: ComposerHostFlushTicket,
    ) -> Result<MainWindowComposerRetiredClose, Arc<Self>> {
        let Some(service) = Arc::get_mut(&mut self) else {
            return Err(self);
        };
        if !service.window_close_is_current(close)
            || Arc::get_mut(&mut service.store).is_none()
            || service
                .native_lineage_driver_started
                .load(Ordering::Acquire)
            || !service
                .native_lineage_sources
                .get_mut()
                .is_ok_and(|sources| sources.is_empty())
        {
            return Err(self);
        }
        let result = service
            .slot
            .get_mut()
            .ok()
            .and_then(|slot| slot.take_clean_window_close(close, flush));
        result.ok_or(self)
    }

    #[cfg(feature = "test-faults")]
    pub fn test_begin_window_close_gate(
        &self,
        close: MainWindowConversationComposerCloseTicket,
    ) -> Result<(), String> {
        self.begin_window_close_gate(close)
    }

    #[cfg(feature = "test-faults")]
    pub fn test_retain_home_reference(&self) -> Arc<HomeServiceReference> {
        self.store.clone()
    }
}
