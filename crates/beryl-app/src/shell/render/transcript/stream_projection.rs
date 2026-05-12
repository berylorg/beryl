use std::{
    cell::RefCell,
    collections::HashMap,
    rc::Rc,
    time::{Duration, Instant},
};

pub(crate) const TRANSCRIPT_STREAM_PROJECTION_MAX_COMPLETED_ENTRIES: usize = 512;
pub(crate) const TRANSCRIPT_STREAM_PROJECTION_MAX_COMPLETED_TEXT_BYTES: usize = 2 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TranscriptStreamProjectionKey(String);

impl TranscriptStreamProjectionKey {
    pub(crate) fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TranscriptStreamProjectionConfig {
    pub(crate) coalesce_interval: Duration,
    pub(crate) max_uncommitted_chars: usize,
}

impl Default for TranscriptStreamProjectionConfig {
    fn default() -> Self {
        Self {
            coalesce_interval: Duration::from_millis(80),
            max_uncommitted_chars: 4096,
        }
    }
}

#[derive(Debug)]
pub(crate) struct TranscriptStreamProjection {
    config: TranscriptStreamProjectionConfig,
    entries: HashMap<TranscriptStreamProjectionKey, TranscriptStreamProjectionEntry>,
    access_tick: u64,
}

#[derive(Debug, Default)]
struct TranscriptStreamProjectionEntry {
    visible_text: String,
    first_uncommitted_at: Option<Instant>,
    last_used: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct TranscriptStreamProjectionRetainedCounts {
    pub(crate) entries: usize,
    pub(crate) key_bytes: usize,
    pub(crate) text_bytes: usize,
    pub(crate) uncommitted_entries: usize,
}

#[derive(Clone)]
pub(super) struct TranscriptStreamProjectionContext {
    projection: Rc<RefCell<TranscriptStreamProjection>>,
}

impl TranscriptStreamProjectionContext {
    pub(super) fn new(projection: Rc<RefCell<TranscriptStreamProjection>>) -> Self {
        Self { projection }
    }

    pub(super) fn visible_text(
        &self,
        key: TranscriptStreamProjectionKey,
        authoritative_text: &str,
        complete: bool,
        now: Instant,
    ) -> String {
        self.projection
            .borrow_mut()
            .visible_text(key, authoritative_text, complete, now)
    }
}

impl TranscriptStreamProjection {
    pub(crate) fn new(config: TranscriptStreamProjectionConfig) -> Self {
        Self {
            config,
            entries: HashMap::new(),
            access_tick: 0,
        }
    }

    pub(crate) fn visible_text(
        &mut self,
        key: TranscriptStreamProjectionKey,
        authoritative_text: &str,
        complete: bool,
        now: Instant,
    ) -> String {
        self.access_tick = self.access_tick.saturating_add(1);
        let access_tick = self.access_tick;
        let key_for_return = key.clone();
        {
            let entry = self.entries.entry(key).or_default();
            entry.last_used = access_tick;
            if complete {
                entry.visible_text.clear();
                entry.visible_text.push_str(authoritative_text);
                entry.first_uncommitted_at = None;
            } else {
                if !authoritative_text.starts_with(entry.visible_text.as_str()) {
                    entry.visible_text.clear();
                    entry.first_uncommitted_at = None;
                }

                if authoritative_text.len() == entry.visible_text.len() {
                    entry.first_uncommitted_at = None;
                } else {
                    let visible_len = entry.visible_text.len();
                    let stable_prefix_len = stable_prefix_len(authoritative_text).max(visible_len);
                    if stable_prefix_len > visible_len {
                        entry.visible_text.clear();
                        entry
                            .visible_text
                            .push_str(&authoritative_text[..stable_prefix_len]);
                        entry.first_uncommitted_at =
                            (stable_prefix_len < authoritative_text.len()).then_some(now);
                    } else {
                        let first_uncommitted_at = *entry.first_uncommitted_at.get_or_insert(now);
                        let uncommitted_chars = authoritative_text[visible_len..].chars().count();
                        if now.saturating_duration_since(first_uncommitted_at)
                            >= self.config.coalesce_interval
                            || uncommitted_chars >= self.config.max_uncommitted_chars
                        {
                            entry.visible_text.clear();
                            entry.visible_text.push_str(authoritative_text);
                            entry.first_uncommitted_at = None;
                        }
                    }
                }
            }
        }

        self.prune_completed_entries();
        self.entries
            .get(&key_for_return)
            .map(|entry| entry.visible_text.clone())
            .unwrap_or_default()
    }

    #[allow(dead_code)]
    pub(crate) fn clear(&mut self) {
        self.entries.clear();
    }

