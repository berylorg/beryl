use super::{access::HistoryAccess, projection, snapshot, transcript};
use crate::support::{self, exact_cas};
use beryl_home_store::{CursorReadLimits, HomeStore, WholeHomeScrubTrigger};
use beryl_model::{CasItemId, SyndicItemId, SyndicThreadId, SyndicTurnId};
use syndic_storage::{
    CreateThread, DraftEditHistoryPolicyV1, ProviderAgentMessageV1, ProviderFrameOrdinalV1,
    ProviderItemDeltaV1, ProviderItemFrameV1, ProviderItemLifecycle, ProviderItemObservationV1,
    ProviderItemV1, ProviderLifecycleTimestampMsV1, ProviderTextV1, SourceEventPayload,
    SyndicPointReadLimit, SyndicStorage, TranscriptViewEntryRecord, TurnEndStatus, TurnLifecycle,
};

fn limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(1_000_000).unwrap()
}

fn project(
    home: &HomeStore,
    storage: &SyndicStorage,
    thread: SyndicThreadId,
    item: SyndicItemId,
) -> usize {
    let access = HistoryAccess::Ordinary(home);
    for commands in 0..1_024 {
        let current = snapshot::item_projection(access, storage, thread, item, limit()).unwrap();
        let before = home.home_revision().unwrap();
        let result = projection::step(access, storage, &current).unwrap();
        match result {
            projection::ItemProjectionStep::Current => {
                assert_eq!(home.home_revision().unwrap(), before);
                return commands;
            }
            projection::ItemProjectionStep::Advanced => {
                assert_eq!(
                    home.home_revision().unwrap(),
                    before.checked_next().unwrap()
                );
                home.scrub_whole_home(WholeHomeScrubTrigger::Explicit)
                    .unwrap();
            }
        }
    }
    panic!("app item projection exceeded its fixture construction bound");
}

fn entries(
    home: &HomeStore,
    storage: &SyndicStorage,
    thread: SyndicThreadId,
) -> Vec<TranscriptViewEntryRecord> {
    transcript::converge_selected_transcript(
        HistoryAccess::Ordinary(home),
        storage,
        thread,
        limit(),
    )
    .unwrap();
    let head = storage
        .transcript_view_head(home, thread, limit())
        .unwrap()
        .unwrap();
    let page = storage
        .transcript_entries(
            home,
            thread,
            head.generation(),
            None,
            CursorReadLimits::new(16, 65_536).unwrap(),
        )
        .unwrap();
    assert!(!page.has_more());
    page.records().to_vec()
}

fn project_live_turn(
    home: &HomeStore,
    storage: &SyndicStorage,
    thread: SyndicThreadId,
    turn: SyndicTurnId,
) -> usize {
    let mut progress = super::live::LiveHistoryProgress::default();
    assert!(!progress.pending());
    progress.restart();
    let mut commands = 0;
    for _ in 0..1_024 {
        let before = home.home_revision().unwrap();
        progress.step(home, storage, thread, turn, limit()).unwrap();
        let after = home.home_revision().unwrap();
        assert!(after == before || after == before.checked_next().unwrap());
        commands += usize::from(after != before);
        if !progress.pending() {
            progress.step(home, storage, thread, turn, limit()).unwrap();
            assert_eq!(home.home_revision().unwrap(), after);
            home.scrub_whole_home(WholeHomeScrubTrigger::Explicit)
                .unwrap();
            return commands;
        }
    }
    panic!("live history exceeded its bounded fixture steps");
}

fn agent(text: &str) -> ProviderItemV1 {
    ProviderItemV1::AgentMessage(ProviderAgentMessageV1 {
        text: ProviderTextV1::inline(text),
        phase: None,
        memory_citation: None,
    })
}

