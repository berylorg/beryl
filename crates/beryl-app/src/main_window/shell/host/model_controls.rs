use super::*;
use crate::main_window::MainWindowComposerSelectionIdentity;
use crate::model_selection::menu::ModelMenuCollection;
use crate::model_selection::{
    ModelDefaults, ModelOptionRecord, ModelReadError, ModelReasoningEffort,
    PublishedModelSelection, SelectedModelScope,
};
use gpui::prelude::FluentBuilder;
use gpui::{AnyElement, AnyView, InteractiveElement, StatefulInteractiveElement};
use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

mod jobs;
mod render;
mod rows;
pub(super) use render::menu_colors;
pub(super) use render::{render_menu, render_segment};

pub(super) struct ModelControls {
    reader: Option<PublishedModelSelection>,
    selection: Option<MainWindowComposerSelectionIdentity>,
    values: ModelDefaults,
    runtime_available: bool,
    generation: Arc<AtomicU64>,
    poll: Option<gpui::Task<()>>,
    popup: Option<ModelPopup>,
    serial: u64,
    anchor_bounds: Option<gpui::Bounds<gpui::Pixels>>,
}

struct ModelPopup {
    serial: u64,
    scope: Option<Arc<SelectedModelScope>>,
    preparation: Option<Arc<crate::model_selection::ModelPreparationFence>>,
    collection: ModelMenuCollection,
    widget: crate::widgets::anchored_context_menu::AnchoredContextMenu,
    pending: bool,
    waiting_election: bool,
    retrying: bool,
    failed_start: Option<usize>,
    failed_continuation: Option<crate::model_selection::ModelContinuation>,
    retry_page: bool,
    feedback: Option<&'static str>,
    desired: usize,
    job: Option<gpui::Task<()>>,
    activation: Option<(usize, ModelOptionRecord, Option<ModelReasoningEffort>, bool)>,
    pending_publication: Option<crate::model_selection::menu::ModelPageArrival>,
    selected_revealed: bool,
    locating_selected: bool,
    preserve_focus: bool,
    reconciliation_micros: u64,
}

#[derive(Clone, Debug)]
pub(crate) struct ModelMenuDiagnostics {
    pub(crate) total: usize,
    pub(crate) realized: usize,
    pub(crate) range: std::ops::Range<usize>,
    pub(crate) overscan: usize,
    pub(crate) row_height: f32,
    pub(crate) scroll_offset: gpui::Pixels,
    pub(crate) focused: bool,
    pub(crate) selected: bool,
    pub(crate) reconciliation_micros: u64,
}

