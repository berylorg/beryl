use super::*;
use crate::widgets::anchored_context_menu::{
    MenuCollection, MenuEvent, MenuIntent, MenuRow, MenuRowKind,
};

pub(super) struct ModelMenuRows<'a> {
    collection: &'a ModelMenuCollection,
    values: &'a ModelDefaults,
    pending: bool,
    feedback: Option<&'static str>,
    efforts: Vec<ModelReasoningEffort>,
}

impl<'a> ModelMenuRows<'a> {
    pub(super) fn new(
        collection: &'a ModelMenuCollection,
        values: &'a ModelDefaults,
        pending: bool,
        feedback: Option<&'static str>,
        efforts: Vec<ModelReasoningEffort>,
    ) -> Self {
        Self {
            collection,
            values,
            pending,
            feedback,
            efforts,
        }
    }
    fn models(&self) -> usize {
        self.collection.observed + usize::from(!self.collection.complete && self.feedback.is_none())
    }
    fn reasoning_start(&self) -> usize {
        self.models() + usize::from(!self.efforts.is_empty())
    }
    fn effort(&self, index: usize) -> Option<ModelReasoningEffort> {
        index
            .checked_sub(self.reasoning_start())
            .and_then(|index| self.efforts.get(index))
            .copied()
    }
    fn reasoning_id(&self, effort: ModelReasoningEffort) -> Option<Arc<str>> {
        self.collection.selected.as_ref().map(|(_, record)| {
            Arc::from(format!(
                "model-reasoning-{}:{}:{}",
                record.id.len(),
                record.id,
                effort.as_str()
            ))
        })
    }
    fn retry(&self, index: usize) -> bool {
        self.feedback.is_some() && index + 1 == self.len()
    }
    fn header(&self, index: usize, label: &'static str, pending: bool) -> MenuRow {
        MenuRow {
            index,
            id: None,
            label: label.into(),
            kind: if pending {
                MenuRowKind::PendingSelection
            } else {
                MenuRowKind::Header
            },
            selected: false,
            enabled: false,
            disabled_reason: None,
            selector: "model-menu-header".into(),
        }
    }
}

impl MenuCollection for ModelMenuRows<'_> {
    fn len(&self) -> usize {
        (self.models()
            + self.efforts.len()
            + usize::from(!self.efforts.is_empty())
            + usize::from(self.feedback.is_some()) * 2)
            .max(1)
    }
    fn row(&self, index: usize) -> MenuRow {
        let models = self.models();
        if index < models {
            let record = self
                .collection
                .row(index)
                .map(|(_, row)| row)
                .or_else(|| {
                    self.collection
                        .selected
                        .as_ref()
                        .filter(|(pin, _)| *pin == index)
                        .map(|(_, row)| row)
                })
                .or_else(|| {
                    self.collection
                        .focused
                        .as_ref()
                        .filter(|(pin, _)| *pin == index)
                        .map(|(_, row)| row)
                });
            let Some(record) = record else {
                return self.header(index, "Loading models…", true);
            };
            let presentation = self.collection.presentation(index);
            let id = presentation
                .map(|row| row.id.clone())
                .unwrap_or_else(|| Arc::from(format!("model-option-{}", record.id)));
            let label = presentation
                .map(|row| gpui::SharedString::new(row.label.clone()))
                .unwrap_or_else(|| record.label.replace(['\r', '\n'], " ").into());
            return MenuRow {
                index,
                id: Some(id),
                label,
                kind: MenuRowKind::Selection,
                selected: self.values.model.as_deref() == Some(record.model.as_str()),
                enabled: true,
                disabled_reason: None,
                selector: format!("model-option-{index}").into(),
            };
        }
        if !self.efforts.is_empty() && index == models {
            return self.header(index, "Reasoning", false);
        }
        if let Some(effort) = self.effort(index) {
            return MenuRow {
                index,
                id: self.reasoning_id(effort),
                label: effort.as_str().into(),
                kind: MenuRowKind::Selection,
                selected: self.values.reasoning.as_deref() == Some(effort.as_str()),
                enabled: true,
                disabled_reason: None,
                selector: format!("model-reasoning-{}", effort.as_str()).into(),
            };
        }
        if let Some(feedback) = self.feedback {
            if self.retry(index) {
                return MenuRow {
                    index,
                    id: Some(Arc::from("main-window-model-retry")),
                    label: "Retry".into(),
                    kind: MenuRowKind::Command,
                    selected: false,
                    enabled: !self.pending,
                    disabled_reason: self
                        .pending
                        .then(|| "The failed model query is being retried.".into()),
                    selector: "main-window-model-retry".into(),
                };
            }
            return self.header(index, feedback, false);
        }
        self.header(
            index,
            if self.collection.complete {
                "No supported models."
            } else {
                "Loading models…"
            },
            false,
        )
    }
    fn index_of(&self, id: &str) -> Option<usize> {
        for resident in &self.collection.pages {
            for (offset, record) in resident.page.records().iter().enumerate() {
                if id.strip_prefix("model-option-") == Some(record.id.as_str()) {
                    return Some(resident.start + offset);
                }
            }
        }
        for (index, record) in [&self.collection.selected, &self.collection.focused]
            .into_iter()
            .flatten()
        {
            if id.strip_prefix("model-option-") == Some(record.id.as_str()) {
                return Some(*index);
            }
        }
        for (offset, effort) in self.efforts.iter().enumerate() {
            if self.reasoning_id(*effort).as_deref() == Some(id) {
                return Some(self.reasoning_start() + offset);
            }
        }
        (self.feedback.is_some() && id == "main-window-model-retry").then(|| self.len() - 1)
    }
}

