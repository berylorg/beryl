use super::*;
use crate::{
    main_window::MainWindowShellRoot,
    theme_runtime::{AppearancePublicationTarget, GpuiAppearanceWindowSet},
};
use beryl_home_store::{CommandCancellation, HomeGeneration};
use gpui::{AsyncApp, WindowHandle};
use settlement::CandidateSettlement;
use std::time::Duration;
use syndic_storage::SyndicTimestamp;

impl RunningProcessOwner {
    pub(super) fn first_conversation_window(
        &self,
    ) -> Result<Option<WindowHandle<MainWindowShellRoot>>, String> {
        let Some(recovery) = self.interrupted_exit.as_ref() else {
            return Ok(None);
        };
        let session = recovery.session.borrow();
        if !session.as_ref().is_some_and(|s| s.has_first_admission()) {
            return Ok(None);
        }
        let shells = self.process.windows.shells();
        if shells.len() != 1 {
            return Err("original first conversation requires the sole preserved shell".into());
        }
        Ok(Some(shells[0].window()))
    }

    pub(super) async fn recover_first_conversation(
        owner: &impl RecoveryOwnerAccess,
        request: &impl RecoveryIdentity,
        retired: HomeGeneration,
        window: WindowHandle<MainWindowShellRoot>,
        at: SyndicTimestamp,
        cancellation: CommandCancellation,
        failed: impl FnMut(RecoveryPreparationFailure),
        cx: &mut AsyncApp,
    ) -> Result<(), String> {
        let result = Self::recover_first_conversation_pass(
            owner,
            request,
            retired,
            window,
            at,
            cancellation,
            failed,
            cx,
        )
        .await;
        if let Err(error) = result {
            let published = owner
                .recovery_owner()?
                .borrow()
                .interrupted_exit
                .as_ref()
                .is_some_and(|r| r.publication.borrow().as_ref().is_some_and(|r| r.is_ok()));
            if !published {
                if let Err(cleanup) = Self::settle_automatic_interrupted_exit_cancellation(
                    owner, request, retired, cx,
                )
                .await
                {
                    return Err(format!(
                        "{error}; first conversation cleanup retains custody: {cleanup}"
                    ));
                }
            }
            return Err(error);
        }
        Ok(())
    }

