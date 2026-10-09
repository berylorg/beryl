use super::*;
#[path = "transcript_provider/candidate_activation.rs"]
mod candidate_activation;
use crate::cas_projection::{
    MinimumTurnCaptureReserve, ProcessScheduledExecutionProvider, ProjectionConnectionService,
    ProjectionServiceConfig,
};
use beryl_home_store::{
    CommandOutcome, HomeCommand, HomeOpenCandidate, HomeOpenOptions, HomeSchemaVersion,
};
use beryl_model::{
    ExecutionBinding, PathFlavor, ProjectionRevision, RootId, RuntimeId, RuntimeMode,
    RuntimeNativePath, SyndicDraftId,
};
use beryl_state::BerylState;
use syndic_storage::{
    CreateThread, DraftEditHistoryPolicyV1, HistorySummaryRecord, ProjectionLifecycle,
    SyndicPointReadLimit, SyndicTimestamp, TranscriptGeneration, TranscriptViewHeadRecord,
    test_faults::{FixtureBatch, FixtureDelete, FixtureRecord},
};

struct Fixture {
    service: ProjectionConnectionService,
    reader: TranscriptProviderReader,
    home: Arc<HomeServiceReference>,
    storage: SyndicStorage,
    state: BerylState,
    graph: Arc<()>,
    host: Arc<()>,
    _directory: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let mut candidate = HomeOpenCandidate::open(HomeOpenOptions::new(
            directory.path(),
            HomeSchemaVersion::CURRENT,
        ))
        .unwrap();
        let state = BerylState::register(&mut candidate).unwrap();
        let storage = SyndicStorage::register(&mut candidate).unwrap();
        let home = candidate
            .prepare_publication(
                BerylState::required_domains()
                    .unwrap()
                    .merge(SyndicStorage::required_domains().unwrap())
                    .unwrap(),
            )
            .unwrap()
            .publish()
            .unwrap();
        let thread_id = SyndicThreadId::from_bytes([1; 16]);
        let binding = ExecutionBinding::new(
            RuntimeId::from_bytes([2; 16]),
            RootId::from_bytes([3; 16]),
            RuntimeNativePath::from_admitted(
                RuntimeMode::host(),
                PathFlavor::Windows,
                "C:\\Transcript",
            )
            .unwrap(),
        );
        execute(
            &home,
            storage.create_thread(
                storage.revision(&home).unwrap(),
                CreateThread::ordinary(
                    thread_id,
                    SyndicDraftId::from_bytes([4; 16]),
                    binding,
                    SyndicTimestamp::from_unix_millis(1),
                    DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
                ),
            ),
        );
        let retained = Arc::new(home.service_reference());
        let (provider, _) = ProcessScheduledExecutionProvider::new();
        let service = ProjectionConnectionService::new(
            Default::default(),
            home,
            storage.clone(),
            ProjectionServiceConfig::try_new(8, 4, MinimumTurnCaptureReserve::try_new(1).unwrap())
                .unwrap(),
            Box::new(provider),
        )
        .unwrap();
        let graph = Arc::new(());
        let host = Arc::new(());
        let reader = TranscriptProviderReader::new(
            Arc::clone(&retained),
            storage.clone(),
            Arc::downgrade(&graph),
            service.service_generation(),
            service.home_mutation_observer(),
        )
        .unwrap();
        let fixture = Self {
            service,
            reader,
            home: retained,
            storage,
            state,
            graph,
            host,
            _directory: directory,
        };
        fixture.seed_head(1);
        fixture
    }

    fn seed_head(&self, revision: u64) {
        self.seed_head_lifecycle(revision, ProjectionLifecycle::Current);
    }

    fn seed_head_lifecycle(&self, revision: u64, lifecycle: ProjectionLifecycle) {
        let thread_id = self.request(1).thread_id;
        let thread = self
            .storage
            .thread(
                &self.home,
                thread_id,
                SyndicPointReadLimit::new(65_536).unwrap(),
            )
            .unwrap()
            .unwrap();
        let revision = ProjectionRevision::new(revision).unwrap();
        let mut batch = FixtureBatch::new();
        batch
            .put(FixtureRecord::TranscriptViewHead(
                TranscriptViewHeadRecord::new(
                    thread_id,
                    TranscriptGeneration::new(1).unwrap(),
                    revision,
                    0,
                    thread.committed_tail(),
                    thread.selected_path_digest(),
                    lifecycle,
                ),
            ))
            .unwrap();
        batch
            .put(FixtureRecord::HistorySummary(HistorySummaryRecord::new(
                thread_id,
                revision,
                thread.revision(),
                thread.committed_tail(),
                thread.selected_path_digest(),
                true,
                SyndicTimestamp::from_unix_millis(1),
            )))
            .unwrap();
        execute(
            &self.home,
            self.storage
                .fixture_contribution(self.storage.revision(&self.home).unwrap(), batch),
        );
    }

    fn request(&self, request_id: u64) -> TranscriptAttachmentRequest {
        TranscriptAttachmentRequest {
            window_id: WindowId::from_bytes([5; 16]),
            host: Arc::downgrade(&self.host),
            activation: 1,
            thread_id: SyndicThreadId::from_bytes([1; 16]),
            request_id,
            placement: TranscriptActivationPlacement::Tail,
            purpose: TranscriptAttachmentPurpose::Attach,
        }
    }

    fn seed_projection(&self, text: &str, revision: u64) {
        use beryl_model::{
            ContentRevision, SyndicContentDigest, SyndicContentId, SyndicItemId,
            SyndicProjectionId, SyndicTurnId,
        };
        use syndic_storage::{
            CanonicalItemRecord, ContentEncoding, ContentReference, ContentSummary,
            ItemProjectionGeneration, MarkdownBlockId, MarkdownBlockKind, ProjectionOrdinal,
            ProjectionRecord, ProjectionSourceRange, TranscriptPosition, TranscriptViewEntryRecord,
            TurnItemOrdinal,
        };
        let thread_id = self.request(1).thread_id;
        let thread = self
            .storage
            .thread(
                &self.home,
                thread_id,
                SyndicPointReadLimit::new(65_536).unwrap(),
            )
            .unwrap()
            .unwrap();
        let item_id = SyndicItemId::from_bytes([11; 16]);
        let turn_id = SyndicTurnId::from_bytes([12; 16]);
        let projection_id = SyndicProjectionId::from_bytes([13; 16]);
        let content = ContentReference::new(
            SyndicContentId::from_bytes([14; 16]),
            ContentRevision::new(1).unwrap(),
            ContentEncoding::ComposerV1,
            ContentSummary::new(
                1,
                1,
                text.len() as u64,
                text.len() as u64,
                1,
                0,
                [0; 32],
                None,
                SyndicContentDigest::from_bytes([15; 32]),
            )
            .unwrap(),
        );
        let mut batch = FixtureBatch::new();
        batch
            .put(FixtureRecord::CanonicalItem(
                CanonicalItemRecord::local_user_input(
                    item_id,
                    turn_id,
                    TurnItemOrdinal::new(1).unwrap(),
                    ProjectionRevision::new(1).unwrap(),
                    content,
                    None,
                ),
            ))
            .unwrap();
        batch
            .put(FixtureRecord::Projection(ProjectionRecord::new(
                projection_id,
                ProjectionRevision::new(revision).unwrap(),
                item_id,
                turn_id,
                ProjectionOrdinal::new(1).unwrap(),
                syndic_storage::ProjectionPayload::inline_markdown(
                    MarkdownBlockId::from_bytes([16; 32]),
                    MarkdownBlockKind::Paragraph,
                    1,
                    ProjectionSourceRange::new(0, text.len() as u64).unwrap(),
                    text,
                )
                .unwrap(),
            )))
            .unwrap();
        batch
            .put(FixtureRecord::TranscriptViewEntry(
                TranscriptViewEntryRecord::new(
                    thread_id,
                    TranscriptGeneration::new(1).unwrap(),
                    TranscriptPosition::new(1).unwrap(),
                    item_id,
                    ProjectionRevision::new(1).unwrap(),
                    ItemProjectionGeneration::new(1).unwrap(),
                    projection_id,
                    ProjectionRevision::new(1).unwrap(),
                ),
            ))
            .unwrap();
        batch
            .put(FixtureRecord::TranscriptViewHead(
                TranscriptViewHeadRecord::new(
                    thread_id,
                    TranscriptGeneration::new(1).unwrap(),
                    ProjectionRevision::new(1).unwrap(),
                    1,
                    thread.committed_tail(),
                    thread.selected_path_digest(),
                    ProjectionLifecycle::Current,
                ),
            ))
            .unwrap();
        execute(
            &self.home,
            self.storage
                .fixture_contribution(self.storage.revision(&self.home).unwrap(), batch),
        );
    }
}

