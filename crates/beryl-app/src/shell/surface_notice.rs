use std::{
    collections::{HashSet, VecDeque},
    sync::atomic::{AtomicU64, Ordering},
};

use beryl_backend::{TurnError, TurnInfo, TurnStatus, TurnStreamEvent};

const MAX_SURFACE_NOTICES: usize = 8;
pub(super) const MAX_TURN_ERROR_NOTICE_DETAIL_BYTES: usize = 128 * 1024;
pub(super) const MAX_REPORTED_SURFACE_NOTICE_SOURCE_KEYS: usize = MAX_SURFACE_NOTICES * 8;
static NEXT_SURFACE_NOTICE_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) enum SurfaceNoticeSourceKey {
    TurnError { thread_id: String, turn_id: String },
}

#[derive(Clone)]
pub(super) struct SurfaceNotice {
    id: u64,
    title: String,
    detail: String,
    source_key: Option<SurfaceNoticeSourceKey>,
}

#[derive(Clone, Default)]
pub(super) struct SurfaceNoticeQueue {
    notices: VecDeque<SurfaceNotice>,
    reported_source_keys: HashSet<SurfaceNoticeSourceKey>,
    reported_source_key_order: VecDeque<SurfaceNoticeSourceKey>,
}

impl SurfaceNotice {
    pub(super) fn new(title: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            id: NEXT_SURFACE_NOTICE_ID.fetch_add(1, Ordering::Relaxed),
            title: title.into(),
            detail: detail.into(),
            source_key: None,
        }
    }

    fn with_source_key(mut self, source_key: SurfaceNoticeSourceKey) -> Self {
        self.source_key = Some(source_key);
        self
    }

    pub(super) fn turn_error(
        detail: impl Into<String>,
        source_key: SurfaceNoticeSourceKey,
    ) -> Self {
        Self::new("Turn error", detail).with_source_key(source_key)
    }

    pub(super) fn id(&self) -> u64 {
        self.id
    }

    pub(super) fn title(&self) -> &str {
        &self.title
    }

    pub(super) fn detail(&self) -> &str {
        &self.detail
    }

    pub(super) fn selectable_text(&self) -> String {
        if self.detail.is_empty() {
            return self.title.clone();
        }
        format!("{}\n{}", self.title, self.detail)
    }
}

impl SurfaceNoticeQueue {
    pub(super) fn from_initial(notice: Option<SurfaceNotice>) -> Self {
        let mut queue = Self::default();
        if let Some(notice) = notice {
            queue.push(notice);
        }
        queue
    }

    pub(super) fn active(&self) -> Option<&SurfaceNotice> {
        self.notices.front()
    }

    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.notices.len()
    }

    #[cfg(test)]
    pub(super) fn reported_source_key_count(&self) -> usize {
        self.reported_source_keys.len()
    }

    pub(super) fn push(&mut self, notice: SurfaceNotice) -> bool {
        if let Some(source_key) = notice.source_key.as_ref() {
            if self.reported_source_keys.contains(source_key) {
                return false;
            }
            self.record_source_key(source_key.clone());
        }

        if self.notices.len() >= MAX_SURFACE_NOTICES {
            if self.notices.len() > 1 {
                self.notices.remove(1);
            } else {
                self.notices.pop_front();
            }
        }
        self.notices.push_back(notice);
        true
    }

    pub(super) fn dismiss_active(&mut self) -> bool {
        self.notices.pop_front().is_some()
    }

    pub(super) fn clear_all(&mut self) {
        self.notices.clear();
    }

    pub(super) fn clear_with_title(&mut self, title: &str) -> bool {
        let initial_len = self.notices.len();
        self.notices.retain(|notice| notice.title != title);
        self.notices.len() != initial_len
    }

    fn record_source_key(&mut self, source_key: SurfaceNoticeSourceKey) {
        let inserted = self.reported_source_keys.insert(source_key.clone());
        if !inserted {
            return;
        }

        self.reported_source_key_order.push_back(source_key);
        while self.reported_source_key_order.len() > MAX_REPORTED_SURFACE_NOTICE_SOURCE_KEYS {
            if let Some(expired_key) = self.reported_source_key_order.pop_front() {
                self.reported_source_keys.remove(&expired_key);
            }
        }
    }
}

