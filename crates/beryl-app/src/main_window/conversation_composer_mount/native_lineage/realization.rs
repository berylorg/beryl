use gpui::prelude::*;
use gpui::{Context, Window};
use gpui_text_input::{
    RangePrepublicationDelivery, RangePrepublicationStatus, RangeTextInputRequest,
};

use super::{NativeLineageHostResult, *};
use crate::main_window::{
    MainWindowNativeLineagePrepublicationResult, MainWindowNativeLineagePrepublicationSource,
    MainWindowNativeLineagePrepublicationWork,
};

impl MainWindowConversationComposerMount {
    pub(super) fn drive_native_lineage_realization(
        &mut self,
        leaving: Option<(&NativeLineageRecoveryControl, NativeLineageRecoveryKey)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.service_native_lineage_cleanup(window, cx)?;
        if self.native_lineage_session.is_none()
            && self.native_lineage_candidate.is_none()
            && self.native_lineage_source.is_some()
        {
            return Ok(());
        }
        if self.native_lineage_session.is_none() && self.native_lineage_candidate.is_none() {
            let seed = self
                .native_lineage_seed
                .ok_or_else(|| "composer recovery seed is unavailable".to_owned())?;
            let config = self
                .native_lineage_config
                .as_ref()
                .ok_or_else(|| "composer recovery configuration is unavailable".to_owned())?;
            if self.native_lineage_cleanup.is_none() {
                self.native_lineage_cleanup = Some(
                    gpui_text_input::RangePrepublicationCleanupLedger::new(
                        window.text_system(),
                        config.native_lineage_cleanup_slots()?,
                    )
                    .map_err(|error| {
                        format!("composer prepublication cleanup ledger was rejected: {error:?}")
                    })?,
                );
            }
            let cleanup = self
                .native_lineage_cleanup
                .as_ref()
                .cloned()
                .expect("prepublication cleanup ledger initialized");
            let environment = config.native_lineage_environment(
                self.native_lineage_next_environment,
                window.text_system(),
                cleanup.clone(),
            )?;
            self.native_lineage_next_environment = self
                .native_lineage_next_environment
                .checked_add(1)
                .ok_or_else(|| {
                    "composer prepublication environment identity exhausted".to_owned()
                })?;
            let mut session =
                gpui_text_input::RangePrepublicationSession::new(seed, environment.clone())
                    .map_err(|error| {
                        format!("composer prepublication session was rejected: {error:?}")
                    })?;
            let selection = self
                .native_lineage_selection
                .ok_or_else(|| "native lineage recovery selection is unavailable".to_owned())?;
            let source = MainWindowNativeLineagePrepublicationSource::new(
                selection,
                environment.id(),
                session.generation(),
                cleanup,
            );
            match self
                .service
                .retain_native_lineage_source(source.clone(), cx.background_executor().clone())
            {
                Ok(()) => {}
                Err(MainWindowNativeLineageSourceRetentionError::CapacityFull { epoch }) => {
                    session.cancel();
                    self.native_lineage_cleanup = None;
                    self.native_lineage_capacity_blocked_epoch = Some(epoch);
                    self.native_lineage_failure = Some(
                        "Composer restoration is waiting for an earlier restoration cleanup to finish. Your draft remains preserved."
                            .to_owned(),
                    );
                    cx.notify();
                    return Ok(());
                }
                Err(MainWindowNativeLineageSourceRetentionError::Failed(error)) => {
                    session.cancel();
                    self.native_lineage_cleanup = None;
                    return Err(error);
                }
            }
            self.native_lineage_environment = Some(environment);
            self.native_lineage_session = Some(session);
            self.native_lineage_source = Some(source);
        }

        self.start_next_native_lineage_host_effect(window, cx)?;
        if self.native_lineage_host_result.is_some() || !self.native_lineage_effects.is_empty() {
            return Ok(());
        }
        if self.native_lineage_candidate.is_none() {
            let session = self
                .native_lineage_session
                .as_mut()
                .ok_or_else(|| "composer prepublication session is unavailable".to_owned())?;
            let step = session.service(window.text_system());
            self.native_lineage_effects.extend(step.effects);
            match step.status {
                RangePrepublicationStatus::Ready => {
                    self.native_lineage_candidate = session.take_candidate();
                    if self.native_lineage_candidate.is_none() {
                        return Err("composer prepublication candidate was unavailable".to_owned());
                    }
                    self.native_lineage_session = None;
                }
                RangePrepublicationStatus::Cancelled
                | RangePrepublicationStatus::Stale
                | RangePrepublicationStatus::Failed(_) => {
                    return self.fail_native_lineage_realization(
                        format!(
                            "Composer restoration could not be realized: {:?}",
                            step.status
                        ),
                        window,
                        cx,
                    );
                }
                RangePrepublicationStatus::Initializing
                | RangePrepublicationStatus::Validating
                | RangePrepublicationStatus::WaitingForResponse
                | RangePrepublicationStatus::Advancing
                | RangePrepublicationStatus::CapacityBlocked => {}
            }
            self.start_next_native_lineage_host_effect(window, cx)?;
            if self.native_lineage_candidate.is_none()
                || self.native_lineage_host_result.is_some()
                || !self.native_lineage_effects.is_empty()
            {
                return Ok(());
            }
        }
        self.publish_native_lineage_candidate(leaving, window, cx)
    }