    async fn recover_first_conversation_pass(
        owner: &impl RecoveryOwnerAccess,
        request: &impl RecoveryIdentity,
        retired: HomeGeneration,
        window: WindowHandle<MainWindowShellRoot>,
        at: SyndicTimestamp,
        cancellation: CommandCancellation,
        failed: impl FnMut(RecoveryPreparationFailure),
        cx: &mut AsyncApp,
    ) -> Result<(), String> {
        let _driver = owner
            .recovery_owner()?
            .borrow_mut()
            .reserve_interrupted_exit_driver(request)?;
        let needs_retirement = owner
            .recovery_owner()?
            .borrow()
            .interrupted_exit_graph_retirement_result(request)
            .is_err();
        if needs_retirement {
            Self::retire_interrupted_exit_for_preparation(
                owner,
                request,
                retired,
                cancellation.clone(),
                cx,
            )
            .await?;
        }
        let prepared = owner
            .recovery_owner()?
            .borrow()
            .interrupted_exit_services_result(request)
            .is_ok();
        if !prepared {
            let configuration = owner
                .recovery_owner()?
                .borrow()
                .process
                .configuration
                .clone();
            Self::retry_interrupted_exit_preparation_attempts(
                owner,
                request,
                retired,
                configuration,
                at,
                cancellation.clone(),
                failed,
                cx,
            )
            .await?;
        }
        let facts = owner
            .recovery_owner()?
            .borrow()
            .interrupted_exit
            .as_ref()
            .unwrap()
            .session
            .borrow()
            .as_ref()
            .and_then(|s| s.first_conversation_facts())
            .cloned();
        let Some(facts) = facts else {
            return Self::complete_prepared_interrupted_exit_threadless(
                owner,
                request,
                retired,
                window,
                cancellation,
                cx,
            )
            .await;
        };
        loop {
            if cancellation.is_cancelled() {
                return Err("fresh first conversation preparation cancelled".into());
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
                    return Err("fresh first conversation graph custody is unavailable".into());
                };
                (slot, graph)
            };
            let cancellation = cancellation.clone();
            let facts = facts.clone();
            let work = cx.background_executor().spawn(async move {
                let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
                    graph.advance_first_conversation(&facts, &cancellation)
                }))
                .unwrap_or_else(|_| Err("fresh first conversation worker unwound".into()));
                (graph, result)
            });
            let (sender, receiver) = futures_channel::oneshot::channel();
            let retained = owner.recovery_owner()?;
            let identity = request.identity();
            cx.spawn(async move |_| {
                let (graph, mut result) = work.await;
                *slot.borrow_mut() = Some(CandidateSettlement::Services(Ok(graph)));
                if !retained.borrow().active_recovery_identity(&identity) {
                    result = Err("first conversation recovery request changed".into());
                }
                let _ = sender.send(result);
            })
            .detach();
            let result = receiver
                .await
                .map_err(|_| "first conversation worker delivery unavailable")?;
            owner
                .recovery_owner()?
                .borrow()
                .interrupted_exit_services_result(request)?;
            if result? {
                break;
            }
            cx.background_executor()
                .timer(Duration::from_millis(50))
                .await;
        }
        let (home_id, generation, appearance) = cx
            .update(|app| -> Result<_, String> {
                let retained = owner.recovery_owner()?;
                let mut retained = retained.borrow_mut();
                retained.interrupted_exit_services_result(request)?;
                let prepared = retained.interrupted_exit_appearance(request)?;
                let home_id = prepared.prepared().home().home_id();
                let generation = prepared.prepared().home().home_generation();
                let existing = retained
                    .interrupted_exit
                    .as_ref()
                    .unwrap()
                    .threadless_appearance
                    .clone();
                let appearance = if let Some(appearance) = existing {
                    appearance
                } else {
                    let previous = retained.process.appearance.clone();
                    let capacity = std::num::NonZeroUsize::new(
                        previous.read(app).target().snapshot().capacity,
                    )
                    .ok_or("first conversation appearance capacity is unavailable")?;
                    previous.update(app, |set, _| set.retire());
                    let appearance = GpuiAppearanceWindowSet::new(prepared, capacity, app);
                    retained
                        .interrupted_exit
                        .as_mut()
                        .unwrap()
                        .threadless_appearance = Some(appearance.clone());
                    appearance
                };
                window
                    .update(app, |root, native, cx| {
                        retained.attach_first_conversation(request, root, native, cx)
                    })
                    .map_err(|e| e.to_string())??;
                retained.bind_interrupted_exit_appearance(request, window, &appearance, app)?;
                Ok((home_id, generation, appearance))
            })
            .map_err(|e| e.to_string())??;
        loop {
            if cancellation.is_cancelled() {
                return Err("fresh first conversation widget preparation cancelled".into());
            }
            let ready = cx
                .update(|app| -> Result<bool, String> {
                    let retained = owner.recovery_owner()?;
                    let retained = retained.borrow();
                    retained.interrupted_exit_services_result(request)?;
                    let drafts = retained.recovery_drafts()?;
                    window
                        .update(app, |root, native, cx| {
                            drafts
                                .borrow()
                                .advance_first_conversation(root, home_id, generation, native, cx)
                        })
                        .map_err(|e| e.to_string())?
                })
                .map_err(|e| e.to_string())??;
            if ready {
                break;
            }
            cx.background_executor()
                .timer(Duration::from_millis(50))
                .await;
        }
        Self::publish_and_complete_interrupted_exit_pass(
            owner,
            request,
            retired,
            generation,
            &appearance,
            cancellation,
            cx,
        )
        .await
    }

    fn attach_first_conversation(
        &mut self,
        request: &impl RecoveryIdentity,
        root: &mut MainWindowShellRoot,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<MainWindowShellRoot>,
    ) -> Result<(), String> {
        self.interrupted_exit_services_result(request)?;
        #[cfg(test)]
        let reject_attachment = self.reject_ordinary_recovery_attachment_after.take() == Some(1);
        #[cfg(test)]
        let reject_release = std::mem::take(&mut self.reject_first_conversation_widget_release);
        let recovery = self.interrupted_exit.as_ref().unwrap();
        let facts = recovery
            .session
            .borrow()
            .as_ref()
            .and_then(|s| s.first_conversation_facts())
            .cloned()
            .ok_or("original committed first conversation admission is unavailable")?;
        let mut slot = recovery.settlement.borrow_mut();
        let Some(CandidateSettlement::Services(Ok(graph))) = slot.as_mut() else {
            unreachable!()
        };
        let home = graph.appearance().prepared().home();
        let home_id = home.home_id();
        let generation = home.home_generation();
        let requirement = self
            .process
            .configuration
            .projection
            .turn_start_admission_requirement();
        let adapters = graph.composer_recovery_adapters(home_id, generation, requirement)?;
        let configure = graph.first_conversation_configurator()?;
        let transcript = graph.first_conversation_transcript()?;
        let first = graph.first_conversation_preparation()?;
        let drafts = self.recovery_drafts()?;
        drafts.borrow_mut().adopt_first_conversation(
            root, &facts, first, adapters, configure, transcript, window, cx,
        )?;
        #[cfg(test)]
        if reject_attachment {
            if reject_release {
                first
                    .service()
                    .unwrap()
                    .test_fail_next_fresh_widget_release();
            }
            return Err("injected first conversation attachment completion refusal".into());
        }
        Ok(())
    }
}
