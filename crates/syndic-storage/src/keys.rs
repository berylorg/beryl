use std::ops::Range;

use crate::{
    CasProjectionBindingId, ConversationId, CursorId, ItemId, ProjectionRecordId, RecoveryMarkerId,
    ResourceId, SourceEventId, ThreadViewId, TranscriptViewPosition, TranscriptViewRecordId,
    TurnId,
};

const SEP: u8 = 0;

pub(crate) fn simple_id_key(value: &str) -> Vec<u8> {
    value.as_bytes().to_vec()
}

pub(crate) fn conversation_key(id: &ConversationId) -> Vec<u8> {
    simple_id_key(id.as_str())
}

pub(crate) fn conversation_view_key(view_id: &ThreadViewId) -> Vec<u8> {
    simple_id_key(view_id.as_str())
}

pub(crate) fn conversation_source_thread_key(
    provider: &str,
    runtime_target: Option<&str>,
    external_thread_id: &str,
) -> Vec<u8> {
    let mut key = Vec::new();
    push_segment(&mut key, provider);
    push_segment(&mut key, runtime_target.unwrap_or_default());
    push_segment(&mut key, external_thread_id);
    key
}

pub(crate) fn turn_key(id: &TurnId) -> Vec<u8> {
    simple_id_key(id.as_str())
}

pub(crate) fn item_key(id: &ItemId) -> Vec<u8> {
    simple_id_key(id.as_str())
}

pub(crate) fn source_event_id_key(id: &SourceEventId) -> Vec<u8> {
    simple_id_key(id.as_str())
}

pub(crate) fn projection_key(id: &ProjectionRecordId) -> Vec<u8> {
    simple_id_key(id.as_str())
}

pub(crate) fn resource_key(id: &ResourceId) -> Vec<u8> {
    simple_id_key(id.as_str())
}

pub(crate) fn cursor_key(id: &CursorId) -> Vec<u8> {
    simple_id_key(id.as_str())
}

pub(crate) fn recovery_marker_key(id: &RecoveryMarkerId) -> Vec<u8> {
    simple_id_key(id.as_str())
}

pub(crate) fn cas_projection_binding_key(id: &CasProjectionBindingId) -> Vec<u8> {
    simple_id_key(id.as_str())
}

pub(crate) fn cas_projection_binding_view_key(view_id: &ThreadViewId) -> Vec<u8> {
    simple_id_key(view_id.as_str())
}

pub(crate) fn revision_key(view_id: &ThreadViewId) -> Vec<u8> {
    simple_id_key(view_id.as_str())
}

pub(crate) fn source_event_sequence_key(turn_id: &TurnId, sequence: u64) -> Vec<u8> {
    let mut key = scoped_prefix(turn_id.as_str());
    key.extend_from_slice(&sequence.to_be_bytes());
    key
}

pub(crate) fn source_event_head_key(turn_id: &TurnId) -> Vec<u8> {
    simple_id_key(turn_id.as_str())
}

pub(crate) fn source_event_prefix(turn_id: &TurnId) -> Vec<u8> {
    scoped_prefix(turn_id.as_str())
}

pub(crate) fn transcript_view_key(
    view_id: &ThreadViewId,
    position: TranscriptViewPosition,
    record_id: &TranscriptViewRecordId,
) -> Vec<u8> {
    let mut key = transcript_view_position_prefix(view_id, position);
    key.extend_from_slice(record_id.as_str().as_bytes());
    key
}

pub(crate) fn transcript_view_position_prefix(
    view_id: &ThreadViewId,
    position: TranscriptViewPosition,
) -> Vec<u8> {
    let mut key = scoped_prefix(view_id.as_str());
    key.extend_from_slice(&position.0.to_be_bytes());
    key.push(SEP);
    key
}

pub(crate) fn transcript_view_prefix(view_id: &ThreadViewId) -> Vec<u8> {
    scoped_prefix(view_id.as_str())
}

pub(crate) fn prefix_range(prefix: &[u8]) -> Range<Vec<u8>> {
    let mut upper = prefix.to_vec();
    for byte in upper.iter_mut().rev() {
        if *byte < u8::MAX {
            *byte += 1;
            return prefix.to_vec()..upper;
        }
    }

    prefix.to_vec()..{
        let mut fallback = prefix.to_vec();
        fallback.push(u8::MAX);
        fallback
    }
}

pub(crate) fn exclusive_after(mut key: Vec<u8>) -> Vec<u8> {
    key.push(SEP);
    key
}

fn scoped_prefix(value: &str) -> Vec<u8> {
    let mut key = Vec::with_capacity(value.len() + 1);
    key.extend_from_slice(value.as_bytes());
    key.push(SEP);
    key
}

fn push_segment(key: &mut Vec<u8>, value: &str) {
    key.extend_from_slice(&(value.len() as u64).to_be_bytes());
    key.extend_from_slice(value.as_bytes());
}
