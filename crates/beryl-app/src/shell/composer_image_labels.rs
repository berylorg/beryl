use std::collections::HashMap;

use beryl_backend::{ThreadInfo, ThreadItem, TurnInfo, TurnStatus, UserInput};

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
    cache: ComposerImageLabelThreadCache,
    last_touched: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct ComposerImageLabelThreadCache {
    allocator: ComposerImageLabelAllocator,
    history: ComposerImageLabelHistoryState,
}

#[allow(dead_code)]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
enum ComposerImageLabelHistoryState {
    #[default]
    Unknown,
    NeedsScan,
    NeedsValidation {
        frontier: Option<ComposerImageLabelHistoryFrontier>,
    },
    Validating {
        frontier: Option<ComposerImageLabelHistoryFrontier>,
    },
    Scanning {
        frontier: Option<ComposerImageLabelHistoryFrontier>,
    },
    Complete {
        frontier: Option<ComposerImageLabelHistoryFrontier>,
    },
    Failed {
        message: String,
    },
    Unavailable {
        message: String,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ComposerImageLabelHistoryFrontier {
    pub(crate) newest_turn_id: Option<String>,
    pub(crate) scanned_turn_count: usize,
    pub(crate) turn_identity_digest: u64,
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
    Validating,
    Scanning,
    Failed { message: String },
    Unavailable { message: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum ComposerImageLabelHistorySyncRequest {
    Scan {
        thread_id: String,
    },
    Validate {
        thread_id: String,
        frontier: Option<ComposerImageLabelHistoryFrontier>,
    },
}

#[allow(dead_code)]
impl ComposerImageLabelHistoryState {
    fn frontier(&self) -> Option<&ComposerImageLabelHistoryFrontier> {
        match self {
            ComposerImageLabelHistoryState::NeedsValidation { frontier }
            | ComposerImageLabelHistoryState::Validating { frontier }
            | ComposerImageLabelHistoryState::Scanning { frontier }
            | ComposerImageLabelHistoryState::Complete { frontier } => frontier.as_ref(),
            ComposerImageLabelHistoryState::Unknown
            | ComposerImageLabelHistoryState::NeedsScan
            | ComposerImageLabelHistoryState::Failed { .. }
            | ComposerImageLabelHistoryState::Unavailable { .. } => None,
        }
    }
}

#[allow(dead_code)]
impl ComposerImageLabelHistoryFrontier {
    pub(crate) fn new(newest_turn_id: Option<String>, scanned_turn_count: usize) -> Self {
        Self {
            newest_turn_id,
            scanned_turn_count,
            turn_identity_digest: ComposerImageLabelHistoryFrontierBuilder::empty_digest(),
        }
    }

    pub(crate) fn from_turns_desc(turns: &[TurnInfo]) -> Self {
        let mut builder = ComposerImageLabelHistoryFrontierBuilder::default();
        builder.observe_turns_desc(turns);
        builder.finish()
    }

    pub(crate) fn empty() -> Self {
        ComposerImageLabelHistoryFrontierBuilder::default().finish()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.scanned_turn_count == 0
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ComposerImageLabelHistoryFrontierBuilder {
    newest_turn_id: Option<String>,
    scanned_turn_count: usize,
    digest: u64,
}

impl Default for ComposerImageLabelHistoryFrontierBuilder {
    fn default() -> Self {
        Self {
            newest_turn_id: None,
            scanned_turn_count: 0,
            digest: Self::empty_digest(),
        }
    }
}

impl ComposerImageLabelHistoryFrontierBuilder {
    pub(crate) fn observe_turns_desc(&mut self, turns: &[TurnInfo]) {
        for turn in turns {
            self.observe_turn_desc(turn);
        }
    }

    pub(crate) fn observe_turn_desc(&mut self, turn: &TurnInfo) {
        if self.scanned_turn_count == 0 {
            self.newest_turn_id = Some(turn.id.clone());
        }
        self.scanned_turn_count = self.scanned_turn_count.saturating_add(1);
        self.write_str(&turn.id);
        self.write_u8(turn_status_digest_byte(turn.status));
    }

    pub(crate) fn finish(self) -> ComposerImageLabelHistoryFrontier {
        ComposerImageLabelHistoryFrontier {
            newest_turn_id: self.newest_turn_id,
            scanned_turn_count: self.scanned_turn_count,
            turn_identity_digest: self.digest,
        }
    }

    const fn empty_digest() -> u64 {
        0xcbf29ce484222325
    }

    fn write_str(&mut self, value: &str) {
        self.write_usize(value.len());
        for byte in value.bytes() {
            self.write_u8(byte);
        }
    }

    fn write_usize(&mut self, value: usize) {
        for byte in (value as u64).to_le_bytes() {
            self.write_u8(byte);
        }
    }

    fn write_u8(&mut self, value: u8) {
        self.digest ^= u64::from(value);
        self.digest = self.digest.wrapping_mul(0x100000001b3);
    }
}

impl ComposerImageLabelState {
    pub(super) fn allocate(&mut self, selected_thread_id: Option<&str>) -> String {
        self.allocator_mut(selected_thread_id).allocate()
    }

    pub(super) fn try_allocate(
        &mut self,
        selected_thread_id: Option<&str>,
    ) -> Result<String, ComposerImagePasteReadiness> {
        match self.paste_readiness(selected_thread_id) {
            ComposerImagePasteReadiness::Ready => Ok(self.allocate(selected_thread_id)),
            blocked => Err(blocked),
        }
    }

    pub(super) fn prepare_thread_history_scan(
        &mut self,
        thread_id: &str,
        has_unloaded_history: bool,
    ) {
        let thread = self.thread_state_mut(thread_id);
        match (&thread.cache.history, has_unloaded_history) {
            (ComposerImageLabelHistoryState::Complete { frontier }, true) => {
                thread.cache.history = ComposerImageLabelHistoryState::NeedsValidation {
                    frontier: frontier.clone(),
                };
            }
            (_, true) => thread.cache.history = ComposerImageLabelHistoryState::NeedsScan,
            (_, false) => {
                thread.cache.history = ComposerImageLabelHistoryState::Complete { frontier: None };
            }
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
            .map(|thread| &thread.cache.history)
        {
            Some(ComposerImageLabelHistoryState::Complete { .. }) => {
                ComposerImagePasteReadiness::Ready
            }
            Some(ComposerImageLabelHistoryState::NeedsValidation { .. })
            | Some(ComposerImageLabelHistoryState::Validating { .. }) => {
                ComposerImagePasteReadiness::Validating
            }
            Some(ComposerImageLabelHistoryState::Failed { message }) => {
                ComposerImagePasteReadiness::Failed {
                    message: message.clone(),
                }
            }
            Some(ComposerImageLabelHistoryState::Unavailable { message }) => {
                ComposerImagePasteReadiness::Unavailable {
                    message: message.clone(),
                }
            }
            Some(ComposerImageLabelHistoryState::NeedsScan)
            | Some(ComposerImageLabelHistoryState::Scanning { .. })
            | Some(ComposerImageLabelHistoryState::Unknown)
            | None => ComposerImagePasteReadiness::Scanning,
        }
    }

    pub(super) fn selected_thread_needing_history_sync(
        &self,
        selected_thread_id: Option<&str>,
    ) -> Option<ComposerImageLabelHistorySyncRequest> {
        let thread_id = selected_thread_id?;
        match self
            .threads
            .get(thread_id)
            .map(|thread| &thread.cache.history)
        {
            Some(ComposerImageLabelHistoryState::NeedsValidation { frontier }) => {
                Some(ComposerImageLabelHistorySyncRequest::Validate {
                    thread_id: thread_id.to_string(),
                    frontier: frontier.clone(),
                })
            }
            Some(
                ComposerImageLabelHistoryState::NeedsScan | ComposerImageLabelHistoryState::Unknown,
            )
            | None => Some(ComposerImageLabelHistorySyncRequest::Scan {
                thread_id: thread_id.to_string(),
            }),
            Some(
                ComposerImageLabelHistoryState::Validating { .. }
                | ComposerImageLabelHistoryState::Scanning { .. }
                | ComposerImageLabelHistoryState::Complete { .. }
                | ComposerImageLabelHistoryState::Failed { .. }
                | ComposerImageLabelHistoryState::Unavailable { .. },
            ) => None,
        }
    }

    #[allow(dead_code)]
    pub(super) fn selected_thread_needing_history_scan(
        &self,
        selected_thread_id: Option<&str>,
    ) -> Option<String> {
        match self.selected_thread_needing_history_sync(selected_thread_id) {
            Some(ComposerImageLabelHistorySyncRequest::Scan { thread_id }) => Some(thread_id),
            Some(ComposerImageLabelHistorySyncRequest::Validate { .. }) | None => None,
        }
    }

    pub(super) fn begin_thread_history_scan(&mut self, thread_id: &str) -> bool {
        let thread = self.thread_state_mut(thread_id);
        match &thread.cache.history {
            ComposerImageLabelHistoryState::NeedsScan | ComposerImageLabelHistoryState::Unknown => {
                thread.cache.history = ComposerImageLabelHistoryState::Scanning { frontier: None };
                true
            }
            _ => false,
        }
    }

    #[allow(dead_code)]
    pub(super) fn finish_thread_history_scan(
        &mut self,
        thread_id: &str,
        observations: ComposerImageLabelObservations,
    ) {
        self.finish_thread_history_scan_with_frontier(thread_id, observations, None);
    }

    pub(super) fn finish_thread_history_scan_with_frontier(
        &mut self,
        thread_id: &str,
        observations: ComposerImageLabelObservations,
        frontier: Option<ComposerImageLabelHistoryFrontier>,
    ) {
        self.complete_thread_history_scan(thread_id, observations, frontier);
    }

    pub(super) fn finish_in_flight_thread_history_scan_with_frontier(
        &mut self,
        thread_id: &str,
        observations: ComposerImageLabelObservations,
        frontier: Option<ComposerImageLabelHistoryFrontier>,
    ) -> bool {
        let thread = self.thread_state_mut(thread_id);
        if !matches!(
            thread.cache.history,
            ComposerImageLabelHistoryState::Scanning { .. }
        ) {
            return false;
        }
        self.complete_thread_history_scan(thread_id, observations, frontier);
        true
    }

    fn complete_thread_history_scan(
        &mut self,
        thread_id: &str,
        observations: ComposerImageLabelObservations,
        frontier: Option<ComposerImageLabelHistoryFrontier>,
    ) {
        let thread = self.thread_state_mut(thread_id);
        thread
            .cache
            .allocator
            .observe_next_index(observations.next_index);
        thread.cache.history = ComposerImageLabelHistoryState::Complete { frontier };
    }

    pub(super) fn fail_thread_history_scan(&mut self, thread_id: &str, message: impl Into<String>) {
        self.thread_state_mut(thread_id).cache.history = ComposerImageLabelHistoryState::Failed {
            message: bounded_scan_failure_message(message.into()),
        };
    }

    pub(super) fn fail_in_flight_thread_history_scan(
        &mut self,
        thread_id: &str,
        message: impl Into<String>,
    ) -> bool {
        let thread = self.thread_state_mut(thread_id);
        if !matches!(
            thread.cache.history,
            ComposerImageLabelHistoryState::Scanning { .. }
        ) {
            return false;
        }
        thread.cache.history = ComposerImageLabelHistoryState::Failed {
            message: bounded_scan_failure_message(message.into()),
        };
        true
    }

    #[allow(dead_code)]
    pub(super) fn mark_thread_history_needs_validation(&mut self, thread_id: &str) {
        let thread = self.thread_state_mut(thread_id);
        thread.cache.history = match &thread.cache.history {
            ComposerImageLabelHistoryState::Complete { frontier } => {
                ComposerImageLabelHistoryState::NeedsValidation {
                    frontier: frontier.clone(),
                }
            }
            ComposerImageLabelHistoryState::Unknown
            | ComposerImageLabelHistoryState::Failed { .. }
            | ComposerImageLabelHistoryState::Unavailable { .. } => {
                ComposerImageLabelHistoryState::NeedsScan
            }
            ComposerImageLabelHistoryState::NeedsScan
            | ComposerImageLabelHistoryState::NeedsValidation { .. } => {
                thread.cache.history.clone()
            }
            ComposerImageLabelHistoryState::Validating { frontier } => {
                ComposerImageLabelHistoryState::NeedsValidation {
                    frontier: frontier.clone(),
                }
            }
            ComposerImageLabelHistoryState::Scanning { .. } => {
                ComposerImageLabelHistoryState::NeedsScan
            }
        };
    }

    pub(super) fn begin_thread_history_validation(&mut self, thread_id: &str) -> bool {
        let thread = self.thread_state_mut(thread_id);
        let ComposerImageLabelHistoryState::NeedsValidation { frontier } = &thread.cache.history
        else {
            return false;
        };
        thread.cache.history = ComposerImageLabelHistoryState::Validating {
            frontier: frontier.clone(),
        };
        true
    }

    pub(super) fn finish_thread_history_validation(
        &mut self,
        thread_id: &str,
        frontier: ComposerImageLabelHistoryFrontier,
    ) -> bool {
        let thread = self.thread_state_mut(thread_id);
        if !matches!(
            thread.cache.history,
            ComposerImageLabelHistoryState::Validating { .. }
        ) {
            return false;
        }
        thread.cache.history = ComposerImageLabelHistoryState::Complete {
            frontier: Some(frontier),
        };
        true
    }

    pub(super) fn begin_thread_history_scan_after_validation(
        &mut self,
        thread_id: &str,
        frontier: ComposerImageLabelHistoryFrontier,
    ) -> bool {
        let thread = self.thread_state_mut(thread_id);
        if !matches!(
            thread.cache.history,
            ComposerImageLabelHistoryState::Validating { .. }
        ) {
            return false;
        }
        thread.cache.history = ComposerImageLabelHistoryState::Scanning {
            frontier: Some(frontier),
        };
        true
    }

    pub(super) fn fail_thread_history_validation(
        &mut self,
        thread_id: &str,
        message: impl Into<String>,
    ) {
        self.thread_state_mut(thread_id).cache.history =
            ComposerImageLabelHistoryState::Unavailable {
                message: bounded_scan_failure_message(message.into()),
            };
    }

    pub(super) fn fail_in_flight_thread_history_validation(
        &mut self,
        thread_id: &str,
        message: impl Into<String>,
    ) -> bool {
        let thread = self.thread_state_mut(thread_id);
        if !matches!(
            thread.cache.history,
            ComposerImageLabelHistoryState::Validating { .. }
        ) {
            return false;
        }
        thread.cache.history = ComposerImageLabelHistoryState::Unavailable {
            message: bounded_scan_failure_message(message.into()),
        };
        true
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

    pub(super) fn observe_thread_items(&mut self, thread_id: &str, items: &[ThreadItem]) {
        for item in items {
            if let ThreadItem::UserMessage(message) = item {
                self.observe_thread_backend_input(thread_id, &message.content);
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
        self.thread_allocator_mut(thread_id)
            .observe_backend_input(records);
    }

    pub(super) fn bind_pending_new_thread_to_thread(&mut self, thread_id: &str) {
        let pending = std::mem::take(&mut self.pending_new_thread);
        let thread = self.thread_state_mut(thread_id);
        thread.cache.allocator.merge(pending);
        thread.cache.history = ComposerImageLabelHistoryState::Complete { frontier: None };
    }

    pub(super) fn reset_pending_new_thread(&mut self) {
        self.pending_new_thread = ComposerImageLabelAllocator::default();
    }

    fn observe_turn(&mut self, thread_id: &str, turn: &TurnInfo) {
        self.observe_thread_items(thread_id, &turn.items);
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
        &mut self.thread_state_mut(thread_id).cache.allocator
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

fn turn_status_digest_byte(status: TurnStatus) -> u8 {
    match status {
        TurnStatus::Completed => 1,
        TurnStatus::Interrupted => 2,
        TurnStatus::Failed => 3,
        TurnStatus::InProgress => 4,
    }
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
    #[allow(dead_code)]
    pub(crate) fn observe_turns(&mut self, turns: &[TurnInfo]) {
        for turn in turns {
            self.observe_turn(turn);
        }
    }

    pub(crate) fn observe_turn(&mut self, turn: &TurnInfo) {
        observe_turn_label_indexes(turn, |label_index| self.observe_label_index(label_index));
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
