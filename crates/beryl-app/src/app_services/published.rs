use super::*;
use crate::theme_runtime::ThemeRuntime;
use beryl_model::DomainRevision;
use beryl_state::SettingRecord;

#[derive(Debug)]
pub(crate) enum AppThemeLoadError {
    AlreadyAttempted,
    Start(ThemeRuntimeStartError),
}

impl ProcessServiceOwner {
    pub(crate) fn graph_mut(&mut self) -> Option<&mut PublishedAppServices> {
        self.graph.as_mut()
    }
}

impl PublishedAppServices {
    pub(crate) fn home(&self) -> &HomeStore {
        self.home.as_ref().expect("published home")
    }
    pub(crate) fn state(&self) -> &BerylState {
        &self.state
    }
    pub(crate) fn syndic(&self) -> &SyndicStorage {
        &self.syndic
    }
    pub(crate) fn cas(&self) -> &ProjectionConnectionService {
        self.cas.as_ref().expect("published CAS")
    }
    pub(crate) fn sessions(&self) -> &ScheduledExecutionSessions {
        &self.sessions
    }
    pub(crate) fn attention(&self) -> &Arc<ProcessLifecycleAttentionPool> {
        &self.attention
    }
    pub(crate) fn marker(&self) -> DraftMarkerSealService {
        self.marker.as_ref().expect("published marker").clone()
    }
    pub(crate) fn activity(&self) -> &ActivityService {
        self.activity.as_ref().expect("published Activity")
    }
    pub(crate) fn theme(&mut self) -> Option<&mut ThemeRuntime> {
        self.loaded_theme.as_mut()
    }

    pub(crate) fn load_theme(
        &mut self,
        revision: DomainRevision,
        active: Option<&SettingRecord>,
    ) -> Result<(), AppThemeLoadError> {
        let prepared = self
            .theme
            .take()
            .ok_or(AppThemeLoadError::AlreadyAttempted)?;
        self.loaded_theme = Some(
            prepared
                .load_published(self.home(), revision, active)
                .map_err(AppThemeLoadError::Start)?,
        );
        Ok(())
    }
}