    fn publish_native_lineage_candidate(
        &mut self,
        leaving: Option<(&NativeLineageRecoveryControl, NativeLineageRecoveryKey)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let selection = self
            .native_lineage_selection
            .ok_or_else(|| "native lineage recovery selection is unavailable".to_owned())?;
        let config = self
            .native_lineage_config
            .as_ref()
            .ok_or_else(|| "composer recovery configuration is unavailable".to_owned())?;
        let proposed = config.native_lineage_current();
        let current = self
            .service
            .attest_native_lineage_prepublication_current(selection, proposed)?;
        let environment = self
            .native_lineage_environment
            .as_ref()
            .ok_or_else(|| "composer prepublication environment is unavailable".to_owned())?;
        let candidate = self
            .native_lineage_candidate
            .as_ref()
            .ok_or_else(|| "composer prepublication candidate is unavailable".to_owned())?;
        if candidate.source_binding() != current.binding
            || candidate.history() != current.history
            || candidate.environment_id() != environment.id()
            || candidate.adoption_peak().bytes > current.available_capacity.bytes
            || candidate.adoption_peak().items > current.available_capacity.items
        {
            return self.fail_native_lineage_mount(
                "Composer restoration became stale before publication.".to_owned(),
                window,
                cx,
            );
        }

        let config = self
            .native_lineage_config
            .take()
            .expect("composer prepublication configuration checked");
        let environment = self
            .native_lineage_environment
            .take()
            .expect("composer prepublication environment checked");
        let candidate = self
            .native_lineage_candidate
            .take()
            .expect("composer prepublication candidate checked");
        let restored = cx.new(|composer_cx| {
            MainWindowConversationComposer::new_restored(
                config,
                self.service.clone(),
                environment,
                candidate,
                current,
                MainWindowConversationComposer::production_clipboard_writer(),
                window,
                composer_cx,
            )
            .expect("prechecked prepublication composer adoption")
        });
        self.service
            .complete_native_lineage_restoration(selection)?;
        self.native_lineage_widget_release = None;
        self.contribution = Some(restored.clone());
        self.subscribe_to_contribution(window, cx)?;
        self.initialize_autosave(window, cx)?;
        restored.update(cx, |composer, composer_cx| {
            composer.focus_input(window, composer_cx)
        });
        if let Some((control, key)) = leaving {
            control.acknowledge_leaving(key).map_err(|_| {
                "native lineage recovery route changed before restoration".to_owned()
            })?;
        }
        self.clear_native_lineage_mount_state();
        cx.notify();
        Ok(())
    }

    fn start_next_native_lineage_host_effect(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if self.native_lineage_host_result.is_some() {
            return Ok(());
        }
        let Some(effect) = self.native_lineage_effects.pop_front() else {
            return Ok(());
        };
        let route = self
            .native_lineage_snapshot
            .ok_or_else(|| "native lineage recovery snapshot is unavailable".to_owned())?
            .key();
        let selection = self
            .native_lineage_selection
            .ok_or_else(|| "native lineage recovery selection is unavailable".to_owned())?;
        let seed = self
            .native_lineage_seed
            .ok_or_else(|| "composer recovery seed is unavailable".to_owned())?;
        let source = self
            .native_lineage_source
            .as_ref()
            .cloned()
            .ok_or_else(|| "composer prepublication source is unavailable".to_owned())?;
        if source.selection() != selection {
            return Err("composer prepublication source selection is stale".to_owned());
        }
        let work = source.begin(effect)?;
        let (token, generation) = match &work {
            MainWindowNativeLineagePrepublicationWork::Validation { token, request } => {
                (*token, request.key.generation)
            }
            MainWindowNativeLineagePrepublicationWork::Page {
                token, generation, ..
            }
            | MainWindowNativeLineagePrepublicationWork::ObjectPage {
                token, generation, ..
            } => (*token, *generation),
        };
        let service = self.service.clone();
        let source_for_task = source.clone();
        cx.background_executor()
            .spawn(async move {
                let (token, result) = match work {
                    MainWindowNativeLineagePrepublicationWork::Validation { token, request } => {
                        #[cfg(feature = "test-faults")]
                        if let Some(gate) = service.take_test_native_lineage_validation_gate() {
                            gate.await;
                        }
                        #[cfg(feature = "test-faults")]
                        let forced_failure = service.take_test_native_lineage_validation_failure();
                        #[cfg(not(feature = "test-faults"))]
                        let forced_failure = false;
                        (
                            token,
                            MainWindowNativeLineagePrepublicationResult::Validation(
                                if forced_failure {
                                    Err("composer prepublication validation failed for test"
                                        .to_owned())
                                } else {
                                    service.validate_native_lineage_prepublication(
                                        selection, seed, request,
                                    )
                                },
                            ),
                        )
                    }
                    MainWindowNativeLineagePrepublicationWork::Page {
                        token,
                        generation,
                        request,
                    } => {
                        #[cfg(feature = "test-faults")]
                        if let Some(gate) = service.take_test_native_lineage_page_gate() {
                            gate.await;
                        }
                        #[cfg(feature = "test-faults")]
                        let forced_failure = service.take_test_native_lineage_page_failure();
                        #[cfg(not(feature = "test-faults"))]
                        let forced_failure = false;
                        (
                            token,
                            MainWindowNativeLineagePrepublicationResult::Request(
                                if forced_failure {
                                    Err("composer prepublication page failed for test".to_owned())
                                } else {
                                    service.dispatch_native_lineage_prepublication(
                                        selection,
                                        RangeTextInputRequest::Page(request),
                                    )
                                },
                            ),
                        )
                    }
                    MainWindowNativeLineagePrepublicationWork::ObjectPage {
                        token,
                        generation,
                        request,
                    } => {
                        #[cfg(feature = "test-faults")]
                        if let Some(gate) = service.take_test_native_lineage_object_page_gate() {
                            gate.await;
                        }
                        (
                            token,
                            MainWindowNativeLineagePrepublicationResult::Request(
                                service.dispatch_native_lineage_prepublication(
                                    selection,
                                    RangeTextInputRequest::ObjectPage(request),
                                ),
                            ),
                        )
                    }
                };
                source_for_task.finish(token, result);
            })
            .detach();
        self.native_lineage_host_result = Some(NativeLineageHostResult::Settled {
            route,
            selection,
            generation,
            token,
            source,
        });
        Ok(())
    }

