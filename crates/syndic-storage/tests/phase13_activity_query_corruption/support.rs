use beryl_home_store::{CommandOutcome, HomeCommand, HomeStore};
use beryl_model::{CasItemId, SyndicItemId, SyndicThreadId, SyndicTurnId};
use syndic_storage::*;

use crate::support::{
    TestHome, converge_and_release_terminal_history, draft_id,
    exact_cas::{
        admit_event, admit_started_then_completed_item, correlate_user_item, establish_turn,
        submit_current_draft,
    },
    id, open, populated, timestamp,
};

pub struct PublishedChildHandoff {
    pub home: TestHome,
    pub store: HomeStore,
    pub storage: SyndicStorage,
    pub child: SyndicThreadId,
    pub child_turn: SyndicTurnId,
    pub final_answer: SyndicItemId,
    pub head: ActivityQueryHeadRecord,
}

pub struct ChildHandoffCandidate {
    pub home: TestHome,
    pub store: HomeStore,
    pub storage: SyndicStorage,
    pub owner: SyndicThreadId,
    pub child: SyndicThreadId,
    pub child_turn: SyndicTurnId,
    pub final_answer: SyndicItemId,
    pub head: ActivityQueryHeadRecord,
}

fn owner_item() -> SyndicItemId {
    SyndicItemId::from_bytes([
        0x91, 0x32, 0x73, 0xb4, 0xf5, 0x16, 0x57, 0x98, 0xd9, 0x3a, 0x7b, 0xbc, 0xfd, 0x1e, 0x5f,
        0xa0,
    ])
}

fn execute(store: &HomeStore, contribution: beryl_home_store::MutationContribution) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    match store.execute(command) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected clean activity-handoff fixture command, got {outcome:?}"),
    }
}

pub fn child_handoff_candidate(name: &str, later_activity: bool) -> ChildHandoffCandidate {
    let home = TestHome::new(name);
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    populated::seed_populated(&store, storage.clone());
    let owner = id(30);
    let child = id(36);
    converge_and_release_terminal_history(&store, storage.clone(), owner, populated::source_turn());
    submit_current_draft(
        &store,
        storage.clone(),
        owner,
        draft_id(220),
        owner_item(),
        "owner continues",
        timestamp(10),
    );
    let submitted_child_item = SyndicItemId::from_bytes([222; 16]);
    let child_turn = submit_current_draft(
        &store,
        storage.clone(),
        child,
        draft_id(223),
        submitted_child_item,
        "child question",
        timestamp(11),
    );
    let source = establish_turn(&store, storage.clone(), child, child_turn, timestamp(12));
    admit_event(
        &store,
        storage.clone(),
        child,
        child_turn,
        &source,
        SourceEventPayload::TurnActivated,
        timestamp(12),
    );
    correlate_user_item(
        &store,
        storage.clone(),
        child,
        child_turn,
        submitted_child_item,
        &source,
        timestamp(13),
    );
    let final_answer = SyndicItemId::from_bytes([224; 16]);
    let answer = ProviderItemV1::AgentMessage(ProviderAgentMessageV1 {
        text: ProviderTextV1::inline("final child"),
        phase: Some(ProviderMessagePhaseV1::FinalAnswer),
        memory_citation: None,
    });
    admit_started_then_completed_item(
        &store,
        storage.clone(),
        child,
        child_turn,
        final_answer,
        &source,
        CasItemId::new("phase13-corruption-final-answer").unwrap(),
        answer.clone(),
        answer,
        timestamp(14),
        timestamp(15),
    );
    if later_activity {
        admit_started_then_completed_item(
            &store,
            storage.clone(),
            child,
            child_turn,
            SyndicItemId::from_bytes([225; 16]),
            &source,
            CasItemId::new("phase13-corruption-later-activity").unwrap(),
            ProviderItemV1::StandaloneImageGeneration(ProviderImageGenerationV1 {
                status: ProviderImageGenerationStatusV1::InProgress,
                revised_prompt: None,
                saved_path: None,
            }),
            ProviderItemV1::StandaloneImageGeneration(ProviderImageGenerationV1 {
                status: ProviderImageGenerationStatusV1::Completed,
                revised_prompt: None,
                saved_path: Some(ProviderTextV1::inline("child.png")),
            }),
            timestamp(16),
            timestamp(17),
        );
    }
    admit_event(
        &store,
        storage.clone(),
        child,
        child_turn,
        &source,
        SourceEventPayload::TurnEnded(
            TurnEndStatus::new(
                TurnTerminalOutcome::Interrupted,
                Some(TurnIncompleteReason::ItemAuditFailed),
            )
            .unwrap(),
        ),
        timestamp(if later_activity { 18 } else { 16 }),
    );
    converge_and_release_terminal_history(&store, storage.clone(), child, child_turn);
    let head = storage
        .activity_query_head(&store, owner, SyndicPointReadLimit::new(1_000_000).unwrap())
        .unwrap()
        .unwrap()
        .clone();
    ChildHandoffCandidate {
        home,
        store,
        storage,
        owner,
        child,
        child_turn,
        final_answer,
        head,
    }
}

pub fn published_child_handoff(name: &str) -> PublishedChildHandoff {
    let candidate = child_handoff_candidate(name, false);
    execute(
        &candidate.store,
        candidate.storage.publish_activity_child_handoff(
            candidate.storage.revision(&candidate.store).unwrap(),
            PublishActivityChildHandoff::new(
                candidate.owner,
                candidate.head.revision(),
                candidate.child,
                candidate.child_turn,
                candidate.final_answer,
                ProjectionSourceRange::new(0, 11).unwrap(),
            ),
        ),
    );
    let head = candidate
        .storage
        .activity_query_head(
            &candidate.store,
            candidate.owner,
            SyndicPointReadLimit::new(1_000_000).unwrap(),
        )
        .unwrap()
        .unwrap()
        .clone();
    PublishedChildHandoff {
        home: candidate.home,
        store: candidate.store,
        storage: candidate.storage,
        child: candidate.child,
        child_turn: candidate.child_turn,
        final_answer: candidate.final_answer,
        head,
    }
}
