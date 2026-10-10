use super::*;
use crate::{
    syndic_transcript::TranscriptActivationPlacement,
    transcript_provider::{TranscriptAttachmentPurpose, TranscriptAttachmentRequest},
};
use std::sync::atomic::AtomicBool;

pub(super) struct PendingMeasuredAttachment {
    provider: crate::transcript_provider::TranscriptProviderReader,
    prepared: crate::transcript_provider::PreparedTranscriptAttachment,
    reader: crate::app_services::PublishedRunningThreadsReader,
    selection: crate::main_window::MainWindowComposerSelectionIdentity,
    request: TranscriptAttachmentRequest,
    cancel: Arc<AtomicBool>,
}

pub(in crate::main_window::shell::host) struct PendingRecoveredAttachment {
    home: Arc<beryl_home_store::HomeServiceReference>,
    home_generation: beryl_home_store::HomeGeneration,
    cancellation: beryl_home_store::CommandCancellation,
    selection: crate::main_window::MainWindowComposerSelectionIdentity,
    panel: Entity<crate::syndic_transcript::SyndicTranscriptPanel>,
    activation: crate::syndic_transcript::PreparedTranscriptActivation,
    title: Option<beryl_state::CatalogResolvedTitle>,
    lineage: Option<syndic_storage::ThreadLineageHead>,
}