    pub(super) fn finish_native_lineage_host_result(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let Some(result) = self.native_lineage_host_result.take() else {
            return Ok(());
        };
        match result {
            NativeLineageHostResult::Settled {
                route,
                selection,
                generation,
                token,
                source,
            } => {
                if !self.native_lineage_host_result_is_current(route, selection, generation) {
                    return Ok(());
                }
                let Some(result) = source.take(token) else {
                    self.native_lineage_host_result = Some(NativeLineageHostResult::Settled {
                        route,
                        selection,
                        generation,
                        token,
                        source,
                    });
                    return Ok(());
                };
                let session = self
                    .native_lineage_session
                    .as_mut()
                    .ok_or_else(|| "composer prepublication session is unavailable".to_owned())?;
                let delivery = match result {
                    MainWindowNativeLineagePrepublicationResult::Validation(result) => {
                        session.deliver_validation(result?)
                    }
                    MainWindowNativeLineagePrepublicationResult::Request(Ok(
                        crate::main_window::MainWindowComposerDispatchOutcome::Page(page),
                    )) => session.deliver_page(generation, page),
                    MainWindowNativeLineagePrepublicationResult::Request(Ok(
                        crate::main_window::MainWindowComposerDispatchOutcome::ObjectPage(page),
                    )) => session.deliver_object_page(generation, page),
                    MainWindowNativeLineagePrepublicationResult::Request(Err(error)) => {
                        return Err(error);
                    }
                    _ => {
                        return Err(
                            "composer prepublication host returned an unexpected outcome"
                                .to_owned(),
                        );
                    }
                };
                self.accept_native_lineage_delivery(delivery, window, cx)?;
            }
        }
        Ok(())
    }

    fn native_lineage_host_result_is_current(
        &self,
        route: NativeLineageRecoveryKey,
        selection: MainWindowComposerSelectionIdentity,
        generation: gpui_text_input::RangePrepublicationSessionGeneration,
    ) -> bool {
        self.native_lineage_snapshot
            .is_some_and(|snapshot| snapshot.key() == route)
            && self.native_lineage_selection == Some(selection)
            && self
                .native_lineage_session
                .as_ref()
                .is_some_and(|session| session.generation() == generation)
    }

    fn accept_native_lineage_delivery(
        &mut self,
        delivery: RangePrepublicationDelivery,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        match delivery {
            RangePrepublicationDelivery::Accepted
            | RangePrepublicationDelivery::CapacityBlocked
            | RangePrepublicationDelivery::Obsolete => Ok(()),
            RangePrepublicationDelivery::Terminal(failure) => self.fail_native_lineage_mount(
                format!("Composer restoration delivery failed: {failure:?}"),
                window,
                cx,
            ),
        }
    }

    pub(super) fn service_native_lineage_cleanup(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.service.drive_native_lineage_cleanup_sources();
        if self
            .native_lineage_source
            .as_ref()
            .is_some_and(|source| source.drained())
        {
            self.native_lineage_source = None;
            self.native_lineage_cleanup = None;
            self.service.retire_native_lineage_sources();
            return Ok(());
        }
        Ok(())
    }
}
