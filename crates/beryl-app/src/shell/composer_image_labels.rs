use std::collections::HashMap;

use beryl_backend::{ThreadInfo, ThreadItem, TurnInfo, UserInput};

pub(super) const COMPOSER_IMAGE_LABEL_MAX_THREADS: usize = 256;
pub(super) const COMPOSER_IMAGE_LABEL_SCAN_ERROR_MAX_BYTES: usize = 4096;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct ComposerImageLabelState {
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
    history_scan: ComposerImageLabelScanState,
    last_touched: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
enum ComposerImageLabelScanState {
    #[default]
    Unknown,
    Incomplete,
    Complete,
    Failed {
        message: String,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ComposerImageLabelObservations {
    next_index: usize,
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
    Failed { message: String },
}

impl ComposerImageLabelState {
    pub(super) fn allocate(&mut self, selected_thread_id: Option<&str>) -> String {
        self.allocator_mut(selected_thread_id).allocate()
    }

    pub(super) fn prepare_thread_history_scan(
        &mut self,
        thread_id: &str,
        has_unloaded_history: bool,
    ) {
        let thread = self.thread_state_mut(thread_id);
        match (&thread.history_scan, has_unloaded_history) {
            (ComposerImageLabelScanState::Complete, true) => {}
            (_, true) => thread.history_scan = ComposerImageLabelScanState::Incomplete,
            (_, false) => thread.history_scan = ComposerImageLabelScanState::Complete,
        }
    }

    pub(super) fn paste_readiness(
        &self,
        selected_thread_id: Option<&str>,
    ) -> ComposerImagePasteReadiness {
        let Some(thread_id) = selected_thread_id else {
            return ComposerImagePasteReadiness::Ready;
        };

        match self
            .threads
            .get(thread_id)
            .map(|thread| &thread.history_scan)
        {
            Some(ComposerImageLabelScanState::Complete) => ComposerImagePasteReadiness::Ready,
            Some(ComposerImageLabelScanState::Failed { message }) => {
                ComposerImagePasteReadiness::Failed {
                    message: message.clone(),
                }
            }
            Some(ComposerImageLabelScanState::Incomplete)
            | Some(ComposerImageLabelScanState::Unknown)
            | None => ComposerImagePasteReadiness::Scanning,
        }
    }

    pub(super) fn selected_thread_needing_history_scan(
        &self,
        selected_thread_id: Option<&str>,
    ) -> Option<String> {
        let thread_id = selected_thread_id?;
        self.threads
            .get(thread_id)
            .is_some_and(|thread| thread.history_scan == ComposerImageLabelScanState::Incomplete)
            .then(|| thread_id.to_string())
    }

    pub(super) fn finish_thread_history_scan(
        &mut self,
        thread_id: &str,
        observations: ComposerImageLabelObservations,
    ) {
        let thread = self.thread_state_mut(thread_id);
        thread.allocator.observe_next_index(observations.next_index);
        thread.history_scan = ComposerImageLabelScanState::Complete;
    }

    pub(super) fn fail_thread_history_scan(&mut self, thread_id: &str, message: impl Into<String>) {
        self.thread_state_mut(thread_id).history_scan = ComposerImageLabelScanState::Failed {
            message: bounded_scan_failure_message(message.into()),
        };
    }

    pub(super) fn observe_thread_history(&mut self, thread: &ThreadInfo) {
        let thread_id = thread.summary().id;
        self.observe_thread_turns(&thread_id, &thread.turns);
    }

    pub(super) fn observe_thread_turns(&mut self, thread_id: &str, turns: &[TurnInfo]) {
        for turn in turns {
            self.observe_turn(thread_id, turn);
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
        self.thread_allocator_mut(thread_id)
            .observe_backend_input(records);
    }

    pub(super) fn bind_pending_new_thread_to_thread(&mut self, thread_id: &str) {
        let pending = std::mem::take(&mut self.pending_new_thread);
        let thread = self.thread_state_mut(thread_id);
        thread.allocator.merge(pending);
        thread.history_scan = ComposerImageLabelScanState::Complete;
    }

    pub(super) fn reset_pending_new_thread(&mut self) {
        self.pending_new_thread = ComposerImageLabelAllocator::default();
    }

    fn observe_turn(&mut self, thread_id: &str, turn: &TurnInfo) {
        for item in &turn.items {
            if let ThreadItem::UserMessage(message) = item {
                self.observe_thread_backend_input(thread_id, &message.content);
            }
        }
    }

    fn allocator_mut(
        &mut self,
        selected_thread_id: Option<&str>,
    ) -> &mut ComposerImageLabelAllocator {
        match selected_thread_id {
            Some(thread_id) => self.thread_allocator_mut(thread_id),
            None => &mut self.pending_new_thread,
        }
    }

    fn thread_allocator_mut(&mut self, thread_id: &str) -> &mut ComposerImageLabelAllocator {
        &mut self.thread_state_mut(thread_id).allocator
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

    #[cfg(test)]
    pub(super) fn retained_thread_count_for_test(&self) -> usize {
        self.threads.len()
    }

    #[cfg(test)]
    pub(super) fn has_thread_for_test(&self, thread_id: &str) -> bool {
        self.threads.contains_key(thread_id)
    }
}

fn bounded_scan_failure_message(message: String) -> String {
    if message.len() <= COMPOSER_IMAGE_LABEL_SCAN_ERROR_MAX_BYTES {
        return message;
    }

    let suffix = "...";
    let mut end = COMPOSER_IMAGE_LABEL_SCAN_ERROR_MAX_BYTES.saturating_sub(suffix.len());
    while !message.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}{suffix}", &message[..end])
}

impl ComposerImageLabelAllocator {
    fn allocate(&mut self) -> String {
        let label = image_label_for_index(self.next_index);
        self.next_index = self.next_index.saturating_add(1);
        label
    }

    fn merge(&mut self, other: Self) {
        self.next_index = self.next_index.max(other.next_index);
    }

    fn observe_backend_input(&mut self, records: &[UserInput]) {
        observe_backend_input_label_indexes(records, |label_index| {
            self.observe_label_index(label_index)
        });
    }

    fn observe_label_index(&mut self, index: usize) {
        self.next_index = self.next_index.max(index.saturating_add(1));
    }

    fn observe_next_index(&mut self, next_index: usize) {
        self.next_index = self.next_index.max(next_index);
    }
}

impl ComposerImageLabelObservations {
    pub(crate) fn observe_turns(&mut self, turns: &[TurnInfo]) {
        for turn in turns {
            observe_turn_label_indexes(turn, |label_index| self.observe_label_index(label_index));
        }
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn next_index(&self) -> usize {
        self.next_index
    }

    fn observe_label_index(&mut self, index: usize) {
        self.next_index = self.next_index.max(index.saturating_add(1));
    }
}

pub(super) fn image_label_for_index(mut index: usize) -> String {
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

pub(super) fn image_label_index(label: &str) -> Option<usize> {
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

fn observe_turn_label_indexes(turn: &TurnInfo, mut observe: impl FnMut(usize)) {
    for item in &turn.items {
        if let ThreadItem::UserMessage(message) = item {
            observe_backend_input_label_indexes(&message.content, &mut observe);
        }
    }
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
