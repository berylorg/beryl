use super::*;
use crate::composer_host::ComposerHostServiceDisposalCompletion;

impl MainWindowComposerSlot {
    pub(in crate::main_window) fn release_fresh_candidate_widget(
        &self,
        selection: MainWindowComposerSelectionIdentity,
        requests: &[gpui_text_input::RangeTextInputRequest],
    ) -> Result<MainWindowComposerWidgetRelease, String> {
        if self.selected_identity() != Some(selection)
            || self.pending.is_some()
            || self.disposal_stage.is_some()
            || self.submission_successor.is_some()
            || self.native_lineage_suspension.is_some()
            || self
                .selected
                .as_ref()
                .is_none_or(|selected| !selected.dispatcher.is_drained())
        {
            return Err("fresh candidate widget retains admitted work".into());
        }
        if requests
            .iter()
            .any(|request| !Self::widget_release_request_is_settled(request))
        {
            return Err(MainWindowComposerSlotError::WidgetReleaseIncomplete.to_string());
        }
        Ok(MainWindowComposerWidgetRelease::new(selection))
    }

    pub(in crate::main_window) fn new_fresh_candidate(
        window_id: WindowId,
        claim: WindowClaimSelection,
        host: SyndicComposerHost,
        storage: SyndicStorage,
        marker: MainWindowComposerMarkerMetadataAuthority,
    ) -> Result<
        Self,
        (
            SyndicComposerHost,
            MainWindowComposerMarkerMetadataAuthority,
            String,
        ),
    > {
        let ready = (|| {
            let binding = host
                .binding()
                .ok_or("fresh candidate host is not activated")?;
            if host.active_thread_id() != Some(claim.thread_id()) {
                return Err("fresh candidate claim and host differ".to_owned());
            }
            let draft_state = draft_state_for_host(&host, binding).map_err(|e| e.to_string())?;
            Ok((binding, draft_state))
        })();
        let (binding, draft_state) = match ready {
            Ok(ready) => ready,
            Err(error) => return Err((host, marker, error)),
        };
        Ok(Self::from_selected(
            window_id,
            storage,
            marker,
            SelectedComposer {
                identity: MainWindowComposerSelectionIdentity {
                    window_id,
                    claim,
                    binding,
                },
                dispatcher: MainWindowComposerDispatcher::new(binding),
                draft_state,
                host,
            },
        ))
    }

    pub(in crate::main_window) fn retire_fresh_candidate_runtime(
        &mut self,
        store: &HomeStore,
        selection: MainWindowComposerSelectionIdentity,
        disposal: DraftPieceOperationIdV1,
    ) -> Result<(), String> {
        if self.selected_identity() != Some(selection)
            || self.pending.is_some()
            || self.disposal_stage.is_some()
            || self.submission_successor.is_some()
            || self.native_lineage_suspension.is_some()
            || self
                .selected
                .as_ref()
                .is_none_or(|selected| !selected.dispatcher.is_drained())
        {
            return Err("fresh candidate slot retains work or changed identity".into());
        }
        let selected = self.selected.as_mut().unwrap();
        if selected.host.pending_request_count() != 0
            || selected.host.settlement_custody_in_use() != 0
            || selected.host.submission_pending()
            || selected.host.is_dirty()
        {
            return Err("fresh candidate host is not joined at its opening".into());
        }
        if let Some(ticket) = selected.host.window_close_ticket() {
            selected
                .host
                .release_window_close(ticket)
                .map_err(|e| e.to_string())?;
        }
        if selected.host.fresh_abandonment_request(disposal).is_none() {
            return Err("fresh candidate host departed its opening".into());
        }
        match selected
            .host
            .dispose_composer_service(store)
            .map_err(|e| e.to_string())?
        {
            ComposerHostServiceDisposalCompletion::Disposed => {
                self.selected = None;
                self.window_close = None;
                self.disposed = true;
                Ok(())
            }
            ComposerHostServiceDisposalCompletion::Pending => {
                Err("fresh candidate runtime retirement remains pending".into())
            }
        }
    }
}
