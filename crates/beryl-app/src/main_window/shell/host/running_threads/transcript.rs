use super::*;
use crate::{
    syndic_transcript::TranscriptActivationPlacement,
    transcript_provider::{TranscriptAttachmentPurpose, TranscriptAttachmentRequest},
};
use std::sync::atomic::AtomicBool;

impl MainWindowShellRoot {
    pub(in crate::main_window::shell::host) fn sync_running_transcript(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.running_threads_enabled()
            || !self.running_read_capacity_available()
            || self.running_threads.transcript_task.is_some()
            || self.running_threads.has_activation_custody()
        {
            return;
        }
        let Some((selection, _)) = self.cached_running_selection(cx) else {
            return;
        };
        let Some(reader) = self.running_threads.reader.clone() else {
            return;
        };
        let Some(request_id) = self.running_threads.transcript_request.checked_add(1) else {
            return;
        };
        self.running_threads.transcript_request = request_id;
        let panel = self.running_threads.transcript.clone();
        let prior_claim = self.running_threads.transcript_claim;
        let placement = if prior_claim == Some(selection.claim()) {
            panel.read(cx).refresh_placement()
        } else {
            TranscriptActivationPlacement::Tail
        };
        let request = TranscriptAttachmentRequest {
            window_id: selection.window_id(),
            host: panel.read(cx).lifetime(),
            activation: selection.binding().host_generation().get(),
            thread_id: selection.claim().thread_id(),
            request_id,
            placement,
            purpose: if prior_claim == Some(selection.claim()) {
                TranscriptAttachmentPurpose::Refresh
            } else {
                TranscriptAttachmentPurpose::Attach
            },
        };
        self.running_threads.transcript_cancel = Arc::new(AtomicBool::new(false));
        let cancel = self.running_threads.transcript_cancel.clone();
        let provider = self.running_threads.transcript_provider.clone();
        let source = reader.clone();
        let expected = request.clone();
        let cancelled = cancel.clone();
        let output_release = self
            .running_threads
            .workers
            .track(release_running_read_output as fn());
        let work = self.running_threads.workers.track(move || {
            let result = (|| {
                let provider = match provider {
                    Some(provider) => provider,
                    None => source.transcript_provider()?,
                };
                let prepared = provider.prepare_attachment(expected, &cancelled)?;
                Ok::<_, crate::transcript_provider::TranscriptAttachmentError>((provider, prepared))
            })();
            RunningReadOutput {
                result,
                _release: output_release,
            }
        });
        let job = cx.background_executor().spawn(async move { work.run() });
        self.running_threads.transcript_task = Some(cx.spawn_in(window, async move |this, cx| {
            let output = job.await;
            let _ = this.update_in(cx, |root, _, cx| {
                let RunningReadOutput { _release, result } = output;
                if root.running_threads.transcript_request != request_id {
                    return;
                }
                root.running_threads.transcript_task = None;
                if cancel.load(Ordering::Acquire)
                    || !reader.current()
                    || !root.running_threads_enabled()
                    || root.running_threads.transcript != panel
                    || !root
                        .running_threads
                        .reader
                        .as_ref()
                        .is_some_and(|current| current.same_publication(&reader))
                    || root
                        .cached_running_selection(cx)
                        .map(|(current, _)| current)
                        != Some(selection)
                    || (prior_claim == Some(selection.claim())
                        && panel.read(cx).refresh_placement() != placement)
                {
                    return;
                }
                if let Ok((provider, prepared)) = result {
                    let title = prepared.resolved_title().clone();
                    let source_identity = prepared.source_identity();
                    if root.running_threads.transcript_claim == Some(selection.claim())
                        && root.running_threads.transcript_source.as_ref() == Some(&source_identity)
                    {
                        return;
                    }
                    if provider
                        .publish_if_current(prepared, &request, &cancel, |seed| {
                            panel.update(cx, |panel, panel_cx| {
                                let outcome = panel.publish_coherent_activation(seed);
                                panel_cx.notify();
                                outcome
                            })
                        })
                        .is_ok()
                    {
                        root.running_threads.transcript_provider = Some(provider);
                        root.running_threads.transcript_claim = Some(selection.claim());
                        root.running_threads.transcript_source = Some(source_identity);
                        root.running_threads.selected_title = Some((selection, title));
                        cx.notify();
                    }
                }
            });
        }));
    }

    pub(crate) fn coherent_selected_thread_title(
        &self,
        app: &gpui::App,
    ) -> Option<(
        beryl_state::WindowClaimSelection,
        beryl_state::CatalogResolvedTitle,
    )> {
        if self.startup_interaction_gated()
            || self.shutdown_interaction_gated
            || self.ordinary_close_interaction_gated
            || self.running_threads.views_retired
        {
            return None;
        }
        let selected = self.cached_running_selection(app)?.0;
        let (expected, title) = self.running_threads.selected_title.as_ref()?;
        (expected.window_id() == selected.window_id()
            && expected.claim() == selected.claim()
            && expected.binding().home_id() == selected.binding().home_id()
            && expected.binding().home_generation() == selected.binding().home_generation()
            && self.running_threads.transcript_claim == Some(selected.claim()))
        .then(|| (selected.claim(), title.clone()))
    }
}
