use super::*;
use crate::{
    app_services::runtime_setup::RuntimeSetupFlight,
    syndic_transcript::TranscriptActivationPlacement,
    transcript_provider::{
        PreparedTranscriptAttachment, TranscriptAttachmentPurpose, TranscriptAttachmentRequest,
        TranscriptProviderReader,
    },
};
use std::sync::atomic::{AtomicBool, Ordering};

pub(super) struct SetupTranscript {
    provider: TranscriptProviderReader,
    request: TranscriptAttachmentRequest,
    prepared: PreparedTranscriptAttachment,
}

impl MainWindowShellRoot {
    pub(crate) fn retire_setup_first_mount(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<bool, String> {
        self.runtime_setup.cancel_reads();
        self.runtime_setup
            .transcript_cancel
            .store(true, Ordering::Release);
        self.runtime_setup.transcript = None;
        let Some(mount) = self.runtime_setup.mount.clone() else {
            return Ok(self.runtime_setup.workers.retained() == 0);
        };
        if let Some(error) = &self.runtime_setup.mount_release_error {
            return Err(error.clone());
        }
        if let Some(release) = &self.runtime_setup.mount_release {
            if !mount.update(cx, |mount, cx| {
                mount.retire_unpublished_first_mount(release, cx)
            })? {
                return Ok(false);
            }
            if self.runtime_setup.workers.retained() != 0 {
                return Ok(false);
            }
            self.runtime_setup.mount = None;
            self.runtime_setup.mount_release = None;
            if let Some(flight) = &self.runtime_setup.flight {
                flight.with_first_conversation(|first| {
                    first.mark_mount_released();
                    Ok(())
                })?;
            }
            return Ok(true);
        }
        if self.runtime_setup.mount_release_task.is_some() {
            return Ok(false);
        }
        let resident = mount
            .read(cx)
            .contribution()
            .ok_or("original first conversation resident is missing")?;
        let release = resident.update(cx, |resident, cx| {
            resident.release_startup_widget(window, cx)
        })?;
        self.runtime_setup.mount_release_task = Some(cx.spawn_in(window, async move |this, cx| {
            let result = release.await;
            let _ = this.update_in(cx, |root, window, cx| {
                root.runtime_setup.mount_release_task = None;
                match result {
                    Ok(release) => root.runtime_setup.mount_release = Some(release),
                    Err(error) => root.runtime_setup.mount_release_error = Some(error),
                }
                let _ = root.retire_setup_first_mount(window, cx);
                cx.notify();
            });
        }));
        Ok(false)
    }

    pub(super) fn drive_first_conversation(
        &mut self,
        flight: &Arc<RuntimeSetupFlight>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let services = self
            .runtime_setup
            .services
            .clone()
            .filter(|services| services.current())
            .ok_or("First conversation services retired.")?;
        if flight.cancellation().is_cancelled() {
            return Err(
                "First conversation preparation was cancelled; its original outcome is retained."
                    .into(),
            );
        }
        if self.runtime_setup.mount.is_none() {
            if !services.prepare_first_conversation(flight)? {
                return Ok(());
            }
            let (prepared, configure, marker, submission) =
                flight.with_first_conversation(|first| {
                    let controller = self
                        .controller
                        .as_ref()
                        .ok_or("First conversation shell is unavailable.")?;
                    if !controller.is_threadless()
                        || controller.window_id() != first.window().window_id()
                        || controller.composer_mount.is_some()
                    {
                        return Err(
                            "First conversation no longer matches its original threadless shell."
                                .into(),
                        );
                    }
                    first.take_mount_inputs()
                })?;
            match MainWindowConversationComposerMount::from_prepared_entity_retained(
                prepared,
                configure,
                marker,
                submission,
                true,
                #[cfg(feature = "test-faults")]
                false,
                window,
                cx,
            ) {
                Ok(mount) => self.runtime_setup.mount = Some(mount),
                Err((error, partial)) => {
                    if partial.is_none() {
                        flight.with_first_conversation(|first| {
                            first.mark_mount_released();
                            Ok(())
                        })?;
                    }
                    self.runtime_setup.mount = partial;
                    return Err(error);
                }
            }
        }
        let mount = self.runtime_setup.mount.as_ref().unwrap().clone();
        if !mount.read(cx).selected_first_presentable(cx) {
            return Ok(());
        }
        let composer = mount
            .read(cx)
            .contribution()
            .ok_or("First conversation composer is unavailable.")?;
        let selection = composer.read(cx).selection_identity();
        if self.runtime_setup.transcript.is_none() {
            self.prepare_setup_transcript(selection, window, cx)?;
            return Ok(());
        }
        if !services.revalidate_first_conversation(flight)? {
            return Ok(());
        }
        composer.update(cx, |composer, cx| {
            composer.prepare_startup_interaction_release(cx)
        })?;
        let transcript = self.runtime_setup.transcript.take().unwrap();
        let cancellation = self.runtime_setup.transcript_cancel.clone();
        flight.publish_first_conversation(|first| {
            transcript
                .provider
                .publish_if_current(
                    transcript.prepared,
                    &transcript.request,
                    &cancellation,
                    |activation| {
                        let controller = self
                            .controller
                            .as_ref()
                            .ok_or("First conversation controller is unavailable.")?;
                        if !controller.is_threadless()
                            || controller.window_id() != first.window().window_id()
                            || controller.composer_mount.is_some()
                            || first.selection() != Some(selection)
                            || first.window().selected_thread() != Some(selection.claim())
                        {
                            return Err("First conversation publication identity changed.".into());
                        }
                        first.mark_published_at_coherent_boundary()?;
                        let record = first.window().clone();
                        let controller = self.controller.as_mut().unwrap();
                        controller
                            .retire_construction()
                            .expect("qualified threadless shell retirement is infallible");
                        let ShellContent::Retired { reservation, .. } = &mut controller.content
                        else {
                            unreachable!()
                        };
                        let reservation = reservation
                            .take()
                            .expect("first conversation retains its original native reservation");
                        controller.content = ShellContent::Selected {
                            window: record,
                            selection,
                            reservation,
                        };
                        controller.composer_mount = Some(mount.clone());
                        let input = composer.read(cx).gpui_input();
                        self.composer_observer = Some(cx.observe(&input, |_, _, cx| cx.notify()));
                        self.running_threads.transcript.update(cx, |panel, cx| {
                            panel.publish_coherent_activation(activation);
                            cx.notify();
                        });
                        self.running_threads.transcript_claim = Some(selection.claim());
                        composer.update(cx, |composer, cx| {
                            composer.commit_startup_interaction_release(cx)
                        });
                        Ok(())
                    },
                )
                .map_err(|error| error.to_string())?
        })?;
        self.running_threads.transcript_provider = Some(transcript.provider);
        flight.finish_first_conversation()?;
        self.runtime_setup.mount = None;
        self.runtime_setup.flight = None;
        self.runtime_setup.first_preparing = false;
        self.runtime_setup.first_revalidating = false;
        self.runtime_setup.picker = None;
        self.runtime_setup.subscription = None;
        self.runtime_setup.command = None;
        window.focus(&self.shell_focus);
        cx.notify();
        Ok(())
    }

    fn prepare_setup_transcript(
        &mut self,
        selection: crate::main_window::MainWindowComposerSelectionIdentity,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if self.runtime_setup.transcript_task.is_some() {
            return Ok(());
        }
        let reader = self.running_threads.reader.clone();
        #[cfg(all(test, feature = "test-faults"))]
        let reader = reader.or_else(|| self.runtime_setup.fixture_transcript_reader.clone());
        let reader = reader.ok_or("First conversation transcript source is unavailable.")?;
        let panel = self.running_threads.transcript.clone();
        let request = TranscriptAttachmentRequest {
            window_id: selection.window_id(),
            host: panel.read(cx).lifetime(),
            activation: selection.binding().host_generation().get(),
            thread_id: selection.claim().thread_id(),
            request_id: 1,
            placement: TranscriptActivationPlacement::Tail,
            purpose: TranscriptAttachmentPurpose::Attach,
        };
        self.runtime_setup.transcript_cancel = Arc::new(AtomicBool::new(false));
        let cancellation = self.runtime_setup.transcript_cancel.clone();
        let cancel = cancellation.clone();
        let release = self.runtime_setup.workers.track(|| ());
        let work = self.runtime_setup.workers.track(move || {
            let result = (|| {
                let provider = reader
                    .transcript_provider()
                    .map_err(|error| error.to_string())?;
                let prepared = provider
                    .prepare_attachment(request.clone(), &cancel)
                    .map_err(|error| error.to_string())?;
                Ok::<_, String>(SetupTranscript {
                    provider,
                    request,
                    prepared,
                })
            })();
            (result, release)
        });
        let job = cx.background_executor().spawn(async move { work.run() });
        self.runtime_setup.transcript_task = Some(cx.spawn_in(window, async move |this, cx| {
            let (result, _release) = job.await;
            let _ = this.update_in(cx, |root, _, cx| {
                root.runtime_setup.transcript_task = None;
                if cancellation.load(Ordering::Acquire)
                    || !root.setup_enabled()
                    || root
                        .runtime_setup
                        .mount
                        .as_ref()
                        .and_then(|mount| mount.read(cx).selected_identity())
                        != Some(selection)
                {
                    return;
                }
                match result {
                    Ok(prepared) => root.runtime_setup.transcript = Some(prepared),
                    Err(error) => root.setup_failure(&error, true),
                }
                cx.notify();
            });
        }));
        Ok(())
    }
}
