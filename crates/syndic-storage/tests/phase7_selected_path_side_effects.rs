#![cfg(feature = "test-faults")]

mod support;

use beryl_home_store::{CursorReadLimits, HomeCommand, HomeStore};
use beryl_model::{CasItemId, SyndicDraftId, SyndicItemId, SyndicThreadId, SyndicTurnId};
use syndic_storage::*;

use support::{
    converge_and_release_terminal_history, draft_id,
    exact_cas::{
        admit_event, admit_item_frame, correlate_user_item, establish_turn, submit_current_draft,
    },
    id, open, timestamp, TestHome,
};

const PAGE_BYTES: usize = 4_096;

#[path = "phase7_selected_path_side_effects/fixture.rs"]
mod fixture;
#[path = "phase7_selected_path_side_effects/helpers.rs"]
mod helpers;
#[path = "phase7_selected_path_side_effects/off_path.rs"]
mod off_path;
#[path = "phase7_selected_path_side_effects/selected_path.rs"]
mod selected_path;

use fixture::*;
use helpers::*;
