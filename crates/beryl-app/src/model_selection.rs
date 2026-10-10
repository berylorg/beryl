pub(crate) mod menu;
mod reader;
#[cfg(all(test, feature = "test-faults"))]
pub(crate) use reader::qualification_support as test_support;
pub(crate) use reader::*;

use std::sync::{Arc, Weak};

use crate::cas_projection::WeakScheduledExecutionSessions;
use crate::main_window::MainWindowComposerSelectionIdentity;

#[derive(Clone)]
pub(crate) struct PublishedModelSelection {
    pub(crate) reader: PublishedModelReader,
    pub(crate) sessions: WeakScheduledExecutionSessions,
    pub(crate) lifetime: Weak<()>,
}

pub(crate) struct SelectedModelScope {
    pub(crate) selection: MainWindowComposerSelectionIdentity,
    pub(crate) query: Arc<ModelQuery>,
    pub(crate) execution: beryl_model::ExecutionBinding,
    pub(crate) draft_only: bool,
    pub(crate) values: ModelDefaults,
    sessions: WeakScheduledExecutionSessions,
    choice_revision: Option<u64>,
    choice_epoch: u64,
}

impl SelectedModelScope {
    pub(crate) fn with_current_status<T>(
        &self,
        publish: impl FnOnce() -> T,
    ) -> Result<T, ModelReadError> {
        self.query.with_current_status(|| {
            self.sessions
                .upgrade()
                .ok_or(ModelReadError::Unavailable)?
                .with_current_model_choice_elected(
                    self.selection.claim().thread_id(),
                    &self.execution,
                    self.choice_revision,
                    self.choice_epoch,
                    publish,
                )
                .map_err(|_| ModelReadError::Busy)
        })?
    }
}

impl PublishedModelSelection {
    pub(crate) fn current(&self) -> bool {
        self.lifetime.upgrade().is_some()
    }

    pub(crate) fn prepare(
        &self,
        selection: MainWindowComposerSelectionIdentity,
    ) -> Result<SelectedModelScope, ModelReadError> {
        self.prepare_scoped(selection, false, None)
    }

    pub(crate) fn preparation_fence(
        &self,
        selection: MainWindowComposerSelectionIdentity,
    ) -> Result<Arc<ModelPreparationFence>, ModelReadError> {
        if !self.current()
            || selection.binding().home_id() != self.reader.identity().0
            || selection.binding().home_generation() != self.reader.identity().1
        {
            return Err(ModelReadError::Unavailable);
        }
        self.reader
            .preparation_fence(selection.window_id(), selection.claim())
    }

    pub(crate) fn prepare_fenced(
        &self,
        selection: MainWindowComposerSelectionIdentity,
        fence: &Arc<ModelPreparationFence>,
    ) -> Result<SelectedModelScope, ModelReadError> {
        self.prepare_scoped(selection, false, Some(fence))
    }

    pub(crate) fn observe(
        &self,
        selection: MainWindowComposerSelectionIdentity,
    ) -> Result<SelectedModelScope, ModelReadError> {
        self.prepare_scoped(selection, true, None)
    }

    fn prepare_scoped(
        &self,
        selection: MainWindowComposerSelectionIdentity,
        status: bool,
        fence: Option<&Arc<ModelPreparationFence>>,
    ) -> Result<SelectedModelScope, ModelReadError> {
        if !self.current() {
            return Err(ModelReadError::Unavailable);
        }
        let identity = self.reader.identity();
        if selection.binding().home_id() != identity.0
            || selection.binding().home_generation() != identity.1
        {
            return Err(ModelReadError::Unavailable);
        }
        let (query, execution, draft_only) = if let Some(fence) = fence {
            if !fence.matches_selection(selection.window_id(), selection.claim()) {
                return Err(ModelReadError::Unavailable);
            }
            self.reader.prepare_fenced(fence)?
        } else if status {
            self.reader
                .prepare_selected_status(selection.window_id(), selection.claim())?
        } else {
            self.reader
                .prepare_selected(selection.window_id(), selection.claim())?
        };
        let sessions = self.sessions.upgrade().ok_or(ModelReadError::Unavailable)?;
        let (pending, choice_epoch) = sessions
            .model_choice_snapshot(selection.claim().thread_id(), &execution)
            .ok_or(ModelReadError::Unavailable)?;
        let choice_revision = pending.as_ref().map(|pending| pending.revision());
        let values = if let Some(pending) = pending {
            ModelDefaults {
                model: Some(pending.choice.model.as_str().to_owned()),
                reasoning: pending
                    .choice
                    .reasoning
                    .map(|effort| crate::cas_projection::reasoning_wire(effort).to_owned()),
            }
        } else if draft_only {
            query.read_defaults()?
        } else {
            let metadata =
                sessions.observed_model_metadata(selection.claim().thread_id(), &execution);
            ModelDefaults {
                model: metadata
                    .as_ref()
                    .and_then(|metadata| metadata.model.clone()),
                reasoning: metadata.and_then(|metadata| metadata.reasoning_effort),
            }
        };
        if status {
            query.with_current_status(|| ())?;
        } else {
            query.with_current(|| ())?;
        }
        let scope = SelectedModelScope {
            selection,
            query,
            execution,
            draft_only,
            values,
            sessions: self.sessions.clone(),
            choice_revision,
            choice_epoch,
        };
        scope.with_current_status(|| ())?;
        Ok(scope)
    }

    pub(crate) fn choose(
        &self,
        scope: &SelectedModelScope,
        selection: MainWindowComposerSelectionIdentity,
        page: &ModelPage,
        option: &ModelOptionRecord,
        reasoning: Option<ModelReasoningEffort>,
    ) -> Result<(), ModelReadError> {
        if !self.current() || selection != scope.selection || !page.belongs_to(&scope.query) {
            return Err(ModelReadError::Unavailable);
        }
        let sessions = self.sessions.upgrade().ok_or(ModelReadError::Unavailable)?;
        let model = beryl_backend::ProtocolIdentity::try_new(&option.model)
            .map_err(|_| ModelReadError::Unavailable)?;
        page.with_current(|records| {
            if !records.iter().any(|current| current == option)
                || reasoning.is_some_and(|effort| !option.efforts.contains(effort))
            {
                return Err(ModelReadError::Unavailable);
            }
            sessions
                .select_next_turn_model_elected(
                    selection.claim().thread_id(),
                    &scope.execution,
                    crate::cas_projection::ThreadModelChoice {
                        model,
                        reasoning: reasoning.map(ModelReasoningEffort::backend),
                    },
                )
                .map_err(|_| ModelReadError::Unavailable)
        })?
    }
}

impl Drop for SelectedModelScope {
    fn drop(&mut self) {
        self.query.close();
    }
}