impl ModelPopup {
    fn efforts(&self) -> Vec<ModelReasoningEffort> {
        self.collection
            .selected
            .as_ref()
            .map(|(_, row)| {
                ModelReasoningEffort::ALL
                    .into_iter()
                    .filter(|effort| row.efforts.contains(*effort))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn model_row_count(&self) -> usize {
        self.collection.observed + usize::from(!self.collection.complete && self.feedback.is_none())
    }

    fn row_count(&self) -> usize {
        let efforts = self.efforts();
        (self.model_row_count()
            + efforts.len()
            + usize::from(!efforts.is_empty())
            + usize::from(self.feedback.is_some()) * 2)
            .max(1)
    }
}

impl ModelControls {
    pub(super) fn new(_: &mut Context<MainWindowShellRoot>) -> Self {
        Self {
            reader: None,
            selection: None,
            values: ModelDefaults {
                model: None,
                reasoning: None,
            },
            runtime_available: false,
            generation: Arc::new(AtomicU64::new(1)),
            poll: None,
            popup: None,
            serial: 0,
            anchor_bounds: None,
        }
    }

    fn known(&self) -> bool {
        self.values.model.is_some() || self.values.reasoning.is_some()
    }

    fn text(&self) -> String {
        format!(
            "Model {} • Reasoning {}",
            self.values.model.as_deref().unwrap_or("Unknown"),
            self.values.reasoning.as_deref().unwrap_or("Unknown")
        )
    }
}

impl Drop for ModelControls {
    fn drop(&mut self) {
        self.generation.fetch_add(1, Ordering::AcqRel);
        if let Some(scope) = self.popup.as_ref().and_then(|popup| popup.scope.as_ref()) {
            scope.query.close();
        }
    }
}

impl MainWindowShellRoot {
    fn model_disabled_reason(&self) -> Option<&'static str> {
        if self.model_turn_active() {
            Some("Model changes are unavailable while the turn is active.")
        } else if !self.model_controls.runtime_available {
            Some("Runtime is unavailable.")
        } else {
            self.status_mutation_gate()
        }
    }

    pub(super) fn sync_model_controls(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self
            .model_controls
            .reader
            .as_ref()
            .is_none_or(|reader| !reader.current())
        {
            let reader = crate::running_owner::RunningProcessOwner::mounted_owner(cx)
                .and_then(|owner| owner.upgrade())
                .and_then(|owner| {
                    owner
                        .try_borrow()
                        .ok()
                        .and_then(|owner| owner.model_selection_reader())
                });
            if let Some(reader) = reader {
                self.close_model_menu(window, cx);
                self.model_controls.reader = Some(reader);
                self.model_controls.selection = None;
                self.model_controls.values = ModelDefaults {
                    model: None,
                    reasoning: None,
                };
                self.model_controls
                    .generation
                    .fetch_add(1, Ordering::AcqRel);
            }
        }
        let selection = self.status_selection(cx);
        if self.model_controls.selection != selection {
            self.close_model_menu(window, cx);
            self.model_controls.selection = selection;
            self.model_controls.values = ModelDefaults {
                model: None,
                reasoning: None,
            };
            self.model_controls.runtime_available = false;
            self.model_controls
                .generation
                .fetch_add(1, Ordering::AcqRel);
        }
        let popup_stale = self.model_controls.popup.as_ref().and_then(|popup| popup.scope.as_ref())
            .is_some_and(|scope| matches!(scope.query.with_current(|| ()), Err(error) if error.scope_retired()));
        let preparation_stale = self
            .model_controls
            .popup
            .as_ref()
            .and_then(|popup| popup.preparation.as_ref())
            .is_some_and(
                |fence| matches!(fence.with_current(|| ()), Err(error) if error.scope_retired()),
            );
        if self.model_controls.popup.is_some()
            && (selection.is_none()
                || !self.model_controls.known()
                || self.model_disabled_reason().is_some()
                || popup_stale
                || preparation_stale)
        {
            self.close_model_menu(window, cx);
        }
        self.start_model_poll(window, cx);
    }

    fn close_model_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(mut popup) = self.model_controls.popup.take() {
            if let Some(scope) = &popup.scope {
                scope.query.close();
            }
            popup.widget.dismiss(window, &self.shell_focus);
            cx.notify();
        }
    }

    fn toggle_model_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.model_controls.popup.is_some() {
            self.close_model_menu(window, cx);
            return;
        }
        self.sync_model_controls(window, cx);
        if !self.model_controls.known() || self.model_disabled_reason().is_some() {
            return;
        }
        let (Some(reader), Some(selection)) = (
            self.model_controls.reader.clone(),
            self.model_controls.selection,
        ) else {
            return;
        };
        self.model_controls.serial = self.model_controls.serial.wrapping_add(1);
        let serial = self.model_controls.serial;
        self.model_controls.popup = Some(ModelPopup {
            serial,
            scope: None,
            preparation: None,
            collection: ModelMenuCollection::new(),
            widget: crate::widgets::anchored_context_menu::AnchoredContextMenu::open(
                serial, window, cx,
            ),
            pending: true,
            waiting_election: false,
            retrying: false,
            failed_start: None,
            failed_continuation: None,
            retry_page: false,
            feedback: None,
            desired: 0,
            job: None,
            activation: None,
            pending_publication: None,
            selected_revealed: false,
            locating_selected: false,
            preserve_focus: false,
            reconciliation_micros: 0,
        });
        self.start_model_prepare(reader, selection, serial, window, cx);
        cx.notify();
    }

