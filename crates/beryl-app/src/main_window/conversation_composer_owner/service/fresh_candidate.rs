use super::*;
use crate::main_window::MainWindowConversationComposerCloseTicket;

impl MainWindowConversationComposerService {
    #[cfg(feature = "test-faults")]
    pub fn test_fail_next_fresh_widget_release(&self) {
        self.test_fail_fresh_widget_release
            .store(true, Ordering::Release);
    }

    pub(in crate::main_window) fn release_fresh_candidate_widget(
        &self,
        selection: MainWindowComposerSelectionIdentity,
        requests: &[RangeTextInputRequest],
    ) -> Result<MainWindowComposerWidgetRelease, String> {
        #[cfg(feature = "test-faults")]
        if self
            .test_fail_fresh_widget_release
            .swap(false, Ordering::AcqRel)
        {
            return Err("injected fresh widget release validation failure".into());
        }
        self.slot
            .lock()
            .map_err(|_| "fresh candidate slot lock failed")?
            .release_fresh_candidate_widget(selection, requests)
    }

    pub(in crate::main_window) fn fence_fresh_candidate(
        &self,
        ticket: MainWindowConversationComposerCloseTicket,
    ) -> Result<(), String> {
        if self.selected_identity() != Some(ticket.selection()) {
            return Err("fresh candidate close selection changed".into());
        }
        let mut current = self
            .window_close
            .lock()
            .map_err(|_| "fresh candidate close gate lock failed")?;
        if current.is_some() {
            return Err("fresh candidate close gate is occupied or stale".into());
        }
        *current = Some(ticket);
        drop(current);
        self.begin_window_close_flush(ticket)?
            .ok_or_else(|| "fresh candidate close lane remains busy".to_owned())?;
        Ok(())
    }

    pub(in crate::main_window) fn retire_fresh_candidate_runtime(
        mut self: Arc<Self>,
        selection: MainWindowComposerSelectionIdentity,
        disposal: syndic_storage::DraftPieceOperationIdV1,
    ) -> Result<(), (Arc<Self>, String)> {
        let Some(service) = Arc::get_mut(&mut self) else {
            return Err((
                self,
                "fresh candidate GUI or service resources remain retained".into(),
            ));
        };
        if Arc::get_mut(&mut service.store).is_none()
            || service
                .native_lineage_driver_started
                .load(Ordering::Acquire)
            || !service
                .native_lineage_sources
                .get_mut()
                .is_ok_and(|sources| sources.is_empty())
        {
            return Err((
                self,
                "fresh candidate worker resources remain retained".into(),
            ));
        }
        let result = service
            .slot
            .get_mut()
            .map_err(|_| "fresh candidate slot lock failed".to_owned())
            .and_then(|slot| {
                slot.retire_fresh_candidate_runtime(&service.store, selection, disposal)
            });
        match result {
            Ok(()) => Ok(()),
            Err(error) => Err((self, error)),
        }
    }
}
