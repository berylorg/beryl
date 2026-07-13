use beryl_model::{Availability, ThreadRevision};
use beryl_state::{
    AvailabilitySnapshot, GeneratedTitle, RecordRevision, TokenUsageBreakdown, TokenUsageSnapshot,
    UnixMillis, ValueError,
};

#[test]
fn generated_titles_enforce_the_exact_text_budget() {
    let revision = ThreadRevision::new(1).unwrap();
    assert!(GeneratedTitle::new("x".repeat(512), revision, UnixMillis::new(1)).is_ok());
    assert!(matches!(
        GeneratedTitle::new("x".repeat(513), revision, UnixMillis::new(1)),
        Err(ValueError::TooLong { maximum: 512, .. })
    ));
    assert!(matches!(
        GeneratedTitle::new(" padded", revision, UnixMillis::new(1)),
        Err(ValueError::SurroundingWhitespace { .. })
    ));
    assert!(matches!(
        GeneratedTitle::new("line\nbreak", revision, UnixMillis::new(1)),
        Err(ValueError::ControlCharacter { .. })
    ));
}

#[test]
fn availability_and_usage_values_reject_ambiguous_zero_states() {
    assert!(matches!(
        AvailabilitySnapshot::observed(Availability::Unknown, UnixMillis::new(1)),
        Err(ValueError::UnknownAvailabilityObserved)
    ));
    assert!(matches!(
        TokenUsageSnapshot::new(
            TokenUsageBreakdown::default(),
            TokenUsageBreakdown::default(),
            Some(0),
            ThreadRevision::new(1).unwrap(),
            UnixMillis::new(1),
        ),
        Err(ValueError::ZeroModelContextWindow)
    ));
    assert!(matches!(
        RecordRevision::new(0),
        Err(ValueError::ZeroRecordRevision)
    ));
}
