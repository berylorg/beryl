use super::*;
use beryl_model::{CasItemId, SyndicItemId};
use syndic_storage::*;

pub(super) fn child() -> SyndicThreadId {
    SyndicThreadId::from_bytes([248; 16])
}

pub(super) fn prepare_home(path: &std::path::Path) {
    prepare_home_with_child_chunks(path, 1);
}

pub(super) fn prepare_taller_child_home(path: &std::path::Path) {
    prepare_home_with_child_chunks(path, 2);
}

fn prepare_home_with_child_chunks(path: &std::path::Path, child_chunks: usize) {
    eprintln!(
        "original ordinary claim activation Home fixture: {}",
        path.display()
    );
    let mut candidate =
        HomeOpenCandidate::open(HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT)).unwrap();
    let state = beryl_state::BerylState::register(&mut candidate).unwrap();
    let storage = SyndicStorage::register(&mut candidate).unwrap();
    let home = candidate
        .prepare_publication(
            beryl_state::BerylState::required_domains()
                .unwrap()
                .merge(SyndicStorage::required_domains().unwrap())
                .unwrap(),
        )
        .unwrap()
        .publish()
        .unwrap();
    let selected = state.session().minimal_bootstrap(&home).unwrap().unwrap();
    let prior_execution = storage
        .thread_execution(
            &home,
            selected.windows()[0].selected_thread().unwrap().thread_id(),
            SyndicPointReadLimit::new(65_536).unwrap(),
        )
        .unwrap()
        .unwrap()
        .execution()
        .clone();
    let validator = crate::runtime_admission::validation::RuntimePathValidator::new(
        crate::cas_projection::RuntimeTokenDirectory::from_admitted(
            beryl_model::AdmittedHostPath::from_admitted(
                beryl_model::PathFlavor::Windows,
                path.to_str().unwrap(),
            )
            .unwrap(),
        ),
        crate::runtime_admission::validation::ValidationLimits::default(),
        None,
    )
    .unwrap();
    let root = validator
        .resolve_root(
            path,
            &beryl_model::RuntimeMode::host(),
            &CommandCancellation::new(),
        )
        .unwrap();
    let root_id = beryl_model::RootId::from_bytes([253; 16]);
    let mut registration = HomeCommand::new(home.home_revision().unwrap());
    registration
        .add(
            state.runtime_roots().add_root(
                state.runtime_roots().revision(&home).unwrap(),
                beryl_state::AddConfiguredRoot::new(
                    prior_execution.runtime_id(),
                    beryl_state::RootRegistration::new(
                        root_id,
                        root.runtime_native_path().clone(),
                        root.display_path().clone(),
                        beryl_state::UnixMillis::new(1),
                        beryl_state::AvailabilitySnapshot::observed(
                            beryl_model::Availability::Available,
                            beryl_state::UnixMillis::new(1),
                        )
                        .unwrap(),
                    ),
                ),
            ),
        )
        .unwrap();
    assert!(matches!(
        home.execute(registration),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    let admitted = state
        .runtime_roots()
        .catalog_source(&home, prior_execution.runtime_id(), root_id)
        .unwrap();
    assert_eq!(admitted.root().canonical_path(), root.runtime_native_path());
    assert_eq!(
        admitted.root().availability().availability(),
        beryl_model::Availability::Available
    );
    assert_eq!(
        admitted.runtime().availability().availability(),
        beryl_model::Availability::Available
    );
    let execution = beryl_model::ExecutionBinding::new(
        prior_execution.runtime_id(),
        root_id,
        root.runtime_native_path().clone(),
    );
    let parent = SyndicThreadId::from_bytes([242; 16]);
    let mut creation = HomeCommand::new(home.home_revision().unwrap());
    creation
        .add(storage.create_thread(
            storage.revision(&home).unwrap(),
            CreateThread::ordinary(
                parent,
                SyndicDraftId::from_bytes([243; 16]),
                execution.clone(),
                SyndicTimestamp::from_unix_millis(1),
                DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
            ),
        ))
        .unwrap();
    assert!(matches!(
        home.execute(creation),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    assert_eq!(
        storage
            .thread_execution(&home, parent, SyndicPointReadLimit::new(65_536).unwrap())
            .unwrap()
            .unwrap()
            .execution(),
        &execution
    );
    super::transcript::prepare_history(&home, &storage, parent);
    assert_eq!(
        resident_fixture::composer_support::seed_published_draft_chunks(&storage, &home, parent, 1),
        768
    );
    let (turn, item) = append_assistant(&home, &storage, parent);
    let limit = SyndicPointReadLimit::new(65_536).unwrap();
    let thread = storage.thread(&home, parent, limit).unwrap().unwrap();
    let head = storage
        .transcript_view_head(&home, parent, limit)
        .unwrap()
        .unwrap();
    let entries = storage
        .transcript_entries(
            &home,
            parent,
            head.generation(),
            None,
            beryl_home_store::CursorReadLimits::new(32, 65_536).unwrap(),
        )
        .unwrap();
    assert_eq!(entries.records().len(), 3);
    let entry = entries
        .records()
        .iter()
        .find(|entry| entry.item_id() == item)
        .unwrap();
    let source = storage
        .prepare_discussion_source(
            &home,
            DiscussionContextSource::new(
                parent,
                turn,
                item,
                entry.projection_id(),
                entry.projection_revision(),
                DiscussionContextRange::new(0, ASSISTANT_TEXT.len() as u64).unwrap(),
            ),
            thread.selected_path(),
            CurrentTranscriptEntryProof::new(head.generation(), entry.position()),
            DiscussionContextText::new(ASSISTANT_TEXT).unwrap(),
        )
        .unwrap();
    let prepared = storage
        .prepare_discussion_creation(
            &home,
            source,
            CreateDiscussion::new(
                child(),
                SyndicDraftId::from_bytes([249; 16]),
                SyndicTimestamp::from_unix_millis(220),
                DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
            ),
        )
        .unwrap();
    let mut command = HomeCommand::new(home.home_revision().unwrap());
    command.add(prepared.contribution()).unwrap();
    assert!(matches!(
        home.execute(command),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    let child_head = storage
        .thread_lineage_head(&home, child(), limit)
        .unwrap()
        .unwrap();
    assert_eq!(child_head.total_parent_count(), 1);
    let lineage = storage
        .thread_lineage_page(
            &home,
            &child_head,
            child_head.cursor().unwrap(),
            beryl_home_store::CursorReadLimits::new(1, 65_536).unwrap(),
        )
        .unwrap();
    assert_eq!(lineage.records()[0].thread_id(), parent);
    crate::support::exact_cas::converge_transcript(&home, storage.clone(), child());
    let child_thread = storage.thread(&home, child(), limit).unwrap().unwrap();
    let child_transcript = storage
        .transcript_view_head(&home, child(), limit)
        .unwrap()
        .unwrap();
    assert_eq!(child_transcript.lifecycle(), ProjectionLifecycle::Current);
    assert_eq!(child_transcript.entry_count(), 3);
    assert_eq!(
        child_transcript.committed_tail(),
        child_thread.committed_tail()
    );
    assert_eq!(
        child_transcript.selected_path_digest(),
        child_thread.selected_path_digest()
    );
    let child_entries = storage
        .transcript_entries(
            &home,
            child(),
            child_transcript.generation(),
            None,
            beryl_home_store::CursorReadLimits::new(32, 65_536).unwrap(),
        )
        .unwrap();
    assert_eq!(
        child_entries
            .records()
            .iter()
            .map(|entry| (
                entry.item_id(),
                entry.projection_id(),
                entry.projection_revision()
            ))
            .collect::<Vec<_>>(),
        entries
            .records()
            .iter()
            .map(|entry| (
                entry.item_id(),
                entry.projection_id(),
                entry.projection_revision()
            ))
            .collect::<Vec<_>>(),
    );
    assert!(
        storage
            .history_summary(&home, child(), limit)
            .unwrap()
            .unwrap()
            .complete()
    );
    assert_eq!(
        resident_fixture::composer_support::seed_published_draft_chunks(&storage, &home, parent, 1),
        768
    );
    assert_eq!(
        resident_fixture::composer_support::seed_published_draft_chunks(
            &storage,
            &home,
            child(),
            child_chunks
        ),
        (768 * child_chunks) as u64
    );
    for thread in [parent, child()] {
        match crate::catalog_projection::prepare_thread_catalog_projection(
            &home, &storage, &state, thread,
        )
        .unwrap()
        {
            crate::catalog_projection::ThreadCatalogProjectionPreparation::Publish(command) => {
                assert!(matches!(
                    home.execute(command),
                    CommandOutcome::Committed {
                        later_failure: None,
                        ..
                    }
                ));
            }
            crate::catalog_projection::ThreadCatalogProjectionPreparation::ExactCurrent => {}
            crate::catalog_projection::ThreadCatalogProjectionPreparation::ThreadMissing => {
                panic!("real lineage fixture lost canonical thread")
            }
        }
        assert!(matches!(
            storage
                .authenticate_thread_catalog_summary(&home, thread)
                .unwrap(),
            FrozenThreadCatalogSummaryAuthentication::Current(_)
        ));
    }
    drop((storage, state));
    home.close().unwrap();
}

const ASSISTANT_TEXT: &str = "A genuine assistant discussion parent";

fn append_assistant(
    home: &beryl_home_store::HomeStore,
    storage: &SyndicStorage,
    parent: SyndicThreadId,
) -> (beryl_model::SyndicTurnId, SyndicItemId) {
    use crate::support::exact_cas::{
        admit_event, admit_started_then_completed_item, converge_and_release_terminal_history,
        correlate_user_item, establish_turn, submit_current_draft,
    };
    let user = SyndicItemId::from_bytes([250; 16]);
    let assistant = SyndicItemId::from_bytes([251; 16]);
    let timestamp = |value| SyndicTimestamp::from_unix_millis(value);
    let turn = submit_current_draft(
        home,
        storage.clone(),
        parent,
        SyndicDraftId::from_bytes([252; 16]),
        user,
        "Discuss the assistant response",
        timestamp(201),
    );
    let source = establish_turn(home, storage.clone(), parent, turn, timestamp(202));
    admit_event(
        home,
        storage.clone(),
        parent,
        turn,
        &source,
        SourceEventPayload::TurnActivated,
        timestamp(203),
    );
    correlate_user_item(
        home,
        storage.clone(),
        parent,
        turn,
        user,
        &source,
        timestamp(204),
    );
    let message = || {
        ProviderItemV1::AgentMessage(ProviderAgentMessageV1 {
            text: ProviderTextV1::inline(ASSISTANT_TEXT),
            phase: Some(ProviderMessagePhaseV1::FinalAnswer),
            memory_citation: None,
        })
    };
    admit_started_then_completed_item(
        home,
        storage.clone(),
        parent,
        turn,
        assistant,
        &source,
        CasItemId::new("lineage-assistant").unwrap(),
        message(),
        message(),
        timestamp(205),
        timestamp(206),
    );
    admit_event(
        home,
        storage.clone(),
        parent,
        turn,
        &source,
        SourceEventPayload::TurnEnded(TurnEndStatus::complete()),
        timestamp(207),
    );
    converge_and_release_terminal_history(home, storage.clone(), parent, turn);
    (turn, assistant)
}