impl MainWindowShellRoot {
    pub(crate) fn finish_recovered_transcript_publication(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<bool, String> {
        self.synchronize_transcript_appearance(cx);
        self.drive_recovered_transcript(window, cx)?;
        Ok(self.running_threads.recovered_attachment.is_none())
    }

    pub(in crate::main_window::shell::host) fn stage_recovered_transcript(
        &mut self,
        selection: crate::main_window::MainWindowComposerSelectionIdentity,
        activation: crate::syndic_transcript::PreparedTranscriptActivation,
        title: Option<beryl_state::CatalogResolvedTitle>,
        lineage: Option<syndic_storage::ThreadLineageHead>,
        publication_home: (
            Arc<beryl_home_store::HomeServiceReference>,
            beryl_home_store::HomeGeneration,
        ),
        cancellation: beryl_home_store::CommandCancellation,
        cx: &mut Context<Self>,
    ) {
        let panel = self.running_threads.transcript.clone();
        panel.update(cx, |panel, panel_cx| {
            panel.stage_activation(&activation, None);
            panel_cx.notify();
        });
        self.running_threads.recovered_attachment = Some(Ok(PendingRecoveredAttachment {
            selection,
            panel,
            activation,
            title,
            lineage,
            home: publication_home.0,
            home_generation: publication_home.1,
            cancellation,
        }));
        cx.notify();
    }

    pub(in crate::main_window::shell::host) fn drive_recovered_transcript(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let Some(pending) = self.running_threads.recovered_attachment.take() else {
            return Ok(());
        };
        let pending = match pending {
            Ok(pending) => pending,
            Err(error) => {
                self.running_threads.recovered_attachment = Some(Err(error.clone()));
                return Err(error);
            }
        };
        self.synchronize_transcript_appearance(cx);
        let exact_selection = self.controller.as_ref().is_some_and(|controller| {
            matches!(&controller.content, ShellContent::Selected { selection, .. } if *selection == pending.selection)
                && controller.composer_mount().is_some_and(|mount| {
                    mount.read(cx).fresh_recovery_ticket().is_some()
                        && mount.read(cx).contribution().is_some_and(|composer| {
                            composer.read(cx).selection_identity() == pending.selection
                        })
                })
        });
        if pending.cancellation.is_cancelled()
            || pending.panel != self.running_threads.transcript
            || !exact_selection
        {
            pending
                .panel
                .update(cx, |panel, _| panel.discard_staged_activation());
            let error = "Recovered transcript publication custody is stale or cancelled".to_owned();
            self.running_threads.recovered_attachment = Some(Err(error.clone()));
            return Err(error);
        }
        let mount = self
            .controller
            .as_ref()
            .and_then(|controller| controller.composer_mount());
        let allocation = self.transcript_attachment_allocation(
            mount.as_ref(),
            pending.title.is_some()
                && pending
                    .lineage
                    .as_ref()
                    .is_some_and(|lineage| lineage.total_parent_count() != 0),
            window,
            cx,
        );
        pending.panel.update(cx, |panel, _| {
            panel.stage_activation(&pending.activation, Some(allocation))
        });
        if !pending.panel.read(cx).measurement_current(window) {
            self.running_threads.recovered_attachment = Some(Ok(pending));
            return Ok(());
        }
        let published = pending
            .home
            .try_elect_coherent(pending.home_generation, || {
                pending.panel.update(cx, |panel, panel_cx| {
                    panel.publish_coherent_activation(pending.activation.clone());
                    panel_cx.notify();
                })
            });
        if let Err(error) = published {
            use beryl_home_store::{HomeCoherenceError, HomeHealthState};
            let waiting = matches!(
                error,
                HomeCoherenceError::Busy
                    | HomeCoherenceError::ReconciliationPending
                    | HomeCoherenceError::Unhealthy(
                        HomeHealthState::Opening | HomeHealthState::Reopening
                    )
            ) || (error == HomeCoherenceError::StaleGeneration
                && pending.home.health().state() == HomeHealthState::Reopening);
            if waiting {
                self.running_threads.recovered_attachment = Some(Ok(pending));
            } else {
                pending
                    .panel
                    .update(cx, |panel, _| panel.discard_staged_activation());
                let error = format!("Recovered transcript publication failed: {error:?}");
                self.running_threads.recovered_attachment = Some(Err(error.clone()));
                return Err(error);
            }
            return Ok(());
        }
        self.running_threads.transcript_claim = Some(pending.selection.claim());
        self.running_threads.selected_title = pending
            .title
            .clone()
            .map(|title| (pending.selection, title));
        if let (Some(title), Some(lineage)) = (pending.title, pending.lineage) {
            self.publish_selected_lineage(pending.selection, lineage, title, window, cx);
        }
        cx.notify();
        Ok(())
    }

    pub(in crate::main_window::shell::host) fn sync_running_transcript(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.synchronize_transcript_appearance(cx);
        if let Some(pending) = self.running_threads.measured_attachment.take() {
            let panel = self.running_threads.transcript.clone();
            if pending.cancel.load(Ordering::Acquire)
                || !pending.reader.current()
                || !self.running_threads_enabled()
                || pending.request.request_id != self.running_threads.transcript_request
                || !self
                    .running_threads
                    .reader
                    .as_ref()
                    .is_some_and(|reader| reader.same_publication(&pending.reader))
                || self
                    .cached_running_selection(cx)
                    .map(|(selection, _)| selection)
                    != Some(pending.selection)
            {
                panel.update(cx, |panel, _| panel.discard_staged_activation());
            } else {
                let mount = self
                    .controller
                    .as_ref()
                    .and_then(|controller| controller.composer_mount());
                let allocation = self.transcript_attachment_allocation(
                    mount.as_ref(),
                    pending.prepared.lineage_head().total_parent_count() != 0,
                    window,
                    cx,
                );
                panel.update(cx, |panel, _| {
                    panel.stage_activation(pending.prepared.activation(), Some(allocation))
                });
                if !panel.read(cx).measurement_current(window) {
                    self.running_threads.measured_attachment = Some(pending);
                    return;
                }
                let title = pending.prepared.resolved_title().clone();
                let lineage = pending.prepared.lineage_head().clone();
                let identity = pending.prepared.source_identity();
                if pending
                    .provider
                    .publish_if_current(
                        pending.prepared,
                        &pending.request,
                        &pending.cancel,
                        |activation| {
                            panel.update(cx, |panel, panel_cx| {
                                panel.publish_coherent_activation(activation);
                                panel_cx.notify();
                            });
                        },
                    )
                    .is_ok()
                {
                    self.running_threads.transcript_provider = Some(pending.provider);
                    self.running_threads.transcript_claim = Some(pending.selection.claim());
                    self.running_threads.transcript_source = Some(identity);
                    self.running_threads.selected_title = Some((pending.selection, title.clone()));
                    self.publish_selected_lineage(pending.selection, lineage, title, window, cx);
                    cx.notify();
                } else {
                    panel.update(cx, |panel, _| panel.discard_staged_activation());
                }
            }
            return;
        }
        if !self.running_threads_enabled()
            || !self.running_read_capacity_available()
            || self.running_threads.transcript_task.is_some()
            || self.running_threads.has_activation_custody()
            || self.running_threads.recovered_attachment.is_some()
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
            let _ = this.update_in(cx, |root, window, cx| {
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
                    let lineage = prepared.lineage_head().clone();
                    let source_identity = prepared.source_identity();
                    if root.running_threads.transcript_claim == Some(selection.claim())
                        && root.running_threads.transcript_source.as_ref() == Some(&source_identity)
                    {
                        if root
                            .running_threads
                            .selected_title
                            .as_ref()
                            .is_some_and(|(current, _)| *current == selection)
                            && root.running_threads.lineage.matches_selection(selection)
                        {
                            return;
                        }
                        if provider
                            .publish_if_current(prepared, &request, &cancel, |_| ())
                            .is_ok()
                        {
                            root.running_threads.transcript_provider = Some(provider);
                            root.running_threads.selected_title = Some((selection, title.clone()));
                            root.publish_selected_lineage(selection, lineage, title, window, cx);
                            cx.notify();
                        }
                        return;
                    }
                    panel.update(cx, |panel, panel_cx| {
                        panel.stage_activation(prepared.activation(), None);
                        panel_cx.notify();
                    });
                    root.running_threads.measured_attachment = Some(PendingMeasuredAttachment {
                        provider,
                        prepared,
                        reader,
                        selection,
                        request,
                        cancel,
                    });
                    cx.notify();
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
