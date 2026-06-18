use std::{
    collections::{BTreeSet, HashSet},
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, TryRecvError},
    thread,
};

use beryl_backend::UserInput;
use syndic_storage::{
    HistoryState, SourceEventRecord, StoreOpenOptions, SyndicStore, ThreadViewId,
    TranscriptPageAnchor, TranscriptPageDirection, TurnId,
};

const LABEL_FRONTIER_PAGE_LIMIT: usize = 1_024;
const LABEL_FRONTIER_MAX_VIEW_RECORDS: usize = 8_192;
const LABEL_FRONTIER_SOURCE_EVENT_LIMIT: usize = 1_024;
const LABEL_FRONTIER_MAX_SOURCE_EVENTS_PER_TURN: usize = 8_192;

pub(crate) struct ComposerImageLabelFrontierTask {
    thread_id: String,
    expected_updated_at: i64,
    receiver: Receiver<ComposerImageLabelFrontierUpdate>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ComposerImageLabelFrontierUpdate {
    pub(crate) thread_id: String,
    pub(crate) expected_updated_at: i64,
    pub(crate) outcome: ComposerImageLabelFrontierOutcome,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ComposerImageLabelFrontierOutcome {
    Ready { labels: Vec<String> },
    Unavailable { message: String },
}

impl ComposerImageLabelFrontierTask {
    pub(crate) fn try_recv(&self) -> Result<ComposerImageLabelFrontierUpdate, TryRecvError> {
        self.receiver.try_recv()
    }

    pub(crate) fn disconnected_update(&self) -> ComposerImageLabelFrontierUpdate {
        ComposerImageLabelFrontierUpdate {
            thread_id: self.thread_id.clone(),
            expected_updated_at: self.expected_updated_at,
            outcome: ComposerImageLabelFrontierOutcome::Unavailable {
                message: "Beryl lost the background Syndic image-label frontier scan.".to_string(),
            },
        }
    }
}

pub(crate) fn spawn_composer_image_label_frontier_worker(
    storage_dir: PathBuf,
    thread_id: String,
    expected_updated_at: i64,
) -> ComposerImageLabelFrontierTask {
    let (sender, receiver) = mpsc::channel();
    let task_thread_id = thread_id.clone();
    thread::spawn(move || {
        let outcome = match scan_composer_image_label_frontier(&storage_dir, &thread_id) {
            Ok(labels) => ComposerImageLabelFrontierOutcome::Ready { labels },
            Err(message) => ComposerImageLabelFrontierOutcome::Unavailable { message },
        };
        let _ = sender.send(ComposerImageLabelFrontierUpdate {
            thread_id,
            expected_updated_at,
            outcome,
        });
    });
    ComposerImageLabelFrontierTask {
        thread_id: task_thread_id,
        expected_updated_at,
        receiver,
    }
}

pub(crate) fn scan_composer_image_label_frontier(
    storage_dir: &Path,
    thread_id: &str,
) -> Result<Vec<String>, String> {
    let store = SyndicStore::open(storage_dir, StoreOpenOptions::default())
        .map_err(|error| format!("Syndic image-label frontier storage is unavailable: {error}"))?;
    let view_id = ThreadViewId::from(thread_id.to_string());
    let conversation = store
        .conversation_by_view(&view_id)
        .map_err(|error| format!("Syndic image-label frontier lookup failed: {error}"))?
        .ok_or_else(|| {
            "Syndic has no captured transcript history for this thread yet.".to_string()
        })?;

    match &conversation.history_state {
        HistoryState::Complete => {}
        HistoryState::Incomplete { reason, detail } => {
            return Err(format!(
                "Syndic history is incomplete for this thread: {}.",
                history_state_detail(reason, detail.as_deref())
            ));
        }
        HistoryState::Unavailable { reason, detail } => {
            return Err(format!(
                "Syndic history is unavailable for this thread: {}.",
                history_state_detail(reason, detail.as_deref())
            ));
        }
    }

    let turn_ids = scan_complete_view_turn_ids(&store, &view_id, conversation.current_revision)?;
    let mut label_indexes = BTreeSet::new();
    for turn_id in turn_ids {
        scan_turn_label_indexes(&store, &turn_id, &mut label_indexes)?;
    }

    Ok(label_indexes
        .into_iter()
        .map(image_label_for_index)
        .collect())
}

fn scan_complete_view_turn_ids(
    store: &SyndicStore,
    view_id: &ThreadViewId,
    revision: syndic_storage::ProviderRevision,
) -> Result<Vec<TurnId>, String> {
    let mut anchor = TranscriptPageAnchor::Start;
    let mut turn_ids = Vec::new();
    let mut seen_turn_ids = HashSet::new();
    let mut scanned_records = 0usize;

    loop {
        let page = store
            .read_transcript_page(
                view_id,
                anchor,
                TranscriptPageDirection::Forward,
                LABEL_FRONTIER_PAGE_LIMIT,
                Some(revision),
            )
            .map_err(|error| format!("Syndic image-label frontier page read failed: {error}"))?;

        scanned_records = scanned_records.saturating_add(page.records.len());
        if scanned_records > LABEL_FRONTIER_MAX_VIEW_RECORDS {
            return Err(format!(
                "Syndic image-label frontier scan exceeded {LABEL_FRONTIER_MAX_VIEW_RECORDS} transcript records."
            ));
        }

        for record in page.records {
            let Some(turn_id) = record.provenance.turn_id else {
                continue;
            };
            if seen_turn_ids.insert(turn_id.clone()) {
                turn_ids.push(turn_id);
            }
        }

        if page.at_end {
            break;
        }
        anchor = TranscriptPageAnchor::Cursor(page.next_cursor.ok_or_else(|| {
            "Syndic image-label frontier page did not provide a continuation cursor.".to_string()
        })?);
    }

    Ok(turn_ids)
}

fn scan_turn_label_indexes(
    store: &SyndicStore,
    turn_id: &TurnId,
    label_indexes: &mut BTreeSet<usize>,
) -> Result<(), String> {
    let mut start_sequence = 0u64;
    let mut scanned_events = 0usize;

    loop {
        let page = store
            .read_source_events(turn_id, start_sequence, LABEL_FRONTIER_SOURCE_EVENT_LIMIT)
            .map_err(|error| {
                format!("Syndic image-label frontier source-event read failed: {error}")
            })?;

        scanned_events = scanned_events.saturating_add(page.records.len());
        if scanned_events > LABEL_FRONTIER_MAX_SOURCE_EVENTS_PER_TURN {
            return Err(format!(
                "Syndic image-label frontier scan exceeded {LABEL_FRONTIER_MAX_SOURCE_EVENTS_PER_TURN} source events for one turn."
            ));
        }

        for record in page.records {
            observe_source_event_label_indexes(&record, label_indexes)?;
        }

        if page.at_end {
            break;
        }
        start_sequence = page.next_sequence.ok_or_else(|| {
            "Syndic image-label frontier source-event page did not provide a continuation sequence."
                .to_string()
        })?;
    }

    Ok(())
}

fn observe_source_event_label_indexes(
    record: &SourceEventRecord,
    label_indexes: &mut BTreeSet<usize>,
) -> Result<(), String> {
    if record.payload.kind != "acceptedUserInput" {
        return Ok(());
    }
    let Some(backend_input) = record.payload.body.get("backendInput") else {
        return Ok(());
    };
    let records =
        serde_json::from_value::<Vec<UserInput>>(backend_input.clone()).map_err(|error| {
            format!("Syndic accepted input did not contain readable backend input: {error}")
        })?;
    observe_backend_input_label_indexes(&records, |index| {
        label_indexes.insert(index);
    });
    Ok(())
}

fn history_state_detail(
    reason: &syndic_storage::HistoryIncompleteReason,
    detail: Option<&str>,
) -> String {
    detail
        .filter(|detail| !detail.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("{reason:?}"))
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

#[derive(Clone, Debug, PartialEq, Eq)]
struct GeneratedImageAnchor {
    record_index: usize,
    label_index: usize,
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
