use beryl_model::Availability;
use beryl_state::{AvailabilitySnapshot, RecordRevision, UnixMillis, ValueError};

#[test]
fn availability_and_record_revisions_reject_ambiguous_zero_states() {
    assert!(matches!(
        AvailabilitySnapshot::observed(Availability::Unknown, UnixMillis::new(1)),
        Err(ValueError::UnknownAvailabilityObserved)
    ));
    assert!(matches!(
        RecordRevision::new(0),
        Err(ValueError::ZeroRecordRevision)
    ));
}
