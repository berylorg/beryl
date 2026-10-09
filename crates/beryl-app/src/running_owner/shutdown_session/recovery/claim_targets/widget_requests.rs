use super::*;

impl RunningProcessOwner {
    pub(super) async fn prepare_recovered_claim_widget_batch(
        owner: &impl RecoveryOwnerAccess,
        request: &impl RecoveryIdentity,
        window: gpui::WindowHandle<crate::main_window::MainWindowShellRoot>,
        window_id: beryl_model::WindowId,
        home: beryl_model::BerylHomeId,
        generation: beryl_home_store::HomeGeneration,
        cancellation: &beryl_home_store::CommandCancellation,
        cx: &mut gpui::AsyncApp,
    ) -> Result<(), String> {
        let batch = cx
            .update(|app| -> Result<_, String> {
                let retained = owner.recovery_owner()?;
                let retained = retained.borrow();
                retained.interrupted_exit_services_result(request)?;
                let drafts = retained.recovery_drafts()?;
                window
                    .update(app, |root, native, cx| {
                        drafts
                            .borrow()
                            .claim_widget_batch(root, home, generation, native, cx)
                    })
                    .map_err(|error| error.to_string())?
            })
            .map_err(|error| error.to_string())??;
        let Some(batch) = batch else {
            return Ok(());
        };
        if cancellation.is_cancelled() {
            return Err("fresh ordinary widget batch cancelled before worker".into());
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
            let graph = {
                let mut current = slot.borrow_mut();
                if !matches!(current.as_ref(), Some(CandidateSettlement::Services(Ok(_)))) {
                    return Err("fresh ordinary widget graph custody is unavailable".into());
                }
                let Some(CandidateSettlement::Services(Ok(graph))) =
                    current.replace(CandidateSettlement::Pending)
                else {
                    unreachable!()
                };
                graph
            };
            (slot, Box::new(graph))
        };
        let cancel = cancellation.clone();
        let work = cx.background_executor().spawn(async move {
            let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
                graph.prepare_claim_widget_batch(window_id, batch, &cancel)
            }))
            .unwrap_or_else(|_| Err("fresh ordinary widget candidate worker unwound".into()));
            Box::new((graph, result))
        });
        let (graph, result) = *work.await;
        *slot.borrow_mut() = Some(CandidateSettlement::Services(Ok(*graph)));
        result?;
        if cancellation.is_cancelled() {
            return Err("fresh ordinary widget batch cancelled after worker".into());
        }
        cx.update(|app| -> Result<(), String> {
            let retained = owner.recovery_owner()?;
            let retained = retained.borrow();
            retained.interrupted_exit_services_result(request)?;
            let drafts = retained.recovery_drafts()?;
            let mut slot = slot.borrow_mut();
            let Some(CandidateSettlement::Services(Ok(graph))) = slot.as_mut() else {
                return Err("fresh ordinary widget reply graph is unavailable".into());
            };
            let mut reply = graph.claim_widget_reply(window_id)?.take();
            let delivered = window
                .update(app, |root, native, cx| {
                    let original = reply
                        .take()
                        .ok_or("fresh ordinary widget reply is missing")?;
                    match drafts
                        .borrow()
                        .accept_claim_widget_reply(root, home, generation, native, original, cx)
                    {
                        Ok(()) => Ok(()),
                        Err((original, error)) => {
                            reply = Some(original);
                            Err(error)
                        }
                    }
                })
                .map_err(|error| error.to_string())
                .and_then(|result| result);
            *graph.claim_widget_reply(window_id)? = reply;
            delivered
        })
        .map_err(|error| error.to_string())?
    }
}
