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
    let revision = beryl_model::ProjectionRevision::new(
        head.revision().get().max(summary.revision().get()) + 1,
    )
    .unwrap();
    let mut batch = FixtureBatch::new();
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
                true,
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
    ProviderRevision(revision.get())
}
