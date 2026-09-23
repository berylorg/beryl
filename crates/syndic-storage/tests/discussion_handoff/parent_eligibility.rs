use super::*;
use beryl_home_store::CursorReadLimits;
use beryl_model::SyndicItemId;
use support::{draft_id, exact_cas};
use syndic_storage::test_faults::FixtureRecord;

fn admit_request(
    store: &HomeStore,
    storage: &SyndicStorage,
    request: AdmitDiscussionHandoff,
) -> DiscussionParentRequest {
    let prepared = storage
        .prepare_discussion_handoff(store, DiscussionHandoffMutation::Admit(request.clone()))
        .unwrap();
    let result = DiscussionParentRequest {
        child_gate: prepared.intent().new_gate(),
        parent_thread_id: request.parent.thread_id,
        context_owner: request.context_owner,
        context_digest: request.context_digest,
    };
    execute(store, prepared);
    result
}

fn proven(
    store: &HomeStore,
    storage: &SyndicStorage,
    request: DiscussionParentRequest,
) -> PreparedDiscussionParent {
    let DiscussionParentEligibility::Proven(proof) =
        storage.prepare_discussion_parent(store, request).unwrap()
    else {
        panic!("expected parent proof");
    };
    proof
}

#[path = "parent_eligibility/draft.rs"]
mod draft;
#[path = "parent_eligibility/nested.rs"]
mod nested;
#[path = "parent_eligibility/ordinary.rs"]
mod ordinary;

use support::discussion_creation::create_child;
