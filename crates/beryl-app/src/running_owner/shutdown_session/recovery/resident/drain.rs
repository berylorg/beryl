use super::*;

impl RunningProcessOwner {
    pub(in crate::running_owner) fn failed_resident_preparation(
        &self,
        key: &ResidentPreparationKey,
    ) -> Result<bool, String> {
        let flight = self
            .interrupted_exit
            .as_ref()
            .and_then(|recovery| recovery.resident.as_ref())
            .ok_or("failed resident preparation is unavailable")?;
        if !Rc::ptr_eq(&flight.key.0, &key.0) || !self.active_recovery_identity(&flight.request) {
            return Err("failed resident preparation identity changed".into());
        }
        Ok(matches!(flight.preparation, Preparation::Failed(_)))
    }

    pub(in crate::running_owner) async fn cancel_and_drain_failed_resident(
        owner: &impl RecoveryOwnerAccess,
        preparation: &mut Option<ResidentPreparationKey>,
        cx: &mut gpui::AsyncApp,
    ) -> Result<(), String> {
        let key = preparation
            .as_ref()
            .ok_or("failed resident preparation key is unavailable")?;
        let retained_owner = owner.recovery_owner()?;
        let (window, graph, source, capture) = loop {
            let returned = cx
                .update(|app| {
                    {
                        let retained = retained_owner.borrow();
                        if !retained.failed_resident_preparation(key)? {
                            return Err("failed resident preparation type changed".to_string());
                        }
                        let recovery = retained.interrupted_exit.as_ref().unwrap();
                        if !matches!(
                            recovery.settlement.borrow().as_ref(),
                            Some(CandidateSettlement::Pending)
                        ) {
                            return Err("failed resident candidate custody changed".into());
                        }
                        if recovery.resident.as_ref().unwrap().cleanup_failed {
                            return Err(recovery
                                .resident
                                .as_ref()
                                .unwrap()
                                .result
                                .as_ref()
                                .unwrap_err()
                                .clone());
                        }
                    }
                    Self::cancel_interrupted_exit_resident(&retained_owner, key, app)?;
                    let mut retained = retained_owner.borrow_mut();
                    let flight = retained
                        .interrupted_exit
                        .as_mut()
                        .unwrap()
                        .resident
                        .as_mut()
                        .unwrap();
                    let Some(ReturnedPreparation::Failed(_)) = flight.returned.as_ref() else {
                        return Ok(None);
                    };
                    let window = flight
                        .window
                        .downcast::<crate::main_window::MainWindowShellRoot>()
                        .ok_or("failed resident shell type changed")?;
                    let ReturnedPreparation::Failed(resources) = flight.returned.take().unwrap()
                    else {
                        unreachable!()
                    };
                    let (graph, source, capture) = *resources;
                    Ok(Some((window, graph, source, capture)))
                })
                .map_err(|error| error.to_string())??;
            if let Some(returned) = returned {
                break returned;
            }
            cx.background_executor()
                .timer(std::time::Duration::from_millis(50))
                .await;
        };
        let cleanup_executor = cx.background_executor().clone();
        let (graph, failure) = cx
            .background_executor()
            .spawn(async move {
                let mut graph = graph;
                let retired = match source {
                    Ok(source) => match graph
                        .dispose_cancelled_failed_resident_source(source, cleanup_executor)
                        .await
                    {
                        Ok(retired) => retired,
                        Err((source, error)) => return (graph, Some((source, error))),
                    },
                    Err((retired, _)) => retired,
                };
                graph.return_failed_resident_source(retired);
                (graph, None)
            })
            .await;
        cx.update(|_| {
            let mut retained = retained_owner.borrow_mut();
            assert!(retained.failed_resident_preparation(key).unwrap());
            if let Some((source, error)) = failure {
                let flight = retained
                    .interrupted_exit
                    .as_mut()
                    .unwrap()
                    .resident
                    .as_mut()
                    .unwrap();
                flight.returned = Some(ReturnedPreparation::Failed(Box::new((
                    graph,
                    Ok(source),
                    capture,
                ))));
                flight.cleanup_failed = true;
                flight.result = Err(error.clone());
                return Err(error);
            }
            retained
                .recovery_drafts()
                .unwrap()
                .borrow_mut()
                .return_failed_capture(window, capture);
            let recovery = retained.interrupted_exit.as_mut().unwrap();
            assert!(matches!(
                recovery.settlement.borrow().as_ref(),
                Some(CandidateSettlement::Pending)
            ));
            *recovery.settlement.borrow_mut() = Some(CandidateSettlement::Services(Ok(graph)));
            recovery.resident.take();
            Ok(())
        })
        .map_err(|error| error.to_string())??;
        preparation.take();
        Ok(())
    }

    pub(crate) async fn cancel_and_drain_interrupted_exit_resident(
        owner: &impl RecoveryOwnerAccess,
        preparation: &mut Option<ResidentPreparationKey>,
        cx: &mut gpui::AsyncApp,
    ) -> Result<
        Result<MainWindowComposerCandidateSource, (MainWindowComposerRetiredClose, String)>,
        String,
    > {
        let key = preparation.as_ref().ok_or("No resident preparation key")?;
        loop {
            let returned = cx
                .update(|app| {
                    {
                        let retained_owner = owner.recovery_owner()?;
                        let retained = retained_owner.borrow();
                        let flight = retained
                            .interrupted_exit
                            .as_ref()
                            .and_then(|recovery| recovery.resident.as_ref())
                            .ok_or("No resident preparation")?;
                        if !Rc::ptr_eq(&flight.key.0, &key.0) {
                            return Err("Resident preparation changed".into());
                        }
                        if flight.cleanup_failed {
                            return Err(flight.result.as_ref().unwrap_err().clone());
                        }
                        if matches!(flight.returned, Some(ReturnedPreparation::Failed(_))) {
                            return Err("failed resident cancellation retains exact source and protected editor custody".into());
                        }
                    }
                    Self::cancel_interrupted_exit_resident(&owner.recovery_owner()?, key, app)?;
                    Ok(owner
                        .recovery_owner()?
                        .borrow_mut()
                        .take_cancelled_resident_preparation(key))
                })
                .map_err(|error| error.to_string())??;
            if let Some(returned) = returned {
                preparation.take();
                return Ok(returned);
            }
            cx.background_executor()
                .timer(std::time::Duration::from_millis(50))
                .await;
        }
    }

    #[cfg(test)]
    pub(crate) fn test_set_resident_cleanup_failure(&mut self, failed: bool) {
        let flight = self
            .interrupted_exit
            .as_mut()
            .unwrap()
            .resident
            .as_mut()
            .unwrap();
        assert!(flight.cancelled && flight.returned.is_none());
        flight.cleanup_failed = failed;
        if failed {
            flight.result = Err("injected resident cleanup failure".into());
        }
    }
}
