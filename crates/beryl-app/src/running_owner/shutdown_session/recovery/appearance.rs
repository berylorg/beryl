use super::*;
use crate::theme_runtime::{AppearancePublicationTarget, GpuiAppearanceWindowSet};

impl RunningProcessOwner {
    pub(crate) fn bind_interrupted_exit_process(
        &mut self,
        request: &impl RecoveryIdentity,
        appearance: &gpui::Entity<GpuiAppearanceWindowSet>,
        app: &mut App,
    ) -> Result<(), String> {
        self.interrupted_exit_publication_result(request)?;
        self.validate_interrupted_exit_bindings(request, appearance, app)?;
        self.bind_recovered_process_commands(request, app)?;
        self.process.appearance = appearance.clone();
        Ok(())
    }

    pub(crate) fn interrupted_exit_appearance(
        &self,
        request: &impl RecoveryIdentity,
    ) -> Result<std::sync::Arc<crate::theme_runtime::AppearanceGeneration>, String> {
        self.interrupted_exit_services_result(request)?;
        let settlement = self.interrupted_exit.as_ref().unwrap().settlement.borrow();
        let Some(settlement::CandidateSettlement::Services(Ok(graph))) = settlement.as_ref() else {
            unreachable!("validated prepared recovery services")
        };
        Ok(graph.appearance())
    }

    pub(crate) fn release_interrupted_exit_drafts(
        &self,
        request: &impl RecoveryIdentity,
        appearance: &gpui::Entity<GpuiAppearanceWindowSet>,
        app: &mut App,
    ) -> Result<bool, String> {
        self.validate_interrupted_exit_bindings(request, appearance, app)?;
        self.recovery_drafts()?
            .try_borrow()
            .map_err(|_| "Interrupted Exit drafts are busy")?
            .release_recovered_drafts(&self.process.windows, appearance, app)
    }

    pub(crate) fn release_interrupted_exit_mounts(
        &self,
        request: &impl RecoveryIdentity,
        appearance: &gpui::Entity<GpuiAppearanceWindowSet>,
        app: &mut App,
    ) -> Result<bool, String> {
        self.interrupted_exit_theme_activation_result(request)?;
        self.validate_interrupted_exit_bindings(request, appearance, app)?;
        if self.process.appearance != *appearance {
            return Err("Interrupted Exit process bindings are not installed".into());
        }
        self.recovery_drafts()?
            .try_borrow()
            .map_err(|_| "Interrupted Exit drafts are busy")?
            .release_recovered_mounts(&self.process.windows, appearance, app)
    }

    pub(crate) fn validate_interrupted_exit_bindings(
        &self,
        request: &impl RecoveryIdentity,
        appearance: &gpui::Entity<GpuiAppearanceWindowSet>,
        app: &mut App,
    ) -> Result<(), String> {
        self.interrupted_exit_graph_retirement_result(request)?;
        let recovery = self.interrupted_exit.as_ref().unwrap();
        if recovery.resident.is_some()
            || recovery.session.borrow().is_none()
            || recovery
                .pending_resident_frame
                .as_ref()
                .is_some_and(|wake| wake.strong_count() != 0)
        {
            return Err("Interrupted Exit recovery custody is unavailable".into());
        }
        let mut settlement = recovery.settlement.borrow_mut();
        let snapshot = appearance.read(app).target().snapshot();
        let home = snapshot.current.prepared().home();
        if !snapshot.active {
            return Err("Recovery appearance candidate identity changed".into());
        }
        let expected = match settlement.as_mut() {
            Some(settlement::CandidateSettlement::Services(Ok(graph))) => {
                if !graph.matches_candidate(home.home_id(), home.home_generation()) {
                    return Err("Recovery appearance candidate identity changed".into());
                }
                graph.appearance()
            }
            Some(settlement::CandidateSettlement::Published) => {
                self.interrupted_exit_publication_result(request)?;
                self.process
                    .services
                    .as_ref()
                    .and_then(|services| services.graph())
                    .and_then(|graph| graph.current_appearance())
                    .ok_or("Published recovery graph appearance is unavailable")?
            }
            _ => return Err("Interrupted Exit recovery graph is unavailable".into()),
        };
        if !std::sync::Arc::ptr_eq(&snapshot.current, &expected) {
            return Err("Recovery appearance differs from the prepared graph".into());
        }
        drop(settlement);
        let drafts = self.recovery_drafts()?;
        drafts
            .try_borrow()
            .map_err(|_| "Interrupted Exit drafts are busy")?
            .validate_recovered_bindings(&self.process.windows, appearance, app)
    }

    #[cfg(test)]
    pub(crate) fn test_process_appearance(&self) -> gpui::Entity<GpuiAppearanceWindowSet> {
        self.process.appearance.clone()
    }

    pub(crate) fn bind_interrupted_exit_appearance(
        &mut self,
        request: &impl RecoveryIdentity,
        window: gpui::WindowHandle<crate::main_window::MainWindowShellRoot>,
        appearance: &gpui::Entity<GpuiAppearanceWindowSet>,
        app: &mut App,
    ) -> Result<(), String> {
        self.interrupted_exit_services_result(request)?;
        let recovery = self.interrupted_exit.as_ref().unwrap();
        if recovery.resident.is_some()
            || recovery.session.borrow().is_none()
            || recovery
                .pending_resident_frame
                .as_ref()
                .is_some_and(|wake| wake.strong_count() != 0)
        {
            return Err("Interrupted Exit recovery custody is unavailable".into());
        }
        let snapshot = appearance.read(app).target().snapshot();
        let home = snapshot.current.prepared().home();
        let mut settlement = recovery.settlement.borrow_mut();
        let Some(settlement::CandidateSettlement::Services(Ok(graph))) = settlement.as_mut() else {
            unreachable!("validated prepared recovery services")
        };
        if !snapshot.active || !graph.matches_candidate(home.home_id(), home.home_generation()) {
            return Err("Recovery appearance candidate identity changed".into());
        }
        if !std::sync::Arc::ptr_eq(&snapshot.current, &graph.appearance()) {
            return Err("Recovery appearance differs from the prepared graph".into());
        }
        drop(settlement);
        let drafts = self.recovery_drafts()?;
        let drafts = drafts
            .try_borrow_mut()
            .map_err(|_| "Interrupted Exit drafts are busy")?;
        drafts.require_recovery_window(window)?;
        self.process
            .windows
            .bind_interrupted_exit_appearance(window, appearance, app)
    }
}
