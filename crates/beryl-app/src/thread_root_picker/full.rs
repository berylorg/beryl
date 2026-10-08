use super::runtime::RuntimeViewport;
use super::*;

pub const PICKER_COLLECTION_HISTORY_CAPACITY: usize = 16;

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum PickerCommand {
    Return,
    BrowseRoots(PickerRowKey),
    AddRoot(PickerRowKey),
    AddRuntime,
    Confirm(PickerRowKey),
    RetryCollection,
    RetryRuntime,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PickerCommandState {
    pub label: String,
    pub unavailable_reason: Option<String>,
    pub pending: bool,
}

impl PickerCommandState {
    pub fn enabled(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            unavailable_reason: None,
            pending: false,
        }
    }

    pub fn unavailable(label: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            unavailable_reason: Some(reason.into()),
            pending: false,
        }
    }

    pub fn can_dispatch(&self) -> bool {
        !self.pending && self.unavailable_reason.is_none()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PickerSelectionMode {
    Immediate,
    Confirmed { confirm: PickerCommandState },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PickerRowPresentation {
    Thread,
    Root,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PickerRuntimeSectionConfig {
    pub heading: String,
    pub empty_text: String,
    pub add_runtime: PickerCommandState,
}

struct CollectionMemory {
    key: PickerCollectionKey,
    focused: Option<(PickerRowKey, usize)>,
    scroll: f32,
    query: String,
}

pub(super) struct SelectionEligibility {
    pub key: PickerRowKey,
    pub collection_key: PickerCollectionKey,
    pub revision: u64,
    pub unavailable_reason: Option<String>,
}

pub(super) struct FullPickerState {
    pub runtime: Option<RuntimeViewport>,
    pub selection: PickerSelectionMode,
    pub selected: Option<PickerRowKey>,
    pub presentation: PickerRowPresentation,
    pub return_command: Option<PickerCommandState>,
    pub in_flight: Option<PickerCommand>,
    pub in_flight_state: Option<PickerCommandState>,
    pub pending_page_retry_allowed: bool,
    pub command_focus: Vec<(PickerCommand, FocusHandle)>,
    pub rendered_commands: Vec<PickerCommand>,
    pub native_dialog_open: bool,
    pub external_command_reason: Option<String>,
    pub external_search_enabled: Option<bool>,
    pub collection_retry: Option<PickerCommandState>,
    pub runtime_retry: Option<PickerCommandState>,
    pub eligibility: Option<SelectionEligibility>,
    memories: Vec<CollectionMemory>,
}

impl FullPickerState {
    pub fn new(key: &PickerCollectionKey) -> Self {
        let mut memories = Vec::with_capacity(PICKER_COLLECTION_HISTORY_CAPACITY);
        memories.push(CollectionMemory {
            key: key.clone(),
            focused: None,
            scroll: 0.,
            query: String::new(),
        });
        Self {
            runtime: None,
            selection: PickerSelectionMode::Immediate,
            selected: None,
            presentation: PickerRowPresentation::Thread,
            return_command: None,
            in_flight: None,
            in_flight_state: None,
            pending_page_retry_allowed: false,
            command_focus: Vec::new(),
            rendered_commands: Vec::new(),
            native_dialog_open: false,
            external_command_reason: None,
            external_search_enabled: None,
            collection_retry: None,
            runtime_retry: None,
            eligibility: None,
            memories,
        }
    }
}

impl ThreadRootPicker {
    pub fn retained_collection_count(&self) -> usize {
        self.full.memories.len()
    }

    pub(super) fn discard_collection_memory(&mut self) {
        self.full.memories.clear();
    }
    pub fn configure_selection(&mut self, selection: PickerSelectionMode, cx: &mut Context<Self>) {
        if self.dismissed {
            return;
        }
        if matches!(selection, PickerSelectionMode::Immediate) {
            self.full.selected = None;
            self.full.eligibility = None;
        }
        self.full.selection = selection;
        self.request_viewport(cx);
        cx.notify();
    }

    pub fn set_row_presentation(
        &mut self,
        presentation: PickerRowPresentation,
        cx: &mut Context<Self>,
    ) {
        self.full.presentation = presentation;
        self.request_viewport(cx);
        cx.notify();
    }

    pub fn set_return_command(
        &mut self,
        command: Option<PickerCommandState>,
        cx: &mut Context<Self>,
    ) {
        self.full.return_command = command;
        cx.notify();
    }

    pub fn set_collection_labels(
        &mut self,
        helper: String,
        heading: String,
        empty_text: String,
        cx: &mut Context<Self>,
    ) {
        self.config.helper = helper;
        self.config.heading = heading;
        self.config.empty_text = empty_text;
        cx.notify();
    }

    pub fn clear_search(&mut self, cx: &mut Context<Self>) {
        if self.full.external_command_reason.is_some() {
            return;
        }
        self.change_query(String::new(), cx);
        let query = self.query.clone();
        self.search
            .update(cx, |input, cx| input.set_text(query, cx));
    }

    pub fn query_text(&self) -> &str {
        &self.query
    }

    pub fn selected_key(&self) -> Option<&PickerRowKey> {
        self.full.selected.as_ref()
    }

    pub fn set_selected_key(&mut self, selected: Option<PickerRowKey>, cx: &mut Context<Self>) {
        if self.dismissed
            || matches!(self.full.selection, PickerSelectionMode::Immediate)
            || self.full.native_dialog_open
            || self.full.external_command_reason.is_some()
            || self.full.in_flight.is_some()
        {
            return;
        }
        if self.full.selected != selected {
            self.full.eligibility = None;
        }
        self.full.selected = selected;
        cx.notify();
    }

    pub fn set_selection_eligibility(
        &mut self,
        key: &PickerRowKey,
        unavailable_reason: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if self.dismissed || self.full.selected.as_ref() != Some(key) {
            return;
        }
        self.full.eligibility = Some(SelectionEligibility {
            key: key.clone(),
            collection_key: self.collection.key.clone(),
            revision: self.collection.revision,
            unavailable_reason,
        });
        cx.notify();
    }

    pub fn confirmation_state(&self) -> Option<PickerCommandState> {
        let PickerSelectionMode::Confirmed { confirm } = &self.full.selection else {
            return None;
        };
        let Some(key) = &self.full.selected else {
            let mut state = confirm.clone();
            state.unavailable_reason = Some("Choose a root before confirming.".into());
            return Some(state);
        };
        self.command_state(&PickerCommand::Confirm(key.clone()))
    }

    pub fn set_retry_commands(
        &mut self,
        collection: Option<PickerCommandState>,
        runtime: Option<PickerCommandState>,
        cx: &mut Context<Self>,
    ) {
        self.full.collection_retry = collection;
        self.full.runtime_retry = runtime;
        cx.notify();
    }

    pub(super) fn invalidate_selection_eligibility(&mut self) {
        self.full.eligibility = None;
    }

    pub(super) fn collection_height(&self) -> f32 {
        let style = self.config.style;
        let runtime = if self.full.runtime.is_some() {
            style.heading_height
                + style.runtime_viewport_height
                + style.command_height
                + style.gap * 3.
        } else {
            0.
        };
        let footer = if matches!(self.full.selection, PickerSelectionMode::Confirmed { .. }) {
            style.footer_height + style.gap
        } else {
            0.
        };
        (style.viewport_height() - runtime - footer).max(1.)
    }

    pub(super) fn collection_row_height(&self) -> f32 {
        match self.full.presentation {
            PickerRowPresentation::Thread => self.config.style.row_height,
            PickerRowPresentation::Root => self.config.style.root_row_height,
        }
    }

    pub(super) fn collection_stride(&self) -> f32 {
        (self.collection_row_height() + self.config.style.row_gap).max(1.)
    }

    pub(super) fn save_collection_memory(&mut self) {
        self.full
            .memories
            .retain(|memory| memory.key != self.collection.key);
        self.retain_collection_memory(CollectionMemory {
            key: self.collection.key.clone(),
            focused: self.collection.focused.clone(),
            scroll: -f32::from(self.scroll.offset().y),
            query: self.query.clone(),
        });
    }

    pub(super) fn restore_collection_memory(&mut self) -> Option<usize> {
        let index = self
            .full
            .memories
            .iter()
            .position(|memory| memory.key == self.collection.key);
        let Some(index) = index else {
            self.retain_collection_memory(CollectionMemory {
                key: self.collection.key.clone(),
                focused: None,
                scroll: 0.,
                query: String::new(),
            });
            return None;
        };
        let memory = self.full.memories.remove(index);
        self.collection.focused = memory.focused.clone();
        self.scroll.set_offset(point(px(0.), px(-memory.scroll)));
        self.last_coherent_scroll = memory.scroll;
        self.query = memory.query.clone();
        let position = memory.focused.as_ref().map(|(_, position)| *position);
        self.retain_collection_memory(memory);
        position
    }

    fn retain_collection_memory(&mut self, memory: CollectionMemory) {
        if self.full.memories.len() == PICKER_COLLECTION_HISTORY_CAPACITY {
            self.full.memories.remove(0);
        }
        self.full.memories.push(memory);
    }
}