#[test]
fn app_projection_steps_publish_live_code_then_handoff_to_terminal_convergence() {
    let directory = support::TestHome::new("app-streaming-item-projection");
    let mut candidate = support::open(directory.path());
    let storage = SyndicStorage::register(&mut candidate).unwrap();
    let home = candidate
        .prepare_publication(SyndicStorage::required_domains().unwrap())
        .unwrap()
        .publish()
        .unwrap();
    let thread = support::id(81);
    let mut command = beryl_home_store::HomeCommand::new(home.home_revision().unwrap());
    command
        .add(storage.clone().create_thread(
            storage.revision(&home).unwrap(),
            CreateThread::ordinary(
                thread,
                support::draft_id(82),
                exact_cas::execution_binding(),
                support::timestamp(1),
                DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
            ),
        ))
        .unwrap();
    assert!(matches!(
        home.execute(command),
        beryl_home_store::CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    let user = SyndicItemId::from_bytes([83; 16]);
    let turn = exact_cas::submit_current_draft(
        &home,
        storage.clone(),
        thread,
        support::draft_id(84),
        user,
        "show code",
        support::timestamp(3),
    );
    let source =
        exact_cas::establish_turn(&home, storage.clone(), thread, turn, support::timestamp(4));
    exact_cas::admit_event(
        &home,
        storage.clone(),
        thread,
        turn,
        &source,
        SourceEventPayload::TurnActivated,
        support::timestamp(4),
    );
    exact_cas::correlate_user_item(
        &home,
        storage.clone(),
        thread,
        turn,
        user,
        &source,
        support::timestamp(5),
    );
    project(&home, &storage, thread, user);
    let assistant = SyndicItemId::from_bytes([85; 16]);
    let cas_item = CasItemId::new("app-streaming-code").unwrap();
    let initial = format!("```rust\n{}", "let value = 1;\n".repeat(8_000));
    exact_cas::admit_item_frame(
        &home,
        storage.clone(),
        thread,
        turn,
        assistant,
        &source,
        ProviderItemFrameV1::new(
            ProviderFrameOrdinalV1::FIRST,
            cas_item.clone(),
            ProviderItemObservationV1::Started {
                observed_at: ProviderLifecycleTimestampMsV1::new(6),
                item: agent(&initial),
            },
        ),
        support::timestamp(6),
    );
    assert!(project_live_turn(&home, &storage, thread, turn) > 2);
    let published = home.home_revision().unwrap();
    let first = entries(&home, &storage, thread);
    assert_eq!(home.home_revision().unwrap(), published);
    assert_eq!(first.len(), 2);
    let code = storage
        .projection(&home, first[1].projection_id(), limit())
        .unwrap()
        .unwrap();
    assert!(code.payload().resource_id().is_some());
    let state = storage.turn_state(&home, turn, limit()).unwrap().unwrap();
    assert_eq!(state.lifecycle(), TurnLifecycle::Active);
    assert_eq!(state.finalized_item_count(), 0);
    assert_eq!(
        storage
            .canonical_item(&home, assistant, limit())
            .unwrap()
            .unwrap()
            .provider_lifecycle(),
        ProviderItemLifecycle::Started
    );
    assert!(
        !storage
            .history_summary(&home, thread, limit())
            .unwrap()
            .unwrap()
            .complete()
    );

    let delta = "let answer = 42;\n";
    exact_cas::admit_item_frame(
        &home,
        storage.clone(),
        thread,
        turn,
        assistant,
        &source,
        ProviderItemFrameV1::new(
            ProviderFrameOrdinalV1::new(2).unwrap(),
            cas_item.clone(),
            ProviderItemObservationV1::Delta(ProviderItemDeltaV1::AgentMessage {
                delta: ProviderTextV1::inline(delta),
            }),
        ),
        support::timestamp(7),
    );
    let access = HistoryAccess::Ordinary(&home);
    let pending = snapshot::item_projection(access, &storage, thread, assistant, limit()).unwrap();
    assert_eq!(
        projection::step(access, &storage, &pending).unwrap(),
        projection::ItemProjectionStep::Advanced
    );
    let interrupted =
        snapshot::item_projection(access, &storage, thread, assistant, limit()).unwrap();
    assert!(interrupted.build.is_some());
    let more = "let extra = 7;\n";
    exact_cas::admit_item_frame(
        &home,
        storage.clone(),
        thread,
        turn,
        assistant,
        &source,
        ProviderItemFrameV1::new(
            ProviderFrameOrdinalV1::new(3).unwrap(),
            cas_item.clone(),
            ProviderItemObservationV1::Delta(ProviderItemDeltaV1::AgentMessage {
                delta: ProviderTextV1::inline(more),
            }),
        ),
        support::timestamp(8),
    );
    let before_refusal = home.home_revision().unwrap();
    assert!(matches!(
        projection::step(access, &storage, &interrupted),
        Err(crate::cas_projection::OrdinaryTurnExecutionError::HomeCommandNotCommitted(_))
    ));
    assert_eq!(home.home_revision().unwrap(), before_refusal);
    home.scrub_whole_home(WholeHomeScrubTrigger::Explicit)
        .unwrap();
    let successor =
        snapshot::item_projection(access, &storage, thread, assistant, limit()).unwrap();
    assert!(successor.generation > interrupted.generation);
    assert_eq!(
        projection::step(access, &storage, &successor).unwrap(),
        projection::ItemProjectionStep::Advanced
    );
    let interrupted_again =
        snapshot::item_projection(access, &storage, thread, assistant, limit()).unwrap();
    let closing = "```\n";
    exact_cas::admit_item_frame(
        &home,
        storage.clone(),
        thread,
        turn,
        assistant,
        &source,
        ProviderItemFrameV1::new(
            ProviderFrameOrdinalV1::new(4).unwrap(),
            cas_item.clone(),
            ProviderItemObservationV1::Delta(ProviderItemDeltaV1::AgentMessage {
                delta: ProviderTextV1::inline(closing),
            }),
        ),
        support::timestamp(9),
    );
    let before_second_refusal = home.home_revision().unwrap();
    assert!(matches!(
        projection::step(access, &storage, &interrupted_again),
        Err(crate::cas_projection::OrdinaryTurnExecutionError::HomeCommandNotCommitted(_))
    ));
    assert_eq!(home.home_revision().unwrap(), before_second_refusal);
    let latest = storage
        .latest_item_projection_build(&home, assistant, limit())
        .unwrap()
        .unwrap();
    assert_eq!(latest.generation(), interrupted_again.generation);
    assert!(matches!(
        latest.phase(),
        syndic_storage::ItemProjectionBuildPhase::Superseded(_)
    ));
    assert!(
        storage
            .latest_item_projection_build(&home, assistant, SyndicPointReadLimit::new(1).unwrap())
            .is_err()
    );
    home.close().unwrap();
    let mut candidate = support::open(directory.path());
    let storage = SyndicStorage::register(&mut candidate).unwrap();
    let mut publication = candidate
        .prepare_publication(SyndicStorage::required_domains().unwrap())
        .unwrap();
    {
        let candidate_access = publication.recovery_access().unwrap();
        assert_eq!(
            storage
                .latest_item_projection_build_candidate(&candidate_access, assistant, limit())
                .unwrap()
                .unwrap(),
            latest
        );
        assert!(
            storage
                .latest_item_projection_build_candidate(
                    &candidate_access,
                    assistant,
                    SyndicPointReadLimit::new(1).unwrap()
                )
                .is_err()
        );
        let access = HistoryAccess::Candidate(&candidate_access);
        let next = snapshot::item_projection(access, &storage, thread, assistant, limit()).unwrap();
        assert!(next.generation > interrupted_again.generation);
        assert_eq!(
            projection::step(access, &storage, &next).unwrap(),
            projection::ItemProjectionStep::Advanced
        );
    }
    let home = publication.publish().unwrap();
    project(&home, &storage, thread, assistant);
    let second = entries(&home, &storage, thread);
    assert_eq!(second.len(), 2);
    assert!(second[1].item_revision() > first[1].item_revision());
    assert_eq!(
        storage
            .turn_state(&home, turn, limit())
            .unwrap()
            .unwrap()
            .finalized_item_count(),
        0
    );

    exact_cas::admit_item_frame(
        &home,
        storage.clone(),
        thread,
        turn,
        assistant,
        &source,
        ProviderItemFrameV1::new(
            ProviderFrameOrdinalV1::new(5).unwrap(),
            cas_item,
            ProviderItemObservationV1::Completed {
                observed_at: ProviderLifecycleTimestampMsV1::new(10),
                item: agent(&format!("{initial}{delta}{more}{closing}")),
            },
        ),
        support::timestamp(10),
    );
    project(&home, &storage, thread, assistant);
    let completed = entries(&home, &storage, thread);
    assert_eq!(completed[1].projection_id(), second[1].projection_id());
    exact_cas::admit_event(
        &home,
        storage.clone(),
        thread,
        turn,
        &source,
        SourceEventPayload::TurnEnded(TurnEndStatus::complete()),
        support::timestamp(11),
    );
    super::converge_terminal_history(
        &home,
        &storage,
        thread,
        turn,
        support::timestamp(11),
        limit(),
        None,
    )
    .unwrap();
    let terminal = entries(&home, &storage, thread);
    assert_eq!(terminal[1].projection_id(), completed[1].projection_id());
    assert!(
        storage
            .history_summary(&home, thread, limit())
            .unwrap()
            .unwrap()
            .complete()
    );
    home.scrub_whole_home(WholeHomeScrubTrigger::Explicit)
        .unwrap();
    home.close().unwrap();
}
