#![cfg(feature = "test-faults")]

#[path = "generated_media_admissions/lifecycle.rs"]
mod lifecycle;
#[path = "generated_media_admissions/outcomes.rs"]
mod outcomes;
#[path = "generated_media_admissions/refusals.rs"]
mod refusals;
#[path = "generated_media_admissions/support.rs"]
mod support;

use beryl_home_store::{
    CommandError, CommandOutcome, CommandParticipantKind, HomeCommand, ReconciliationResolution,
    test_faults::FaultPoint,
};
use beryl_model::{GeneratedMediaUnavailableReason, SyndicResourceId};
use beryl_state::{
    AssetMediaType, GeneratedMediaCommandFootprint, GeneratedMediaError, GeneratedMediaFacts,
    GeneratedMediaLifecycle, GeneratedMediaTarget,
};
use support::*;
