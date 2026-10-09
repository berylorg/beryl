use super::*;

impl RunningShutdownDrafts {
    pub(in crate::running_owner) fn validate_adopted_cleanup_return(
        &self,
        window: WindowHandle<MainWindowShellRoot>,
        capture: &crate::main_window::MainWindowFailedResidentCapture,
        capsules: &[crate::main_window::MainWindowRetiredPrepublicationCleanup],
    ) -> Result<(), String> {
        if !self.prepared || self.driving || self.releasing || self.released {
            return Err("adopted cleanup original draft set is unavailable".into());
        }
        self.windows
            .iter()
            .find(|(handle, _)| *handle == window)
            .ok_or("adopted cleanup window is absent from original drafts")?
            .1
            .as_ref()
            .map_err(|error| error.clone())?
            .validate_adopted_cleanup_return(capture, capsules)
    }

    pub(in crate::running_owner) fn return_adopted_cleanup(
        &mut self,
        window: WindowHandle<MainWindowShellRoot>,
        capture: crate::main_window::MainWindowFailedResidentCapture,
        capsules: Vec<crate::main_window::MainWindowRetiredPrepublicationCleanup>,
    ) {
        self.return_failed_capture(window, capture);
        let failed = self
            .windows
            .iter_mut()
            .find(|(handle, _)| *handle == window)
            .unwrap()
            .1
            .as_mut()
            .unwrap()
            .failed
            .as_mut()
            .unwrap();
        assert!(failed.prepublication_cleanup.get_mut().is_none());
        *failed.prepublication_cleanup.get_mut() = Some(capsules);
    }

    pub(in crate::running_owner) fn return_adopted_failed_resident(
        &mut self,
        window: WindowHandle<MainWindowShellRoot>,
        home: beryl_model::BerylHomeId,
        generation: beryl_home_store::HomeGeneration,
        app: &mut App,
    ) -> Result<
        Option<Option<Box<crate::main_window::MainWindowFailedResidentAdoptionReturn>>>,
        String,
    > {
        if !self.prepared || self.driving || self.releasing || self.released {
            return Err("adopted return draft set is unavailable".into());
        }
        let draft = self
            .windows
            .iter_mut()
            .find(|(handle, _)| *handle == window)
            .ok_or("adopted return window is absent from original drafts")?
            .1
            .as_mut()
            .map_err(|error| error.clone())?;
        window
            .update(app, |root, _, cx| {
                root.return_adopted_failed_resident(draft, home, generation, cx)
            })
            .map_err(|error| error.to_string())?
    }
}
