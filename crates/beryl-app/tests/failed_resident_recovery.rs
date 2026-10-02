#![cfg(feature = "test-faults")]

#[path = "syndic_composer_history/support.rs"]
mod composer;
#[path = "syndic_composer_publication/support.rs"]
mod publication;
#[path = "resident_close_flush/support.rs"]
mod support;
#[path = "pending_composer_activation/support.rs"]
mod widget_support;

#[path = "failed_resident_recovery/candidate_markers.rs"]
mod candidate_markers;
#[path = "failed_resident_recovery/host.rs"]
mod host;
#[path = "failed_resident_recovery/marker.rs"]
mod marker;
#[path = "failed_resident_recovery/member.rs"]
mod member;
#[path = "failed_resident_recovery/original.rs"]
mod original;
#[path = "failed_resident_recovery/outcomes.rs"]
mod outcomes;
#[path = "failed_resident_recovery/resident.rs"]
mod resident;
