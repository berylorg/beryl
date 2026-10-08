use super::*;
use settlement::CandidateSettlement;

impl RunningProcessOwner {
    pub(super) async fn recover_owned_thread_creation(
        owner: &impl RecoveryOwnerAccess,
        request: &impl RecoveryIdentity,
        at: syndic_storage::SyndicTimestamp,
        cancellation: beryl_home_store::CommandCancellation,
        configure: impl FnMut(
            gpui::WindowHandle<crate::main_window::MainWindowShellRoot>,
        )
            -> Result<resident_windows_driver::ResidentRecoveryConfigurator, String>,
        failed: impl FnMut(RecoveryPreparationFailure),
        cx: &mut gpui::AsyncApp,
    ) -> Result<(), String> {
        enum Entry {
            Initial,
            Prepared,
            Retired {
                generation: beryl_home_store::HomeGeneration,
                driver: Rc<()>,
            },
        }
        let entry = {
            let retained = owner.recovery_owner()?;
            let mut retained = retained.borrow_mut();
            if !retained.active_recovery_identity(&request.identity()) {
                return Err("New Thread recovery request changed".into());
            }
            if cancellation.is_cancelled() {
                return Err("New Thread recovery continuation was cancelled".into());
            }
            let creation = retained
                .interrupted_exit
                .as_ref()
                .and_then(|recovery| recovery.drafts.as_ref())
                .map(|drafts| drafts.borrow().has_captured_thread_creation())
                .transpose()?
                .unwrap_or(false);
            let published = retained
                .interrupted_exit
                .as_ref()
                .ok_or("New Thread original recovery is unavailable")?
                .publication
                .borrow()
                .is_some();
            if creation && published {
                Entry::Prepared
            } else if retained
                .process
                .services
                .as_ref()
                .and_then(|services| services.graph())
                .is_some()
                || !creation
            {
                Entry::Initial
            } else {
                retained.interrupted_exit_graph_retirement_result(request)?;
                let prepared =
                    match retained
                        .interrupted_exit
                        .as_ref()
                        .unwrap()
                        .settlement
                        .borrow()
                        .as_ref()
                    {
                        Some(CandidateSettlement::Services(Ok(_))) => true,
                        Some(CandidateSettlement::DisposedPreparationFailure { .. })
                        | Some(CandidateSettlement::DisposedFailure(_)) => false,
                        _ => return Err(
                            "New Thread recovery continuation still owns unsettled candidate work"
                                .into(),
                        ),
                    };
                if prepared {
                    Entry::Prepared
                } else {
                    let retired = retained
                        .process
                        .services
                        .as_ref()
                        .ok_or("New Thread original service owner is unavailable")?
                        .retired_service_generation()
                        .map_err(|error| error.to_string())?;
                    let driver = retained.reserve_interrupted_exit_driver(request)?;
                    Entry::Retired {
                        generation: retired,
                        driver,
                    }
                }
            }
        };
        match entry {
            Entry::Initial => {
                Self::recover_interrupted_exit(
                    owner,
                    request,
                    at,
                    cancellation,
                    configure,
                    failed,
                    cx,
                )
                .await
            }
            Entry::Prepared => {
                Self::recover_prepared_interrupted_exit(owner, request, cancellation, cx).await
            }
            Entry::Retired { generation, driver } => {
                let configuration = owner
                    .recovery_owner()?
                    .borrow()
                    .process
                    .configuration
                    .clone();
                Self::retry_interrupted_exit_preparation_attempts(
                    owner,
                    request,
                    generation,
                    configuration,
                    at,
                    cancellation.clone(),
                    failed,
                    cx,
                )
                .await?;
                drop(driver);
                Self::recover_prepared_interrupted_exit(owner, request, cancellation, cx).await
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn test_drop_ready_thread_creation_graph(&mut self) {
        self.drop_thread_creation_graph = true;
    }

    #[cfg(test)]
    pub(crate) fn test_returned_thread_creation_graphs(&self) -> usize {
        self.returned_thread_creation_graphs
    }

    #[cfg(test)]
    fn test_return_original_thread_creation_graph(
        &mut self,
        request: &impl RecoveryIdentity,
        mut graph: Box<crate::app_services::recovery_graph::PreparedRecoveryServiceGraph>,
        window: beryl_model::WindowId,
    ) -> Box<crate::app_services::recovery_graph::PreparedRecoveryServiceGraph> {
        let identity = graph
            .thread_creation_parts(window)
            .unwrap()
            .0
            .selection()
            .unwrap();
        let service = graph
            .thread_creation_parts(window)
            .unwrap()
            .0
            .service()
            .unwrap();
        let original_service = std::sync::Arc::downgrade(&service);
        drop(service);
        graph.validate_thread_creations().unwrap();
        let generation = identity.binding().home_generation();
        drop(graph);
        let configuration = self.process.configuration.clone();
        let services = self.process.services.as_mut().unwrap();
        assert!(services.has_returned_thread_creation_graph());
        assert!(services.running_selection_pending());
        assert!(services.test_recovery_process_is_fenced());
        let retired = services
            .retired_service_generation_for_home_return(identity.binding().home_id())
            .unwrap();
        let refusal = services.prepare_recovery_service_graph(
            retired,
            &mut None,
            configuration,
            syndic_storage::SyndicTimestamp::from_unix_millis(2),
            &beryl_home_store::CommandCancellation::new(),
        );
        assert!(
            matches!(refusal, Err(crate::app_services::recovery_graph::RecoveryServicePreparationError::Refused(error)) if error.contains("exclusive recovery custody"))
        );
        self.interrupted_exit_services_result(request).unwrap();
        let slot = self.interrupted_exit.as_ref().unwrap().settlement.clone();
        let Some(CandidateSettlement::Services(Ok(graph))) =
            slot.borrow_mut().replace(CandidateSettlement::Pending)
        else {
            panic!("original graph was not restored");
        };
        let mut graph = Box::new(graph);
        graph.validate_thread_creations().unwrap();
        let preparation = graph.thread_creation_parts(window).unwrap().0;
        assert_eq!(preparation.selection(), Some(identity));
        assert_eq!(
            preparation.selection().unwrap().binding().home_generation(),
            generation
        );
        let restored = preparation.service().unwrap();
        assert!(std::sync::Arc::ptr_eq(
            &restored,
            &original_service.upgrade().unwrap()
        ));
        drop(restored);
        assert!(
            !self
                .process
                .services
                .as_ref()
                .unwrap()
                .has_returned_thread_creation_graph()
        );
        assert!(
            self.process
                .services
                .as_ref()
                .unwrap()
                .running_selection_pending()
        );
        assert!(
            self.process
                .services
                .as_ref()
                .unwrap()
                .test_recovery_process_is_fenced()
        );
        self.returned_thread_creation_graphs += 1;
        graph
    }
    pub(super) fn committed_thread_creation_window(
        &self,
        request: &impl RecoveryIdentity,
        window: beryl_model::WindowId,
    ) -> Result<bool, String> {
        self.interrupted_exit_services_result(request)?;
        let slot = self.interrupted_exit.as_ref().unwrap().settlement.borrow();
        let Some(CandidateSettlement::Services(Ok(graph))) = slot.as_ref() else {
            return Err("New Thread recovery graph is unavailable".into());
        };
        Ok(graph.has_committed_thread_creation(window))
    }

    pub(super) async fn attach_recovered_thread_creation_pass(
        owner: &impl RecoveryOwnerAccess,
        request: &impl RecoveryIdentity,
        window: gpui::WindowHandle<crate::main_window::MainWindowShellRoot>,
        appearance: &gpui::Entity<crate::theme_runtime::GpuiAppearanceWindowSet>,
        cancellation: beryl_home_store::CommandCancellation,
        cx: &mut gpui::AsyncApp,
    ) -> Result<
        (
            crate::main_window::MainWindowConversationComposerCloseTicket,
            beryl_state::SessionWindowRecord,
        ),
        String,
    > {
        let window_id = cx
            .update(|app| {
                window
                    .read(app)
                    .map_err(|error| error.to_string())
                    .and_then(|root| {
                        root.controller()
                            .map(|controller| controller.window_id())
                            .ok_or("New Thread recovery controller is missing".into())
                    })
            })
            .map_err(|error| error.to_string())??;
        loop {
            if cancellation.is_cancelled() {
                return Err("New Thread recovery target preparation cancelled".into());
            }
            let (slot, mut graph) = {
                let retained = owner.recovery_owner()?;
                let retained = retained.borrow();
                retained.interrupted_exit_services_result(request)?;
                let slot = retained
                    .interrupted_exit
                    .as_ref()
                    .unwrap()
                    .settlement
                    .clone();
                let Some(CandidateSettlement::Services(Ok(graph))) =
                    slot.borrow_mut().replace(CandidateSettlement::Pending)
                else {
                    return Err("New Thread candidate custody is unavailable".into());
                };
                (slot, Box::new(graph))
            };
            let cancel = cancellation.clone();
            let work = cx.background_executor().spawn(async move {
                let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
                    graph.advance_thread_creation(window_id, &cancel)
                }))
                .unwrap_or_else(|_| Err("New Thread recovery target worker unwound".into()));
                (graph, result)
            });
            let (graph, result) = work.await;
            *slot.borrow_mut() = Some(CandidateSettlement::Services(Ok(*graph)));
            if result? {
                break;
            }
            cx.background_executor()
                .timer(std::time::Duration::from_millis(50))
                .await;
        }
        let (home, generation, close, record) = cx
            .update(|app| -> Result<_, String> {
                let retained = owner.recovery_owner()?;
                let mut retained = retained.borrow_mut();
                retained.interrupted_exit_services_result(request)?;
                let drafts = retained.recovery_drafts()?;
                let recovery = retained.interrupted_exit.as_ref().unwrap();
                let mut slot = recovery.settlement.borrow_mut();
                let Some(CandidateSettlement::Services(Ok(graph))) = slot.as_mut() else {
                    return Err("New Thread recovery graph is unavailable".into());
                };
                let home = graph.appearance().prepared().home();
                let home_id = home.home_id();
                let generation = home.home_generation();
                let adapters = graph.composer_recovery_adapters(
                    home_id,
                    generation,
                    retained
                        .process
                        .configuration
                        .projection
                        .turn_start_admission_requirement(),
                )?;
                let configure = graph.thread_creation_configurator()?;
                let (preparation, retirement, transcript) =
                    graph.thread_creation_parts(window_id)?;
                let record = preparation.window().clone();
                let close = window
                    .update(app, |root, native, cx| {
                        drafts.borrow_mut().adopt_recovered_thread_creation(
                            root,
                            preparation,
                            retirement,
                            adapters,
                            configure,
                            transcript,
                            native,
                            cx,
                        )
                    })
                    .map_err(|error| error.to_string())??;
                drop(slot);
                retained.bind_interrupted_exit_appearance(request, window, appearance, app)?;
                Ok((home_id, generation, close, record))
            })
            .map_err(|error| error.to_string())??;
        #[cfg(test)]
        cx.update(|_| -> Result<(), String> {
            let retained = owner.recovery_owner()?;
            let mut retained = retained.borrow_mut();
            if std::mem::take(&mut retained.drop_thread_creation_graph) {
                let slot = retained
                    .interrupted_exit
                    .as_ref()
                    .unwrap()
                    .settlement
                    .clone();
                let Some(CandidateSettlement::Services(Ok(graph))) =
                    slot.borrow_mut().replace(CandidateSettlement::Pending)
                else {
                    return Err("ready New Thread graph is unavailable".into());
                };
                let graph = retained.test_return_original_thread_creation_graph(
                    request,
                    Box::new(graph),
                    window_id,
                );
                *slot.borrow_mut() = Some(CandidateSettlement::Services(Ok(*graph)));
            }
            Ok(())
        })
        .map_err(|error| error.to_string())??;
        loop {
            if cancellation.is_cancelled() {
                return Err("New Thread recovery widget preparation cancelled".into());
            }
            let ready = cx
                .update(|app| -> Result<bool, String> {
                    let retained = owner.recovery_owner()?;
                    let retained = retained.borrow();
                    retained.interrupted_exit_services_result(request)?;
                    let drafts = retained.recovery_drafts()?;
                    window
                        .update(app, |root, native, cx| {
                            drafts.borrow().advance_recovered_thread_creation(
                                root, home, generation, native, cx,
                            )
                        })
                        .map_err(|error| error.to_string())?
                })
                .map_err(|error| error.to_string())??;
            if ready {
                return Ok((close, record));
            }
            cx.background_executor()
                .timer(std::time::Duration::from_millis(50))
                .await;
        }
    }
}
