#![cfg(feature = "test-faults")]

#[path = "phase6_live_history/activity_bounds.rs"]
mod activity_bounds;
#[path = "phase6_live_history/activity_corruption.rs"]
mod activity_corruption;
#[path = "phase6_live_history/canonical.rs"]
mod canonical;
#[path = "phase6_live_history/mismatch.rs"]
mod mismatch;
mod support;

use beryl_home_store::{CommandError, CursorReadLimits, HomeCommand, HomeStore};
use beryl_model::{
    CasItemId, DraftRevision, InputGateRevision, SyndicContentId, SyndicItemId, SyndicThreadId,
    SyndicTurnId, ThreadRevision,
};
use syndic_storage::*;

use support::{
    converge_and_release_terminal_history, draft_id,
    exact_cas::{
        admit_event, admit_item_frame, correlate_user_item, establish_turn, submit_current_draft,
    },
    id, open, timestamp, TestHome,
};

#[path = "phase6_live_history/event_helpers.rs"]
mod event_helpers;
#[path = "phase6_live_history/terminal_outcomes.rs"]
mod terminal_outcomes;
#[path = "phase6_live_history/thread_isolation.rs"]
mod thread_isolation;
#[path = "phase6_live_history/turn_helpers.rs"]
mod turn_helpers;

use event_helpers::*;
use turn_helpers::*;
