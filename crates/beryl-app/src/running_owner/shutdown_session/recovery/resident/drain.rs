use super::*;

impl RunningProcessOwner {
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
