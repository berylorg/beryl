use std::{
    collections::{HashMap, HashSet},
    sync::mpsc::TryRecvError,
};

use beryl_backend::UserInput;
use beryl_model::conversation::ConversationThreadId;
use gpui::{Context, Window};

use super::composer_image_label_frontier_worker::{
    ComposerImageLabelFrontierOutcome, ComposerImageLabelFrontierUpdate,
    spawn_composer_image_label_frontier_worker,
};
use super::{
    ConversationSurfaceState, PENDING_NEW_THREAD_LABEL_SCOPE_BINDINGS_MAX, ShellView,
    turn_input::UserInputFragment,
};

const COMPOSER_IMAGE_LABEL_MAX_THREADS: usize = 256;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct ComposerImageLabelFrontier {
    pending_new_thread: ComposerImageLabelAllocator,
    threads: HashMap<String, ComposerImageLabelThreadState>,
    next_touch_index: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct ComposerImageLabelAllocator {
    next_index: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct ComposerImageLabelThreadState {
    allocator: ComposerImageLabelAllocator,
    frontier: ComposerImageLabelFrontierState,
    last_touched: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
enum ComposerImageLabelFrontierState {
    #[default]
    Unknown,
    Scanning {
        expected_updated_at: i64,
    },
    Ready,
    Unavailable {
        message: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct GeneratedImageAnchor {
    record_index: usize,
    label_index: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ComposerImagePasteReadiness {
    Ready,
    Scanning,
    Unavailable { message: String },
}

impl ComposerImageLabelFrontier {
    pub(super) fn try_allocate(
        &mut self,
        selected_thread_id: Option<&str>,
        reserved_labels: impl IntoIterator<Item = impl AsRef<str>>,
    ) -> Result<String, ComposerImagePasteReadiness> {
        match self.paste_readiness(selected_thread_id) {
            ComposerImagePasteReadiness::Ready => Ok(self
                .allocator_mut(selected_thread_id)
                .allocate(reserved_labels)),
            blocked => Err(blocked),
        }
    }

    pub(super) fn paste_readiness(
        &self,
        selected_thread_id: Option<&str>,
    ) -> ComposerImagePasteReadiness {
        let Some(thread_id) = selected_thread_id else {
            return ComposerImagePasteReadiness::Ready;
        };

        match self.threads.get(thread_id).map(|thread| &thread.frontier) {
            Some(ComposerImageLabelFrontierState::Ready) => ComposerImagePasteReadiness::Ready,
            Some(ComposerImageLabelFrontierState::Scanning { .. }) => {
                ComposerImagePasteReadiness::Scanning
            }
            Some(ComposerImageLabelFrontierState::Unavailable { message }) => {
                ComposerImagePasteReadiness::Unavailable {
                    message: message.clone(),
                }
            }
            Some(ComposerImageLabelFrontierState::Unknown) | None => {
                ComposerImagePasteReadiness::Unavailable {
                    message: "Syndic image-label frontier is not available for this thread yet."
                        .to_string(),
                }
            }
        }
    }

    pub(super) fn observe_backend_input(
        &mut self,
        selected_thread_id: Option<&str>,
        records: &[UserInput],
    ) {
        self.allocator_mut(selected_thread_id)
            .observe_backend_input(records);
    }

    pub(super) fn observe_thread_backend_input(&mut self, thread_id: &str, records: &[UserInput]) {
        let thread = self.thread_state_mut(thread_id);
        thread.allocator.observe_backend_input(records);
    }

    pub(super) fn bind_pending_new_thread_to_thread(&mut self, thread_id: &str) {
        let pending = std::mem::take(&mut self.pending_new_thread);
        let thread = self.thread_state_mut(thread_id);
        thread.allocator.merge(pending);
        thread.frontier = ComposerImageLabelFrontierState::Ready;
    }

    pub(super) fn reset_pending_new_thread(&mut self) {
        self.pending_new_thread = ComposerImageLabelAllocator::default();
    }

    pub(super) fn mark_thread_frontier_unavailable(
        &mut self,
        thread_id: &str,
        message: impl Into<String>,
    ) {
        self.thread_state_mut(thread_id).frontier = ComposerImageLabelFrontierState::Unavailable {
            message: message.into(),
        };
    }

    pub(super) fn mark_thread_frontier_needs_refresh(&mut self, thread_id: &str) {
        self.thread_state_mut(thread_id).frontier = ComposerImageLabelFrontierState::Unknown;
    }

    pub(super) fn begin_thread_frontier_scan(
        &mut self,
        thread_id: &str,
        expected_updated_at: i64,
    ) -> bool {
        let thread = self.thread_state_mut(thread_id);
        match thread.frontier {
            ComposerImageLabelFrontierState::Ready
            | ComposerImageLabelFrontierState::Scanning { .. }
            | ComposerImageLabelFrontierState::Unavailable { .. } => false,
            ComposerImageLabelFrontierState::Unknown => {
                thread.frontier = ComposerImageLabelFrontierState::Scanning {
                    expected_updated_at,
                };
                true
            }
        }
    }

    pub(super) fn apply_thread_frontier_update(
        &mut self,
        update: ComposerImageLabelFrontierUpdate,
    ) -> bool {
        let thread = self.thread_state_mut(&update.thread_id);
        if !matches!(
            thread.frontier,
            ComposerImageLabelFrontierState::Scanning {
                expected_updated_at
            } if expected_updated_at == update.expected_updated_at
        ) {
            return false;
        }

        match update.outcome {
            ComposerImageLabelFrontierOutcome::Ready { labels } => {
                thread
                    .allocator
                    .observe_labels(labels.iter().map(String::as_str));
                thread.frontier = ComposerImageLabelFrontierState::Ready;
            }
            ComposerImageLabelFrontierOutcome::Unavailable { message } => {
                thread.frontier = ComposerImageLabelFrontierState::Unavailable { message };
            }
        }
        true
    }

    fn allocator_mut(
        &mut self,
        selected_thread_id: Option<&str>,
    ) -> &mut ComposerImageLabelAllocator {
        match selected_thread_id {
            Some(thread_id) => {
                let thread = self.thread_state_mut(thread_id);
                &mut thread.allocator
            }
            None => &mut self.pending_new_thread,
        }
    }

    fn thread_state_mut(&mut self, thread_id: &str) -> &mut ComposerImageLabelThreadState {
        let touch_index = self.next_touch_index();
        let thread_id = thread_id.to_string();
        self.threads
            .entry(thread_id.clone())
            .or_default()
            .last_touched = touch_index;
        self.prune_threads(Some(thread_id.as_str()));
        self.threads
            .get_mut(thread_id.as_str())
            .expect("protected thread state should remain after pruning")
    }

    fn next_touch_index(&mut self) -> u64 {
        let touch_index = self.next_touch_index;
        self.next_touch_index = self.next_touch_index.saturating_add(1);
        touch_index
    }

    fn prune_threads(&mut self, protected_thread_id: Option<&str>) {
        if self.threads.len() <= COMPOSER_IMAGE_LABEL_MAX_THREADS {
            return;
        }

        let mut candidates = self
            .threads
            .iter()
            .filter(|(thread_id, _)| Some(thread_id.as_str()) != protected_thread_id)
            .map(|(thread_id, state)| (state.last_touched, thread_id.clone()))
            .collect::<Vec<_>>();
        candidates.sort();

        for (_, thread_id) in candidates {
            if self.threads.len() <= COMPOSER_IMAGE_LABEL_MAX_THREADS {
                break;
            }
            self.threads.remove(&thread_id);
        }
    }
}

impl ComposerImageLabelAllocator {
    fn allocate(&self, reserved_labels: impl IntoIterator<Item = impl AsRef<str>>) -> String {
        let reserved_indexes = reserved_label_indexes(reserved_labels);
        let mut index = self.next_index;
        while reserved_indexes.contains(&index) {
            let Some(next_index) = index.checked_add(1) else {
                break;
            };
            index = next_index;
        }
        image_label_for_index(index)
    }

    fn merge(&mut self, other: Self) {
        self.next_index = self.next_index.max(other.next_index);
    }

    fn observe_backend_input(&mut self, records: &[UserInput]) {
        observe_backend_input_label_indexes(records, |label_index| {
            self.observe_label_index(label_index)
        });
    }

    fn observe_labels<'a>(&mut self, labels: impl IntoIterator<Item = &'a str>) {
        for label in labels {
            if let Some(index) = image_label_index(label) {
                self.observe_label_index(index);
            }
        }
    }

    fn observe_label_index(&mut self, index: usize) {
        self.next_index = self.next_index.max(index.saturating_add(1));
    }
}

impl ConversationSurfaceState {
    pub(super) fn try_allocate_composer_image_label(
        &mut self,
        reserved_labels: &[String],
    ) -> Result<String, ComposerImagePasteReadiness> {
        let selected_thread_id = self.selected_thread_id().map(str::to_string);
        self.composer_image_labels
            .try_allocate(selected_thread_id.as_deref(), reserved_labels)
    }

    pub(super) fn composer_image_paste_readiness(&self) -> ComposerImagePasteReadiness {
        self.composer_image_labels
            .paste_readiness(self.selected_thread_id())
    }

    pub(super) fn observe_composer_image_labels_in_fragment(
        &mut self,
        fragment: &UserInputFragment,
    ) {
        let selected_thread_id = self.selected_thread_id().map(str::to_string);
        self.composer_image_labels
            .observe_backend_input(selected_thread_id.as_deref(), fragment.backend_input());
    }

    pub(super) fn observe_composer_image_labels_in_thread_fragment(
        &mut self,
        thread_id: &str,
        fragment: &UserInputFragment,
    ) {
        self.composer_image_labels
            .observe_thread_backend_input(thread_id, fragment.backend_input());
    }

    pub(super) fn bind_pending_new_thread_image_labels_to_thread(&mut self, thread_id: &str) {
        self.composer_image_labels
            .bind_pending_new_thread_to_thread(thread_id);
        self.pending_new_thread_label_scope_bindings.insert(
            self.pending_new_thread_label_scope_id,
            thread_id.to_string(),
        );
        self.prune_pending_new_thread_label_scope_bindings();
        self.composer_history.bind_pending_new_thread_to_thread(
            self.pending_new_thread_label_scope_id,
            thread_id.to_string(),
        );
    }

    pub(super) fn mark_selected_thread_image_labels_need_refresh_if_updated(
        &mut self,
        thread_id: &str,
        updated_at: i64,
    ) -> bool {
        let Some(index) = self.selected_thread else {
            return false;
        };
        let Some(thread) = self.known_threads.get_mut(index) else {
            return false;
        };
        if thread.id != thread_id || thread.updated_at == updated_at {
            return false;
        }

        thread.updated_at = updated_at;
        self.composer_image_labels
            .mark_thread_frontier_needs_refresh(thread_id);
        true
    }

    pub(super) fn selected_thread_image_label_frontier_request(&self) -> Option<(String, i64)> {
        let thread = self.selected_thread()?;
        Some((thread.id.clone(), thread.updated_at))
    }

    pub(super) fn begin_selected_thread_image_label_frontier_scan(
        &mut self,
        thread_id: &str,
        expected_updated_at: i64,
    ) -> bool {
        let selected = self
            .selected_thread()
            .map(|thread| (thread.id.clone(), thread.updated_at));
        if selected
            .as_ref()
            .is_none_or(|(selected_thread_id, updated_at)| {
                selected_thread_id != thread_id || *updated_at != expected_updated_at
            })
        {
            return false;
        }
        self.composer_image_labels
            .begin_thread_frontier_scan(thread_id, expected_updated_at)
    }

    pub(super) fn mark_selected_thread_image_label_frontier_unavailable(
        &mut self,
        thread_id: &str,
        message: impl Into<String>,
    ) -> bool {
        if self.selected_thread_id() != Some(thread_id) {
            return false;
        }
        self.composer_image_labels
            .mark_thread_frontier_unavailable(thread_id, message);
        true
    }

    pub(super) fn apply_composer_image_label_frontier_update(
        &mut self,
        update: ComposerImageLabelFrontierUpdate,
    ) -> bool {
        if !self.known_threads.iter().any(|thread| {
            thread.id == update.thread_id && thread.updated_at == update.expected_updated_at
        }) {
            return false;
        }
        self.composer_image_labels
            .apply_thread_frontier_update(update)
    }

    fn prune_pending_new_thread_label_scope_bindings(&mut self) {
        if self.pending_new_thread_label_scope_bindings.len()
            <= PENDING_NEW_THREAD_LABEL_SCOPE_BINDINGS_MAX
        {
            return;
        }

        let current_scope = self.pending_new_thread_label_scope_id;
        let mut removable_scopes = self
            .pending_new_thread_label_scope_bindings
            .keys()
            .copied()
            .filter(|scope_id| *scope_id != current_scope)
            .collect::<Vec<_>>();
        removable_scopes.sort_unstable();
        for scope_id in removable_scopes {
            if self.pending_new_thread_label_scope_bindings.len()
                <= PENDING_NEW_THREAD_LABEL_SCOPE_BINDINGS_MAX
            {
                break;
            }
            self.pending_new_thread_label_scope_bindings
                .remove(&scope_id);
        }
    }
}

impl ShellView {
    pub(super) fn poll_composer_image_label_frontier_updates(&mut self) -> bool {
        let Some(task) = self.composer_image_label_frontier_receiver.as_ref() else {
            return false;
        };
        let update = match task.try_recv() {
            Ok(update) => update,
            Err(TryRecvError::Empty) => return false,
            Err(TryRecvError::Disconnected) => task.disconnected_update(),
        };
        self.composer_image_label_frontier_receiver = None;
        self.conversation_surface_mut()
            .is_some_and(|surface| surface.apply_composer_image_label_frontier_update(update))
    }

    pub(super) fn begin_composer_image_label_frontier_refresh_if_needed(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> bool {
        if self.composer_image_label_frontier_receiver.is_some() {
            return false;
        }
        let Some((thread_id, expected_updated_at)) = self
            .conversation_surface()
            .and_then(ConversationSurfaceState::selected_thread_image_label_frontier_request)
        else {
            return false;
        };
        let Some(syndic_view_id) = self.loaded_workspace().and_then(|loaded| {
            let thread_id = ConversationThreadId::new(thread_id.clone());
            loaded
                .workspace_state
                .catalog_thread_registration(&thread_id)
                .and_then(|registration| registration.syndic_view_id())
                .map(|view_id| view_id.as_str().to_string())
        }) else {
            return self.conversation_surface_mut().is_some_and(|surface| {
                surface.mark_selected_thread_image_label_frontier_unavailable(
                    &thread_id,
                    "This thread is not registered as a Syndic conversation view, so image-label frontier data cannot be read.",
                )
            });
        };
        let Some(workspace_id) = self
            .loaded_workspace()
            .map(|loaded| loaded.workspace.id().clone())
        else {
            return false;
        };
        let Some(persistence) = self.workspace_persistence_for_worker() else {
            return self.conversation_surface_mut().is_some_and(|surface| {
                surface.mark_selected_thread_image_label_frontier_unavailable(
                    &thread_id,
                    "Beryl home storage is unavailable, so Syndic image-label frontier data cannot be read.",
                )
            });
        };
        let should_scan = self.conversation_surface_mut().is_some_and(|surface| {
            surface.begin_selected_thread_image_label_frontier_scan(&thread_id, expected_updated_at)
        });
        if !should_scan {
            return false;
        }

        let storage_dir = persistence.workspace_syndic_storage_dir(&workspace_id);
        self.composer_image_label_frontier_receiver =
            Some(spawn_composer_image_label_frontier_worker(
                storage_dir,
                thread_id,
                syndic_view_id,
                expected_updated_at,
            ));
        true
    }
}

fn reserved_label_indexes(labels: impl IntoIterator<Item = impl AsRef<str>>) -> HashSet<usize> {
    labels
        .into_iter()
        .filter_map(|label| image_label_index(label.as_ref()))
        .collect()
}

fn image_label_for_index(mut index: usize) -> String {
    let mut label = Vec::new();
    loop {
        let remainder = index % 26;
        label.push((b'A' + remainder as u8) as char);
        if index < 26 {
            break;
        }
        index = (index / 26) - 1;
    }
    label.iter().rev().collect()
}

fn image_label_index(label: &str) -> Option<usize> {
    if label.is_empty() || !label.bytes().all(|byte| byte.is_ascii_uppercase()) {
        return None;
    }

    let mut one_based = 0usize;
    for byte in label.bytes() {
        let value = usize::from(byte - b'A' + 1);
        one_based = one_based.checked_mul(26)?.checked_add(value)?;
    }
    one_based.checked_sub(1)
}

fn is_image_user_input(input: &UserInput) -> bool {
    matches!(
        input,
        UserInput::Image { .. } | UserInput::LocalImage { .. }
    )
}

fn observe_backend_input_label_indexes(records: &[UserInput], mut observe: impl FnMut(usize)) {
    let anchors = generated_image_anchors_for_records(records);
    let mut next_anchor_index = anchors.len();

    for (record_index, record) in records.iter().enumerate().rev() {
        if !is_image_user_input(record) {
            continue;
        }
        while next_anchor_index > 0 && anchors[next_anchor_index - 1].record_index >= record_index {
            next_anchor_index -= 1;
        }
        if next_anchor_index == 0 {
            continue;
        }

        next_anchor_index -= 1;
        observe(anchors[next_anchor_index].label_index);
    }
}

fn generated_image_anchors_for_records(records: &[UserInput]) -> Vec<GeneratedImageAnchor> {
    let mut anchors = Vec::new();
    for (record_index, record) in records.iter().enumerate() {
        let UserInput::Text { text } = record else {
            continue;
        };
        anchors.extend(generated_image_label_indexes_in_text(text).into_iter().map(
            |label_index| GeneratedImageAnchor {
                record_index,
                label_index,
            },
        ));
    }
    anchors
}

fn generated_image_label_indexes_in_text(text: &str) -> Vec<usize> {
    let bytes = text.as_bytes();
    let mut indexes = Vec::new();
    let mut cursor = 0usize;

    while let Some(relative_start) = text[cursor..].find("Image ") {
        let label_start = cursor + relative_start + "Image ".len();
        let mut label_end = label_start;
        while label_end < bytes.len() && bytes[label_end].is_ascii_uppercase() {
            label_end += 1;
        }
        if label_end == label_start || bytes.get(label_end) != Some(&b':') {
            cursor = label_start;
            continue;
        }
        if let Some(index) = image_label_index(&text[label_start..label_end]) {
            indexes.push(index);
        }
        cursor = label_end + ':'.len_utf8();
    }

    indexes
}
