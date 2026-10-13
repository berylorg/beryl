use super::*;

pub(super) fn subscribe_quote(
    transcript: &Entity<crate::syndic_transcript::SyndicTranscriptPanel>,
    window: &mut Window,
    cx: &mut Context<MainWindowShellRoot>,
) -> gpui::Subscription {
    cx.subscribe_in(
        transcript,
        window,
        |root, source, event: &crate::syndic_transcript::QuoteInsertionRequested, window, cx| {
            if root.running_threads.views_retired
                || root.running_threads.reads_suspended
                || root.running_selection_interaction_gated(cx)
                || root.ordinary_close_interaction_gated
                || root.shutdown_interaction_gated
                || root.startup_interaction_gated()
                || source != &root.running_threads.transcript
                || !source.read(cx).quote_request_current(
                    event.request,
                    event.activation,
                    event.presentation,
                    window,
                )
            {
                return;
            }
            let Some((selection, _)) = root.cached_running_selection(cx) else {
                return;
            };
            if root.running_threads.transcript_claim != Some(selection.claim()) {
                return;
            }
            let Some(composer) = root
                .controller
                .as_ref()
                .and_then(|controller| controller.composer_mount.as_ref())
                .and_then(|mount| mount.read(cx).contribution())
            else {
                return;
            };
            let result = composer.update(cx, |composer, cx| {
                if composer.selection_identity() != selection {
                    return Err("The Quote destination changed before insertion.".to_owned());
                }
                composer.insert_quote(
                    event.markdown.clone(),
                    &source,
                    event.request,
                    event.activation,
                    event.presentation,
                    event.cancellation.clone(),
                    window,
                    cx,
                )
            });
            if result.is_err() {
                cx.notify();
            }
        },
    )
}

impl MainWindowShellRoot {
    pub(super) fn subscribe_quote(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.subscribe_branch_discussion(window, cx);
        self.running_threads.image_preview_subscription =
            Some(super::transcript_image_preview::subscribe(
                &self.running_threads.transcript,
                window,
                cx,
            ));
        self.running_threads.quote_subscription = Some(subscribe_quote(
            &self.running_threads.transcript,
            window,
            cx,
        ));
        self.running_threads.historical_replacement_subscription = Some(
            super::historical_replacement::subscribe_historical_replacement(
                &self.running_threads.transcript,
                window,
                cx,
            ),
        );
    }
}
