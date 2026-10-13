use super::*;

impl MainWindowShellRoot {
    pub(super) fn sync_historical_record_dimming(&mut self, cx: &mut Context<Self>) {
        let composer = self
            .controller
            .as_ref()
            .and_then(|controller| controller.composer_mount.as_ref())
            .and_then(|mount| mount.read(cx).contribution());
        let edit_reason = if self.running_threads.views_retired || self.running_threads.reads_suspended
            || self.running_selection_interaction_gated(cx) || self.ordinary_close_interaction_gated
            || self.shutdown_interaction_gated || self.startup_interaction_gated() {
            Some("Wait for the selected conversation to become available.")
        } else {
            composer.as_ref().map_or(Some("Select an available conversation first."), |composer| composer.read(cx).historical_edit_disabled_reason(cx))
        };
        self.running_threads.transcript.update(cx, |panel,_| panel.set_historical_edit_disabled_reason(edit_reason));
        let intent = composer.as_ref().and_then(|composer| {
            let composer = composer.read(cx);
            (self.running_threads.transcript_claim == Some(composer.selection_identity().claim()))
                .then(|| composer.historical_replacement_target())
                .flatten()
        });
        let panel = self.running_threads.transcript.clone();
        let snapshot = panel.read(cx).snapshot();
        let key = (
            snapshot.activation_revision,
            snapshot.presentation_revision,
            intent,
        );
        if self.running_threads.historical_dimming_key == Some(key) {
            return;
        }
        let records = snapshot
            .records
            .iter()
            .filter_map(|record| {
                let intent = intent?;
                let crate::syndic_transcript::ResidentRecordSource::Syndic(source) =
                    &record.provenance.source
                else {
                    return None;
                };
                let entry = source.transcript_entry?;
                let expected = intent.transcript_entry();
                let thread = self
                    .running_threads
                    .transcript_claim?
                    .thread_id()
                    .to_string();
                let same_target = source
                    .turn_id
                    .as_ref()
                    .is_some_and(|turn| turn.0 == intent.target_turn_id().to_string());
                (source.view_id.0 == thread
                    && entry.generation() == expected.generation()
                    && (same_target || entry.position().get() >= expected.position().get()))
                .then(|| record.id.clone())
            })
            .collect();
        panel.update(cx, |panel, _| panel.set_record_dimming(records));
        self.running_threads.historical_dimming_key = Some(key);
    }
}

pub(super) fn subscribe_historical_replacement(
    transcript: &Entity<crate::syndic_transcript::SyndicTranscriptPanel>,
    window: &mut Window,
    cx: &mut Context<MainWindowShellRoot>,
) -> gpui::Subscription {
    cx.subscribe_in(
        transcript,
        window,
        |root,
         source,
         event: &crate::syndic_transcript::HistoricalReplacementRequested,
         window,
         cx| {
            if root.running_threads.views_retired
                || root.running_threads.reads_suspended
                || root.running_selection_interaction_gated(cx)
                || root.ordinary_close_interaction_gated
                || root.shutdown_interaction_gated
                || root.startup_interaction_gated()
                || source != &root.running_threads.transcript
                || !source
                    .read(cx)
                    .historical_replacement_request_current(event, window)
            {
                return;
            }
            let Some((selection, _)) = root.cached_running_selection(cx) else {
                return;
            };
            if root.running_threads.transcript_claim != Some(selection.claim()) {
                return;
            }
            let Some(mount) = root
                .controller
                .as_ref()
                .and_then(|controller| controller.composer_mount.as_ref())
            else {
                return;
            };
            let Some(composer) = mount.read(cx).contribution() else {
                return;
            };
            let Ok((_, marker_seals)) = mount.read(cx).claim_flush_resources() else {
                return;
            };
            let Some(target) = event.target.record.historical_target() else {
                return;
            };
            if target.thread_id != selection.claim().thread_id() {
                return;
            }
            let target = crate::composer_host::ComposerHostHistoricalTarget {
                thread_id: target.thread_id,
                turn_id: target.turn_id,
                item_id: target.item_id,
                transcript: target.transcript,
            };
            let result = composer.update(cx, |composer, cx| {
                if composer.selection_identity() != selection {
                    return Err("The historical Edit destination changed.".to_owned());
                }
                if let Some(reason) = composer.historical_edit_disabled_reason(cx) {
                    return Err(reason.to_owned());
                }
                composer.start_historical_replacement(
                    target,
                    event.cancellation.clone(),
                    marker_seals,
                    window,
                    cx,
                )
            });
            if let Err(error) = result {
                composer.update(cx, |composer, cx| {
                    composer.report_historical_replacement_refusal(error, cx)
                });
                cx.notify();
            }
        },
    )
}