#[cfg(all(test, feature = "test-faults"))]
#[path = "../../../../../tests/unit/model_menu_rows.rs"]
mod tests;

pub(super) fn prepare_range(
    root: &mut MainWindowShellRoot,
    range: std::ops::Range<usize>,
    window: &mut Window,
    cx: &mut Context<MainWindowShellRoot>,
) -> Vec<MenuRow> {
    let Some(popup) = root.model_controls.popup.as_mut() else {
        return Vec::new();
    };
    popup.collection.range = range.clone();
    popup.collection.realized = range.len();
    let facts = ModelMenuRows::new(
        &popup.collection,
        &root.model_controls.values,
        popup.pending,
        popup.feedback,
        popup.efforts(),
    );
    let changed_focus = popup.widget.reconcile(&facts);
    let rows: Vec<_> = range.clone().map(|index| facts.row(index)).collect();
    let desired = rows
        .iter()
        .find(|row| row.kind == MenuRowKind::PendingSelection)
        .map(|row| row.index);
    if popup.widget.realize(&rows, range, window) || changed_focus {
        cx.notify();
    }
    if let Some(activation) = popup.widget.take_pending_activation(&facts) {
        cx.defer_in(window, move |root, window, cx| {
            event(root, activation, window, cx)
        });
    }
    if let Some(desired) = desired {
        popup.desired = desired;
        root.start_model_page(false, window, cx);
    }
    rows
}

pub(super) fn event(
    root: &mut MainWindowShellRoot,
    event: MenuEvent,
    window: &mut Window,
    cx: &mut Context<MainWindowShellRoot>,
) {
    let Some(popup) = root.model_controls.popup.as_mut() else {
        return;
    };
    let facts = ModelMenuRows::new(
        &popup.collection,
        &root.model_controls.values,
        popup.pending,
        popup.feedback,
        popup.efforts(),
    );
    let intents = popup.widget.input(event, &facts);
    for intent in intents {
        match intent {
            MenuIntent::Dismiss => {
                root.close_model_menu(window, cx);
                return;
            }
            MenuIntent::Focus(index) => {
                let Some(popup) = root.model_controls.popup.as_mut() else {
                    return;
                };
                if index < popup.model_row_count() {
                    popup.collection.reveal_focus(index);
                    popup.desired = index;
                    root.start_model_page(false, window, cx);
                }
            }
            MenuIntent::Activate { index, id } => {
                let Some(popup) = root.model_controls.popup.as_ref() else {
                    return;
                };
                let facts = ModelMenuRows::new(
                    &popup.collection,
                    &root.model_controls.values,
                    popup.pending,
                    popup.feedback,
                    popup.efforts(),
                );
                if facts.retry(index) {
                    root.retry_model_menu(window, cx);
                } else if let Some(effort) = facts.effort(index) {
                    if let Some((selected, record)) = popup.collection.selected.as_ref() {
                        let selected = *selected;
                        let record = record.id.clone();
                        root.activate_model_identity(selected, &record, Some(effort), window, cx);
                    }
                } else if let Some(record) = id.strip_prefix("model-option-") {
                    root.activate_model_identity(index, record, None, window, cx);
                }
            }
        }
    }
    cx.notify();
}
