use beryl_backend::ModelInfo;
use gpui::{Bounds, Pixels, Point};

#[derive(Clone, Debug, Default)]
pub(crate) struct StatusLineOperationState {
    open: Option<StatusLineOperationOpen>,
}

#[derive(Clone, Debug)]
pub(crate) struct StatusLineOperationOpen {
    kind: StatusLineOperationKind,
    position: Point<Pixels>,
    bounds: Option<Bounds<Pixels>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StatusLineOperationKind {
    ModelReasoning,
    Context,
    TurnOperations,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct StatusModelListCache {
    models: Option<Vec<ModelInfo>>,
    loading: bool,
    last_error: Option<String>,
}

impl StatusLineOperationState {
    pub(crate) fn is_open(&self) -> bool {
        self.open.is_some()
    }

    pub(crate) fn active(&self) -> Option<&StatusLineOperationOpen> {
        self.open.as_ref()
    }

    pub(crate) fn open(&mut self, kind: StatusLineOperationKind, position: Point<Pixels>) {
        self.open = Some(StatusLineOperationOpen {
            kind,
            position,
            bounds: None,
        });
    }

    pub(crate) fn close(&mut self) {
        self.open = None;
    }

    pub(crate) fn set_bounds(&mut self, bounds: Option<Bounds<Pixels>>) {
        if let Some(open) = self.open.as_mut() {
            open.bounds = bounds;
        }
    }

    pub(crate) fn should_dismiss_for_mouse_down(&self, position: Point<Pixels>) -> bool {
        self.open
            .as_ref()
            .is_some_and(|open| !open.bounds.is_some_and(|bounds| bounds.contains(&position)))
    }

}

impl StatusLineOperationOpen {
    pub(crate) fn kind(&self) -> StatusLineOperationKind {
        self.kind
    }

    pub(crate) fn position(&self) -> Point<Pixels> {
        self.position
    }
}

impl StatusModelListCache {
    pub(crate) fn models(&self) -> Option<&[ModelInfo]> {
        self.models.as_deref()
    }

    pub(crate) fn loading(&self) -> bool {
        self.loading
    }

    pub(crate) fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    pub(crate) fn begin_loading(&mut self) {
        self.models = None;
        self.loading = true;
        self.last_error = None;
    }

    pub(crate) fn finish_loaded(&mut self, models: Vec<ModelInfo>) {
        self.models = Some(models);
        self.loading = false;
        self.last_error = None;
    }

    pub(crate) fn finish_failed(&mut self, message: String) {
        self.loading = false;
        self.last_error = Some(message);
    }

    pub(crate) fn should_load(&self) -> bool {
        !self.loading && self.models.is_none()
    }

    pub(crate) fn find_model(&self, value: &str) -> Option<&ModelInfo> {
        self.models()?
            .iter()
            .find(|model| model.model == value || model.id == value || model.display_name == value)
    }
}

pub(crate) fn reasoning_effort_for_model_selection(
    model: &ModelInfo,
    current_reasoning_effort: Option<&str>,
) -> Option<String> {
    if model.supported_reasoning_efforts.is_empty() {
        return None;
    }

    if let Some(current) = current_reasoning_effort
        && model
            .supported_reasoning_efforts
            .iter()
            .any(|effort| effort == current)
    {
        return Some(current.to_string());
    }

    model
        .default_reasoning_effort
        .as_deref()
        .filter(|default| {
            model
                .supported_reasoning_efforts
                .iter()
                .any(|effort| effort == default)
        })
        .map(str::to_string)
        .or_else(|| model.supported_reasoning_efforts.first().cloned())
}
