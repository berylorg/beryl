use super::*;
use crate::syndic_transcript::ProviderRevision;
use std::{cell::RefCell, rc::Rc};

pub(super) fn advance_history_revision(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    resource: Option<syndic_storage::ResourceMetadataRecord>,
) -> ProviderRevision {
    use syndic_storage::test_faults::{FixtureBatch, FixtureRecord};
    let owner = owner.borrow();
    let graph = owner.test_services().graph().unwrap();
    let home = graph.home();
    let storage = graph.syndic();
    let thread = graph
        .state()
        .session()
        .minimal_bootstrap(home)
        .unwrap()
        .unwrap()
        .windows()[0]
        .selected_thread()
        .unwrap()
        .thread_id();
    let point = syndic_storage::SyndicPointReadLimit::new(65536).unwrap();
    let head = storage
        .transcript_view_head(home, thread, point)
        .unwrap()
        .unwrap();
    let summary = storage
        .history_summary(home, thread, point)
        .unwrap()
        .unwrap();
    let current = storage.thread(home, thread, point).unwrap().unwrap();
    let build = storage
        .transcript_build(home, thread, head.generation(), point)
        .unwrap()
        .unwrap();
    assert_eq!(
        build.phase(),
        syndic_storage::TranscriptBuildPhase::Complete
    );
    assert_eq!(build.revision(), head.revision());
    assert_eq!(build.entry_count(), head.entry_count());
    let revision = beryl_model::ProjectionRevision::new(
        head.revision().get().max(summary.revision().get()) + 1,
    )
    .unwrap();
    let mut batch = FixtureBatch::new();
    batch
        .put(FixtureRecord::TranscriptBuild(
            syndic_storage::TranscriptBuildRecord::new(
                build.thread_id(),
                build.generation(),
                revision,
                build.source_thread_revision(),
                build.committed_tail(),
                build.selected_path_digest(),
                build.path_turn_count(),
                build.entry_count(),
                build.entry_digest(),
                build.history_complete(),
                build.phase(),
            ),
        ))
        .unwrap();
    if let Some(resource) = resource {
        let projection = storage
            .projection(home, resource.projection_id().unwrap(), point)
            .unwrap()
            .unwrap();
        let syndic_storage::ProjectionPayload::ResourceReference {
            block_id,
            block_kind,
            preview,
            ..
        } = projection.payload()
        else {
            panic!("resource owner projection")
        };
        batch
            .put(FixtureRecord::Projection(
                syndic_storage::ProjectionRecord::new(
                    projection.id(),
                    projection.revision(),
                    projection.item_id(),
                    projection.turn_id(),
                    projection.ordinal(),
                    syndic_storage::ProjectionPayload::resource_reference(
                        *block_id,
                        *block_kind,
                        resource.backing().range().unwrap(),
                        resource.id(),
                        preview,
                    )
                    .unwrap(),
                ),
            ))
            .unwrap();
        batch
            .put(FixtureRecord::ProjectionResource(
                syndic_storage::ProjectionResourceIndexRecord::new(
                    projection.id(),
                    resource.ordinal().unwrap(),
                    resource.id(),
                    resource.revision(),
                    *resource.digest().unwrap(),
                ),
            ))
            .unwrap();
        batch.put(FixtureRecord::Resource(resource)).unwrap();
    }
    batch
        .put(FixtureRecord::TranscriptViewHead(
            syndic_storage::TranscriptViewHeadRecord::new(
                thread,
                head.generation(),
                revision,
                head.entry_count(),
                current.committed_tail(),
                current.selected_path_digest(),
                syndic_storage::ProjectionLifecycle::Current,
            ),
        ))
        .unwrap();
    batch
        .put(FixtureRecord::HistorySummary(
            syndic_storage::HistorySummaryRecord::new(
                thread,
                revision,
                current.revision(),
                current.committed_tail(),
                current.selected_path_digest(),
                build.history_complete(),
                syndic_storage::SyndicTimestamp::from_unix_millis(2000),
            ),
        ))
        .unwrap();
    let mut command = beryl_home_store::HomeCommand::new(home.home_revision().unwrap());
    command
        .add(storage.fixture_contribution(storage.revision(home).unwrap(), batch))
        .unwrap();
    assert!(matches!(
        home.execute(command),
        beryl_home_store::CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    let refreshed = storage
        .transcript_build(home, thread, head.generation(), point)
        .unwrap()
        .unwrap();
    let refreshed_head = storage
        .transcript_view_head(home, thread, point)
        .unwrap()
        .unwrap();
    let refreshed_summary = storage
        .history_summary(home, thread, point)
        .unwrap()
        .unwrap();
    assert_eq!(refreshed.revision(), refreshed_head.revision());
    assert_eq!(refreshed.entry_count(), refreshed_head.entry_count());
    assert_eq!(refreshed.committed_tail(), refreshed_head.committed_tail());
    assert_eq!(
        refreshed.selected_path_digest(),
        refreshed_head.selected_path_digest()
    );
    assert_eq!(refreshed.history_complete(), refreshed_summary.complete());
    ProviderRevision(revision.get())
}
