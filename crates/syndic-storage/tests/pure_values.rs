use beryl_model::{
    ProjectionRevision, SyndicItemId, SyndicProjectionId, SyndicThreadId, SyndicTurnId,
};
use syndic_storage::{
    ConversationParent, DISCUSSION_CONTEXT_MAX_BYTES, DiscussionContextEnvelope,
    DiscussionContextRange, DiscussionContextSource, DiscussionContextText, SyndicTimestamp,
    SyndicValueError,
};

fn context_source(range: DiscussionContextRange) -> DiscussionContextSource {
    DiscussionContextSource::new(
        SyndicThreadId::from_bytes([0; 16]),
        SyndicTurnId::from_bytes([1; 16]),
        SyndicItemId::from_bytes([2; 16]),
        SyndicProjectionId::from_bytes([3; 16]),
        ProjectionRevision::new(4).unwrap(),
        range,
    )
}

#[test]
fn discussion_text_preserves_exact_whitespace_and_newlines() {
    let text = DiscussionContextText::new("  selected\npassage  ").unwrap();

    assert_eq!(text.as_str(), "  selected\npassage  ");
    assert_eq!(text.len(), 20);
    assert!(!text.is_empty());
}

#[test]
fn discussion_text_and_range_enforce_the_exact_shared_limit() {
    assert!(DiscussionContextText::new("x".repeat(DISCUSSION_CONTEXT_MAX_BYTES)).is_ok());
    assert!(matches!(
        DiscussionContextText::new("x".repeat(DISCUSSION_CONTEXT_MAX_BYTES + 1)),
        Err(SyndicValueError::TextTooLong { .. })
    ));
    assert!(matches!(
        DiscussionContextText::new("bad\0text"),
        Err(SyndicValueError::NulByte { index: 3, .. })
    ));
    assert!(DiscussionContextRange::new(10, 10 + DISCUSSION_CONTEXT_MAX_BYTES as u64).is_ok());
    assert!(matches!(
        DiscussionContextRange::new(10, 11 + DISCUSSION_CONTEXT_MAX_BYTES as u64),
        Err(SyndicValueError::RangeTooLong { .. })
    ));
}

#[test]
fn context_descriptor_requires_text_and_source_range_agreement() {
    let text = DiscussionContextText::new("hello").unwrap();
    let range = DiscussionContextRange::new(100, 105).unwrap();
    let envelope = DiscussionContextEnvelope::new(
        context_source(range),
        text,
        SyndicTimestamp::from_unix_millis(6),
    )
    .unwrap();
    let descriptor = envelope.descriptor();

    assert_eq!(descriptor.source_range(), range);
    let changed = DiscussionContextEnvelope::new(
        context_source(range),
        DiscussionContextText::new("jello").unwrap(),
        SyndicTimestamp::from_unix_millis(6),
    )
    .unwrap();
    assert_ne!(descriptor.digest(), changed.descriptor().digest());
    assert!(matches!(
        DiscussionContextEnvelope::new(
            context_source(DiscussionContextRange::new(100, 106).unwrap()),
            DiscussionContextText::new("hello").unwrap(),
            SyndicTimestamp::from_unix_millis(6),
        ),
        Err(SyndicValueError::ContextLengthMismatch { .. })
    ));
}

#[test]
fn immutable_parent_descriptor_distinguishes_root_and_turn() {
    let turn = SyndicTurnId::from_bytes([8; 16]);

    assert_eq!(
        ConversationParent::from_turn(None),
        ConversationParent::Root
    );
    assert_eq!(ConversationParent::from_turn(Some(turn)).turn(), Some(turn));
}
