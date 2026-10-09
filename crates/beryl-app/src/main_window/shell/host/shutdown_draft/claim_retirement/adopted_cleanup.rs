use super::*;

pub(super) fn advance_group(
    capsules: Option<&mut Vec<crate::main_window::MainWindowRetiredPrepublicationCleanup>>,
    window: beryl_model::WindowId,
) -> Result<bool, String> {
    let Some(capsules) = capsules else {
        return Ok(true);
    };
    if capsules
        .iter()
        .any(|capsule| capsule.selection().window_id() != window)
    {
        return Err("retained prepublication cleanup window changed".into());
    }
    let mut drained = true;
    for capsule in capsules {
        drained &= capsule.advance(64)?;
    }
    Ok(drained)
}

impl MainWindowShutdownDraft {
    pub(in crate::main_window::shell::host) fn prepare_adopted_cleanup_return(
        &mut self,
        window: beryl_model::WindowId,
    ) -> Result<bool, String> {
        let failed = self
            .failed
            .as_mut()
            .ok_or("adopted cleanup original resident is unavailable")?;
        let current = failed.prepublication_cleanup.get_mut();
        if !advance_group(current.as_mut(), window)? {
            return Ok(false);
        }
        current.take();
        Ok(true)
    }

    pub(crate) fn validate_adopted_cleanup_return(
        &self,
        capture: &crate::main_window::MainWindowFailedResidentCapture,
        capsules: &[crate::main_window::MainWindowRetiredPrepublicationCleanup],
    ) -> Result<(), String> {
        let failed = self
            .failed
            .as_ref()
            .ok_or("adopted cleanup original resident is unavailable")?;
        if !failed.retired
            || failed.capture.is_some()
            || failed.adoption.is_some()
            || failed.resources.is_some()
            || capture.ticket() != failed.ticket
            || capsules
                .iter()
                .any(|capsule| capsule.selection() != capture.native_selection())
            || failed
                .prepublication_cleanup
                .try_borrow()
                .map_err(|_| "adopted cleanup original group is busy")?
                .is_some()
        {
            return Err("adopted cleanup source/capture/group identity changed".into());
        }
        Ok(())
    }
}
