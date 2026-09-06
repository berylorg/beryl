#![cfg(feature = "test-faults")]

#[path = "syndic_composer_host/support.rs"]
#[allow(dead_code, unused_imports)]
mod base;
#[path = "syndic_composer_history/support.rs"]
#[allow(dead_code)]
mod composer;
#[path = "composer_lifecycle/common.rs"]
#[allow(dead_code)]
mod lifecycle;
#[path = "syndic_composer_publication/support.rs"]
#[allow(dead_code)]
mod publication;

#[path = "saved_opening/flush.rs"]
mod flush;
#[path = "saved_opening/submission.rs"]
mod submission;
#[path = "saved_opening/support.rs"]
mod support;