pub(super) fn backend_turn_error_detail(error: Option<&TurnError>) -> String {
    let Some(error) = error else {
        return "The turn failed without an error payload from the backend.".to_string();
    };

    let primary = error.message.trim();
    let mut detail = if primary.is_empty() {
        "The turn failed without an error message.".to_string()
    } else {
        primary.to_string()
    };

    if let Some(additional) = error.additional_details.as_deref().map(str::trim)
        && !additional.is_empty()
    {
        if detail.is_empty() {
            detail.push_str(additional);
        } else {
            detail.push_str("\n\n");
            detail.push_str(additional);
        }
    }

    truncate_detail_to_byte_limit(&mut detail, MAX_TURN_ERROR_NOTICE_DETAIL_BYTES);
    detail
}

fn truncate_detail_to_byte_limit(detail: &mut String, max_bytes: usize) {
    if detail.len() <= max_bytes {
        return;
    }

    const SUFFIX: &str = "...";
    let suffix = if max_bytes >= SUFFIX.len() {
        SUFFIX
    } else {
        ""
    };
    let mut end = max_bytes.saturating_sub(suffix.len());
    while end > 0 && !detail.is_char_boundary(end) {
        end -= 1;
    }
    detail.truncate(end);
    detail.push_str(suffix);
}

pub(super) fn selected_backend_turn_error_notice(
    selected_thread_id: Option<&str>,
    event_thread_id: &str,
    turn: &TurnInfo,
) -> Option<SurfaceNotice> {
    if selected_thread_id != Some(event_thread_id) || turn.status != TurnStatus::Failed {
        return None;
    }

    Some(SurfaceNotice::turn_error(
        backend_turn_error_detail(turn.error.as_ref()),
        SurfaceNoticeSourceKey::TurnError {
            thread_id: event_thread_id.to_string(),
            turn_id: turn.id.clone(),
        },
    ))
}

pub(super) fn selected_active_turn_error_notice(
    selected_thread_id: Option<&str>,
    active_turn: Option<(&str, &str)>,
    notification_thread_id: &str,
    notification_turn_id: &str,
    error: &TurnError,
    will_retry: bool,
) -> Option<SurfaceNotice> {
    if will_retry
        || notification_thread_id.trim().is_empty()
        || notification_turn_id.trim().is_empty()
    {
        return None;
    }

    let (active_thread_id, active_turn_id) = active_turn?;
    if active_thread_id.trim().is_empty()
        || active_turn_id.trim().is_empty()
        || selected_thread_id != Some(active_thread_id)
        || notification_thread_id != active_thread_id
        || notification_turn_id != active_turn_id
    {
        return None;
    }

    Some(SurfaceNotice::turn_error(
        backend_turn_error_detail(Some(error)),
        SurfaceNoticeSourceKey::TurnError {
            thread_id: notification_thread_id.to_string(),
            turn_id: notification_turn_id.to_string(),
        },
    ))
}

pub(super) fn selected_turn_stream_error_notice(
    event: &TurnStreamEvent,
    selected_thread_id: Option<&str>,
    active_turn: Option<(&str, &str)>,
) -> Option<SurfaceNotice> {
    match event {
        TurnStreamEvent::TurnCompleted { thread_id, turn } => {
            selected_backend_turn_error_notice(selected_thread_id, thread_id, turn)
        }
        TurnStreamEvent::TurnError {
            thread_id,
            turn_id,
            error,
            will_retry,
        } => selected_active_turn_error_notice(
            selected_thread_id,
            active_turn,
            thread_id,
            turn_id,
            error,
            *will_retry,
        ),
        _ => None,
    }
}

pub(super) fn local_turn_failure_notice(message: impl Into<String>) -> SurfaceNotice {
    SurfaceNotice::new("Turn error", message)
}

pub(super) fn selected_active_stream_failure_notice(
    message: impl Into<String>,
    selected_thread_id: Option<&str>,
    active_turn: Option<(&str, &str)>,
) -> SurfaceNotice {
    let message = message.into();
    let Some((thread_id, turn_id)) = active_turn else {
        return local_turn_failure_notice(message);
    };
    if thread_id.trim().is_empty()
        || turn_id.trim().is_empty()
        || selected_thread_id != Some(thread_id)
    {
        return local_turn_failure_notice(message);
    }

    SurfaceNotice::turn_error(
        message,
        SurfaceNoticeSourceKey::TurnError {
            thread_id: thread_id.to_string(),
            turn_id: turn_id.to_string(),
        },
    )
}
