use super::*;
use crate::cas_projection::RuntimeActivityObservation;

#[derive(Clone)]
pub(crate) struct ActivityInteractionGate {
    selection: ActivityWindowSelection,
    observation: HomeMutationObservation,
    runtime: Option<RuntimeActivityObservation>,
}

impl ActivityInteractionGate {
    pub(in crate::activity_service::window) fn with_runtime_current<T>(
        &self,
        publish: impl FnOnce() -> T,
    ) -> Result<T, ActivityReadError> {
        if !self.selection.current() {
            return Err(ActivityReadError::Selection);
        }
        match &self.runtime {
            Some(runtime) => Ok(runtime.with_current(publish)?),
            None => Ok(publish()),
        }
    }

    pub(in crate::activity_service) fn new(
        selection: ActivityWindowSelection,
        observation: HomeMutationObservation,
        runtime: Option<RuntimeActivityObservation>,
    ) -> Self {
        Self {
            selection,
            observation,
            runtime,
        }
    }

    pub(crate) fn with_current<T>(
        &self,
        publish: impl FnOnce() -> T,
    ) -> Result<T, ActivityReadError> {
        if !self.selection.current() {
            return Err(ActivityReadError::Selection);
        }
        let publish = || self.selection.publish_observed(&self.observation, publish);
        match &self.runtime {
            Some(runtime) => runtime.with_current(publish)?,
            None => publish(),
        }
    }
}