fn execute(
    home: &beryl_home_store::HomeStore,
    contribution: beryl_home_store::MutationContribution,
) {
    let mut command = HomeCommand::new(home.home_revision().unwrap());
    command.add(contribution).unwrap();
    assert!(matches!(
        home.execute(command),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
}

#[test]
fn production_read_only_attachment_keeps_one_prepared_request_until_publication_or_drop() {
    let fixture = Fixture::new();
    let cancel = AtomicBool::new(false);
    let request = fixture.request(1);
    let prepared = fixture
        .reader
        .prepare_attachment(request.clone(), &cancel)
        .unwrap();
    assert!(matches!(
        fixture
            .reader
            .prepare_attachment(fixture.request(2), &cancel),
        Err(TranscriptAttachmentError::Busy)
    ));
    let published = fixture
        .reader
        .publish_if_current(prepared, &request, &cancel, |seed| seed.view_id.is_some())
        .unwrap();
    assert!(published);
    let next = fixture
        .reader
        .prepare_attachment(fixture.request(2), &cancel)
        .unwrap();
    drop(next);
    assert!(
        fixture
            .reader
            .prepare_attachment(fixture.request(3), &cancel)
            .is_ok()
    );
}

#[test]
fn stale_source_completion_cannot_publish_and_releases_its_prepared_slot() {
    let fixture = Fixture::new();
    let cancel = AtomicBool::new(false);
    let request = fixture.request(1);
    let prepared = fixture
        .reader
        .prepare_attachment(request.clone(), &cancel)
        .unwrap();
    fixture.seed_head(2);
    let called = AtomicBool::new(false);
    assert_eq!(
        fixture
            .reader
            .publish_if_current(prepared, &request, &cancel, |_| called
                .store(true, Ordering::Release)),
        Err(TranscriptAttachmentError::Stale)
    );
    assert!(!called.load(Ordering::Acquire));
    assert!(
        fixture
            .reader
            .prepare_attachment(fixture.request(2), &cancel)
            .is_ok()
    );
}

#[test]
fn exact_window_host_activation_and_request_identity_fence_prepared_publication() {
    let fixture = Fixture::new();
    let cancel = AtomicBool::new(false);
    for dimension in 0..7 {
        let request = fixture.request(dimension + 1);
        let prepared = fixture
            .reader
            .prepare_attachment(request.clone(), &cancel)
            .unwrap();
        let mut changed = request;
        let other_host = Arc::new(());
        match dimension {
            0 => changed.window_id = WindowId::from_bytes([6; 16]),
            1 => changed.host = Arc::downgrade(&other_host),
            2 => changed.activation += 1,
            3 => changed.request_id += 1,
            4 => changed.purpose = TranscriptAttachmentPurpose::Refresh,
            5 => changed.thread_id = SyndicThreadId::from_bytes([7; 16]),
            _ => changed.placement = TranscriptActivationPlacement::Start,
        }
        assert_eq!(
            fixture
                .reader
                .publish_if_current(prepared, &changed, &cancel, |_| ()),
            Err(TranscriptAttachmentError::Identity)
        );
    }
}

#[test]
fn cancelled_or_retired_attachments_never_run_publication_and_release_capacity() {
    for retire in [false, true] {
        let fixture = Fixture::new();
        let cancel = AtomicBool::new(false);
        let request = fixture.request(1);
        let prepared = fixture
            .reader
            .prepare_attachment(request.clone(), &cancel)
            .unwrap();
        if retire {
            assert!(!fixture.reader.retire());
        } else {
            cancel.store(true, Ordering::Release);
        }
        let called = AtomicBool::new(false);
        let result = fixture
            .reader
            .publish_if_current(prepared, &request, &cancel, |_| {
                called.store(true, Ordering::Release)
            });
        assert_eq!(
            result,
            Err(if retire {
                TranscriptAttachmentError::Retired
            } else {
                TranscriptAttachmentError::Cancelled
            })
        );
        assert!(!called.load(Ordering::Acquire));
        assert!(fixture.reader.retire());
    }
}

#[test]
fn graph_or_host_disposal_revokes_source_and_prepared_completion() {
    for graph in [false, true] {
        let mut fixture = Fixture::new();
        let cancel = AtomicBool::new(false);
        let request = fixture.request(1);
        let prepared = fixture
            .reader
            .prepare_attachment(request.clone(), &cancel)
            .unwrap();
        if graph {
            fixture.graph = Arc::new(());
        } else {
            fixture.host = Arc::new(());
        }
        assert_eq!(
            fixture
                .reader
                .publish_if_current(prepared, &request, &cancel, |_| ()),
            Err(TranscriptAttachmentError::Retired)
        );
    }
}

#[test]
fn known_session_write_revalidates_unchanged_source_but_never_changed_head() {
    let fixture = Fixture::new();
    let cancel = AtomicBool::new(false);
    let request = fixture.request(1);
    let prepared = fixture
        .reader
        .prepare_attachment(request.clone(), &cancel)
        .unwrap();
    let session = fixture.state.session();
    execute(
        &fixture.home,
        session.initialize_threadless(
            session.revision(&fixture.home).unwrap(),
            beryl_state::InitializeThreadlessWindow::new(
                request.window_id,
                beryl_model::WindowPlacement::new(
                    beryl_model::WindowBounds::new(0, 0, 900, 700).unwrap(),
                    beryl_model::WindowDisplayState::Normal,
                    None,
                    None,
                ),
            ),
        ),
    );
    let revalidated = fixture
        .reader
        .revalidate_after_claim(prepared, &request, &cancel)
        .unwrap();
    assert!(
        fixture
            .reader
            .publish_if_current(revalidated, &request, &cancel, |_| true)
            .unwrap()
    );

    let request = fixture.request(2);
    let prepared = fixture
        .reader
        .prepare_attachment(request.clone(), &cancel)
        .unwrap();
    fixture.seed_head(2);
    assert!(matches!(
        fixture
            .reader
            .revalidate_after_claim(prepared, &request, &cancel),
        Err(TranscriptAttachmentError::Stale)
    ));
    assert!(
        fixture
            .reader
            .prepare_attachment(fixture.request(3), &cancel)
            .is_ok()
    );
}

#[test]
fn unpublished_source_attaches_typed_unavailable_but_never_replaces_coherent_refresh() {
    for absent in [true, false] {
        let fixture = Fixture::new();
        if absent {
            let mut batch = FixtureBatch::new();
            batch
                .delete(FixtureDelete::TranscriptViewHead(
                    fixture.request(1).thread_id,
                ))
                .unwrap();
            execute(
                &fixture.home,
                fixture
                    .storage
                    .fixture_contribution(fixture.storage.revision(&fixture.home).unwrap(), batch),
            );
        } else {
            fixture.seed_head_lifecycle(2, ProjectionLifecycle::Stale);
        }
        let cancel = AtomicBool::new(false);
        let request = fixture.request(1);
        let prepared = fixture
            .reader
            .prepare_attachment(request.clone(), &cancel)
            .unwrap();
        assert!(matches!(
            prepared.authority,
            TranscriptAttachmentAuthority::Unpublished { .. }
        ));
        fixture.reader.publish_if_current(prepared, &request, &cancel, |seed| {
            let Some(crate::syndic_transcript::TranscriptProviderResponseKind::ViewPage(page)) = seed.view_page_response else { panic!("expected local source state") };
            assert!(page.records.is_empty());
            assert!(matches!(page.history_state, crate::syndic_transcript::TranscriptProviderHistoryState::Unavailable { reason: crate::syndic_transcript::TranscriptProviderHistoryReason::ProjectionStale, .. }));
        }).unwrap();
        let mut refresh = fixture.request(2);
        refresh.purpose = TranscriptAttachmentPurpose::Refresh;
        assert!(matches!(
            fixture.reader.prepare_attachment(refresh, &cancel),
            Err(TranscriptAttachmentError::Stale)
        ));
    }
}

#[test]
fn changed_projection_cannot_rebase_a_known_claim_write() {
    let fixture = Fixture::new();
    fixture.seed_projection("authored source", 1);
    let cancel = AtomicBool::new(false);
    let request = fixture.request(1);
    let prepared = fixture
        .reader
        .prepare_attachment(request.clone(), &cancel)
        .unwrap();
    fixture.seed_projection("changed source", 2);
    assert!(matches!(
        fixture
            .reader
            .revalidate_after_claim(prepared, &request, &cancel),
        Err(TranscriptAttachmentError::Stale)
    ));
}

#[test]
fn oversized_projection_attaches_exact_local_capacity_state_and_preserves_refresh() {
    let fixture = Fixture::new();
    fixture.seed_projection(&"x".repeat(ATTACHMENT_MAX_BYTES), 1);
    let cancel = AtomicBool::new(false);
    let request = fixture.request(1);
    let prepared = fixture
        .reader
        .prepare_attachment(request.clone(), &cancel)
        .unwrap();
    assert!(matches!(
        prepared.authority,
        TranscriptAttachmentAuthority::CapacityLimited { .. }
    ));
    fixture.reader.publish_if_current(prepared, &request, &cancel, |seed| {
        let Some(crate::syndic_transcript::TranscriptProviderResponseKind::ViewPage(page)) = seed.view_page_response else { panic!("missing local capacity state") };
        assert!(page.records.is_empty());
        assert!(matches!(page.history_state, crate::syndic_transcript::TranscriptProviderHistoryState::Unavailable { reason: crate::syndic_transcript::TranscriptProviderHistoryReason::PresentationCapacity, .. }));
    }).unwrap();
    let mut refresh = fixture.request(2);
    refresh.purpose = TranscriptAttachmentPurpose::Refresh;
    assert!(matches!(
        fixture.reader.prepare_attachment(refresh, &cancel),
        Err(TranscriptAttachmentError::Capacity)
    ));
}

#[test]
fn canonical_projection_bytes_publish_with_stable_source_identity_for_unchanged_refresh() {
    let fixture = Fixture::new();
    fixture.seed_projection("authored source", 1);
    let cancel = AtomicBool::new(false);
    let request = fixture.request(1);
    let prepared = fixture
        .reader
        .prepare_attachment(request.clone(), &cancel)
        .unwrap();
    let identity = prepared.source_identity();
    fixture
        .reader
        .publish_if_current(prepared, &request, &cancel, |seed| {
            let Some(crate::syndic_transcript::TranscriptProviderResponseKind::ProjectionRecords(
                records,
            )) = seed.projection_records_response
            else {
                panic!("canonical projection payload was missing")
            };
            assert_eq!(records.records.len(), 1);
            let crate::syndic_transcript::ProjectionPayload::Text { text } =
                &records.records[0].payload
            else {
                panic!("authored text payload was missing")
            };
            assert_eq!(text.as_ref(), "authored source");
        })
        .unwrap();
    let mut refresh = fixture.request(2);
    refresh.purpose = TranscriptAttachmentPurpose::Refresh;
    let prepared = fixture.reader.prepare_attachment(refresh, &cancel).unwrap();
    assert_eq!(prepared.source_identity(), identity);
}
