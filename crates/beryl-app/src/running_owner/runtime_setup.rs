use super::*;

impl RunningProcessOwner {
    pub(crate) fn runtime_setup_services(
        &self,
    ) -> Option<crate::app_services::runtime_setup::PublishedRuntimeSetupServices> {
        if self.exit_requested() || self.shutdown.is_some() || self.interrupted_exit.is_some() {
            return None;
        }
        self.process.services.as_ref()?.runtime_setup_services()
    }

    pub(crate) fn runtime_setup_members(
        &self,
        app: &App,
    ) -> Result<Vec<beryl_model::WindowId>, String> {
        let members = self
            .process
            .windows
            .shells()
            .iter()
            .filter(|shell| shell.is_published())
            .map(|shell| shell.retained_window_id(app))
            .collect::<Result<Vec<_>, _>>()?;
        if members.is_empty()
            || members.len() > beryl_state::MAX_RESTORABLE_WINDOWS
            || members
                .iter()
                .enumerate()
                .any(|(index, member)| members[..index].contains(member))
        {
            return Err("runtime setup published window membership is invalid".into());
        }
        Ok(members)
    }
}
