use std::collections::{HashMap, HashSet};

use beryl_backend::UserInput;

const COMPOSER_IMAGE_LABEL_MAX_THREADS: usize = 256;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct ComposerImageLabelFrontier {
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
    last_touched: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct GeneratedImageAnchor {
    record_index: usize,
    label_index: usize,
}

impl ComposerImageLabelFrontier {
    pub(super) fn try_allocate(
        &mut self,
        thread_id: &str,
        reserved_labels: impl IntoIterator<Item = impl AsRef<str>>,
    ) -> String {
        self.thread_state_mut(thread_id)
            .allocator
            .allocate(reserved_labels)
    }

    pub(super) fn observe_backend_input(&mut self, thread_id: &str, records: &[UserInput]) {
        let thread = self.thread_state_mut(thread_id);
        thread.allocator.observe_backend_input(records);
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

    fn observe_backend_input(&mut self, records: &[UserInput]) {
        observe_backend_input_label_indexes(records, |label_index| {
            self.observe_label_index(label_index)
        });
    }

    fn observe_label_index(&mut self, index: usize) {
        self.next_index = self.next_index.max(index.saturating_add(1));
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
