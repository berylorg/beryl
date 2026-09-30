use super::*;
use crate::theme_runtime::ThemeRuntime;

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
    pub(crate) fn prepared_appearance(
        &self,
    ) -> Option<Arc<crate::theme_runtime::AppearanceGeneration>> {
        self.theme.as_ref().map(|theme| theme.current())
    }

    pub(crate) fn restored_window_attempt(
        &self,
    ) -> Result<crate::main_window::RestoredWindowPreparationAttempt, String> {
        if self.shutdown.is_some() {
            return Err("service graph is shutting down".to_owned());
        }
        crate::main_window::RestoredWindowPreparationAttempt::new(
            Arc::new(self.home().service_reference()),
            self.state.session(),
            self.syndic.clone(),
            Arc::downgrade(
                self.restore_lifetime
                    .as_ref()
                    .ok_or_else(|| "restore service generation is retired".to_owned())?,
            ),
        )
    }

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

    pub(crate) fn release_theme(&mut self) -> Result<(), AppThemeLoadError> {
        let prepared = self
            .theme
            .take()
            .ok_or(AppThemeLoadError::AlreadyAttempted)?;
        self.loaded_theme = Some(
            prepared
                .release_published(self.home())
                .map_err(AppThemeLoadError::Start)?,
        );
        Ok(())
    }
}