    pub(crate) fn retained_counts(&self) -> TranscriptStreamProjectionRetainedCounts {
        TranscriptStreamProjectionRetainedCounts {
            entries: self.entries.len(),
            key_bytes: self.entries.keys().map(|key| key.0.len()).sum(),
            text_bytes: self
                .entries
                .values()
                .map(|entry| entry.visible_text.len())
                .sum(),
            uncommitted_entries: self
                .entries
                .values()
                .filter(|entry| entry.first_uncommitted_at.is_some())
                .count(),
        }
    }

    fn prune_completed_entries(&mut self) {
        while self.completed_entry_count() > TRANSCRIPT_STREAM_PROJECTION_MAX_COMPLETED_ENTRIES
            || self.completed_text_bytes() > TRANSCRIPT_STREAM_PROJECTION_MAX_COMPLETED_TEXT_BYTES
        {
            let Some(key) = self
                .entries
                .iter()
                .filter(|(_, entry)| entry.first_uncommitted_at.is_none())
                .min_by_key(|(_, entry)| entry.last_used)
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            self.entries.remove(&key);
        }
    }

    fn completed_entry_count(&self) -> usize {
        self.entries
            .values()
            .filter(|entry| entry.first_uncommitted_at.is_none())
            .count()
    }

    fn completed_text_bytes(&self) -> usize {
        self.entries
            .values()
            .filter(|entry| entry.first_uncommitted_at.is_none())
            .map(|entry| entry.visible_text.len())
            .sum()
    }
}

impl Default for TranscriptStreamProjection {
    fn default() -> Self {
        Self::new(TranscriptStreamProjectionConfig::default())
    }
}

fn stable_prefix_len(source: &str) -> usize {
    let mut stable_len = 0;
    let mut line_start = 0;
    let mut list_marker_starts = Vec::new();
    let mut fence: Option<FenceMarker> = None;

    for (line_end_without_newline, _) in source.match_indices('\n') {
        let line_end = line_end_without_newline + 1;
        let line = &source[line_start..line_end_without_newline];
        stable_len = stable_len.max(stable_line_boundary(line, line_end, &mut fence));
        if fence.is_none() && is_list_marker(line) {
            list_marker_starts.push(line_start);
        }
        line_start = line_end;
    }

    if fence.is_none() {
        let partial_line = &source[line_start..];
        if is_list_marker(partial_line) {
            list_marker_starts.push(line_start);
        }
    }

    if list_marker_starts.len() >= 2
        && let Some(last_marker_start) = list_marker_starts.last().copied()
    {
        stable_len = stable_len.max(last_marker_start);
    }

    stable_len
}

fn stable_line_boundary(line: &str, line_end: usize, fence: &mut Option<FenceMarker>) -> usize {
    if let Some(open_fence) = *fence {
        if fence_marker(line).is_some_and(|candidate| candidate.closes(open_fence)) {
            *fence = None;
            return line_end;
        }
        return 0;
    }

    if let Some(open_fence) = fence_marker(line) {
        *fence = Some(open_fence);
        return 0;
    }

    if line.trim().is_empty() {
        return line_end;
    }

    0
}

#[derive(Clone, Copy, Debug)]
struct FenceMarker {
    marker: char,
    len: usize,
}

impl FenceMarker {
    fn closes(self, open: Self) -> bool {
        self.marker == open.marker && self.len >= open.len
    }
}

fn fence_marker(line: &str) -> Option<FenceMarker> {
    let trimmed = line.trim_start_matches(' ');
    if line.len().saturating_sub(trimmed.len()) > 3 {
        return None;
    }

    let marker = trimmed.chars().next()?;
    if marker != '`' && marker != '~' {
        return None;
    }

    let len = trimmed
        .chars()
        .take_while(|candidate| *candidate == marker)
        .count();
    (len >= 3).then_some(FenceMarker { marker, len })
}

fn is_list_marker(line: &str) -> bool {
    let trimmed = line.trim_start_matches(' ');
    if line.len().saturating_sub(trimmed.len()) > 3 {
        return false;
    }

    is_unordered_list_marker(trimmed) || is_ordered_list_marker(trimmed)
}

fn is_unordered_list_marker(line: &str) -> bool {
    matches!(line.as_bytes(), [b'-' | b'*' | b'+', b' ' | b'\t', ..])
}

fn is_ordered_list_marker(line: &str) -> bool {
    let bytes = line.as_bytes();
    let mut index = 0;
    while index < bytes.len() && bytes[index].is_ascii_digit() {
        index += 1;
    }
    if index == 0 || index > 9 {
        return false;
    }

    matches!(
        bytes.get(index..index + 2),
        Some([b'.' | b')', b' ' | b'\t'])
    )
}