    fn retry_model_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.model_disabled_reason().is_some() {
            self.close_model_menu(window, cx);
            return;
        }
        let Some(popup) = self.model_controls.popup.as_ref() else {
            return;
        };
        if popup.pending || popup.feedback.is_none() {
            return;
        }
        if popup.scope.is_none() {
            let (Some(reader), Some(selection)) = (
                self.model_controls.reader.clone(),
                self.model_controls.selection,
            ) else {
                return;
            };
            let serial = popup.serial;
            self.model_controls.popup.as_mut().unwrap().pending = true;
            self.model_controls.popup.as_mut().unwrap().retrying = true;
            self.start_model_prepare(reader, selection, serial, window, cx);
        } else {
            self.start_model_page(true, window, cx);
        }
        cx.notify();
    }

    fn activate_model_row(
        &mut self,
        index: usize,
        effort: Option<ModelReasoningEffort>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.activate_model_row_with_focus(index, effort, effort.is_some(), window, cx);
    }

    fn activate_model_row_with_focus(
        &mut self,
        index: usize,
        effort: Option<ModelReasoningEffort>,
        reasoning_activation: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.model_disabled_reason().is_some() {
            self.close_model_menu(window, cx);
            return;
        }
        let Some(selection) = self.status_selection(cx) else {
            return;
        };
        let Some(reader) = self.model_controls.reader.clone() else {
            return;
        };
        let Some(popup) = self.model_controls.popup.as_mut() else {
            return;
        };
        let Some(scope) = popup.scope.clone() else {
            return;
        };
        let record = popup
            .collection
            .row(index)
            .map(|(_, row)| row.clone())
            .or_else(|| {
                popup
                    .collection
                    .selected
                    .as_ref()
                    .filter(|(selected, _)| *selected == index)
                    .map(|(_, row)| row.clone())
            })
            .or_else(|| {
                popup
                    .collection
                    .focused
                    .as_ref()
                    .filter(|(focused, _)| *focused == index)
                    .map(|(_, row)| row.clone())
            });
        let Some(record) = record else {
            return;
        };
        let effort = effort.or_else(|| {
            self.model_controls
                .values
                .reasoning
                .as_deref()
                .and_then(|value| {
                    ModelReasoningEffort::ALL
                        .into_iter()
                        .find(|effort| effort.as_str() == value && record.efforts.contains(*effort))
                })
        });
        if let Some((page, _)) = popup.collection.row(index) {
            if reader
                .choose(&scope, selection, page, &record, effort)
                .is_ok()
            {
                self.model_controls.values = ModelDefaults {
                    model: Some(record.model.clone()),
                    reasoning: effort.map(|effort| effort.as_str().to_owned()),
                };
                popup.collection.selected = Some((index, record.clone()));
                popup.collection.focused = Some((index, record));
                popup.collection.requested_focus = None;
                let efforts = popup.efforts();
                let focused = if reasoning_activation {
                    effort
                        .and_then(|effort| efforts.iter().position(|value| *value == effort))
                        .map(|position| popup.model_row_count() + 1 + position)
                        .unwrap_or(index)
                } else {
                    index
                };
                let facts = rows::ModelMenuRows::new(
                    &popup.collection,
                    &self.model_controls.values,
                    popup.pending,
                    popup.feedback,
                    efforts,
                );
                popup.widget.focus_row(focused, &facts, false);
                cx.notify();
            } else {
                self.close_model_menu(window, cx);
            }
        } else {
            popup.activation = Some((index, record, effort, reasoning_activation));
            popup.desired = index;
            self.start_model_page(false, window, cx);
        }
    }

    fn activate_model_identity(
        &mut self,
        index: usize,
        id: &str,
        effort: Option<ModelReasoningEffort>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(popup) = self.model_controls.popup.as_ref() else {
            return;
        };
        let matches = popup
            .collection
            .row(index)
            .is_some_and(|(_, record)| record.id == id)
            || popup
                .collection
                .selected
                .as_ref()
                .is_some_and(|(selected, record)| *selected == index && record.id == id)
            || popup
                .collection
                .focused
                .as_ref()
                .is_some_and(|(focused, record)| *focused == index && record.id == id);
        if matches {
            self.activate_model_row(index, effort, window, cx);
        }
    }

    fn reconcile_model_menu_values(
        &mut self,
        previous: &ModelDefaults,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if previous.model == self.model_controls.values.model {
            return;
        }
        let Some(popup) = self.model_controls.popup.as_mut() else {
            return;
        };
        let model = self.model_controls.values.model.as_deref();
        popup.collection.selected = popup.collection.pages.iter().find_map(|resident| {
            resident
                .page
                .records()
                .iter()
                .enumerate()
                .find(|(_, record)| Some(record.model.as_str()) == model)
                .map(|(offset, record)| (resident.start + offset, record.clone()))
        });
        popup.preserve_focus = true;
        popup.selected_revealed = false;
        if let Some((index, _)) = &popup.collection.selected {
            popup.selected_revealed = true;
            popup.preserve_focus = false;
            popup.widget.reveal(*index);
        } else if model.is_some() && popup.collection.observed > 0 {
            popup.locating_selected = true;
            self.start_model_page(false, window, cx);
        }
    }
}

#[cfg(test)]
impl MainWindowShellRoot {
    pub(crate) fn test_toggle_model_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.toggle_model_menu(window, cx);
    }

    pub(crate) fn test_retry_model_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.retry_model_menu(window, cx);
    }

    pub(crate) fn test_choose_model(
        &mut self,
        index: usize,
        effort: Option<ModelReasoningEffort>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.activate_model_row(index, effort, window, cx);
    }

    pub(crate) fn test_model_status(&self) -> (ModelDefaults, bool, Option<&'static str>, bool) {
        (
            self.model_controls.values.clone(),
            self.model_controls.runtime_available,
            self.model_disabled_reason(),
            self.model_controls.popup.is_some(),
        )
    }

    pub(crate) fn test_model_menu_state(
        &self,
    ) -> Option<(usize, usize, bool, bool, Option<&'static str>, usize)> {
        let popup = self.model_controls.popup.as_ref()?;
        Some((
            popup.collection.observed,
            popup.collection.pages.len(),
            popup.pending,
            popup.retrying,
            popup.feedback,
            popup
                .collection
                .selected
                .as_ref()
                .map(|(index, _)| *index)
                .unwrap_or(usize::MAX),
        ))
    }

    pub(crate) fn test_model_menu_focused_row(&self) -> Option<&str> {
        self.model_controls
            .popup
            .as_ref()?
            .collection
            .focused
            .as_ref()
            .map(|(_, record)| record.id.as_str())
    }
}
