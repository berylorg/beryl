use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use beryl_home_store::{
    CommandOutcome, HomeCommand, HomeOpenOptions, HomeSchemaVersion, HomeStore,
};
use beryl_model::{
    AdmittedHostPath, Availability, CasConversationToolProfile, CasLoadedSessionGeneration,
    CasLoadedThreadGeneration, CasNativeTurnCount, CasProcessGeneration, CasThreadId, CasTurnId,
    ExecutionBinding, PathFlavor, ProjectionRevision, RootId, RuntimeId, RuntimeMode,
    RuntimeNativePath, SyndicDraftId, SyndicItemId, SyndicThreadId,
};
use beryl_state::{
    AvailabilitySnapshot, BerylState, CatalogArchiveSummary, CatalogAvailabilitySummary,
    CatalogClaimSummary, CatalogExecutionSummary, CatalogFacts, CatalogLineageSummary,
    CatalogPointReadLimit, CatalogResolvedTitle, CatalogRow, CatalogRowExpectation,
    CatalogSourceRevisions, CreateRuntimeWithHomeRoot, MarkCatalogRowStale, RootRegistration,
    RuntimeRegistration, UnixMillis,
};
use syndic_storage::{
    CasLineageProof, CasRepresentedPrefixProof, ClaimCompactionDispatch, CompactionAdmissionRead,
    CompactionAttemptNonce, CompactionMarkerLifecycle, CompactionOperationId,
    CompactionOperationNonce, CompactionOperationRecord, CompactionProviderEvent,
    CompactionProviderSequence, CompactionRequestDisposition, CompactionThreadStatus,
    ContentAppend, ContentBuild, ContentManifestRecord, CreateThread, DraftEditHistoryPolicyV1,
    ExactThreadCatalogSummary, NativeCasLineage, PreparedContent,
    PreparedThreadCatalogSummaryReplacement, PublishCompactionProviderEvent,
    PublishCompactionRequestDisposition, PublishValidBinding, SealLifecycleContinuationContent,
    SettleLifecycleCompaction, SyndicPointReadLimit, SyndicStorage, SyndicTimestamp,
    ThreadArchiveState, ThreadCatalogSummaryPreparation, ThreadCatalogTitleSource,
    ThreadLineageDepth, TurnEndStatus, TurnTerminalOutcome, empty_selected_path_digest,
    prepare_lifecycle_continuation_content,
};

static NEXT_HOME: AtomicU64 = AtomicU64::new(1);

struct TestHome {
    path: PathBuf,
}

impl TestHome {
    fn new(name: &str) -> Self {
        loop {
            let sequence = NEXT_HOME.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "beryl-phase75-{name}-{}-{sequence}",
                std::process::id()
            ));
            match std::fs::create_dir(&path) {
                Ok(()) => return Self { path },
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => panic!("create test home {path:?}: {error}"),
            }
        }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TestHome {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

struct Fixture {
    home: TestHome,
    store: HomeStore,
    state: BerylState,
    syndic: SyndicStorage,
    thread: SyndicThreadId,
    runtime: RuntimeId,
    root: RootId,
}

impl Fixture {
    fn new(name: &str) -> Self {
        let home = TestHome::new(name);
        let mut store = HomeStore::open(HomeOpenOptions::new(
            home.path(),
            HomeSchemaVersion::CURRENT,
        ))
        .unwrap();
        let state = BerylState::register(&mut store).unwrap();
        let syndic = SyndicStorage::register(&mut store).unwrap();
        let runtime = RuntimeId::from_bytes([75; 16]);
        let root = RootId::from_bytes([76; 16]);
        let thread = SyndicThreadId::from_bytes([77; 16]);
        let mode = RuntimeMode::host();
        let root_path = runtime_path(mode.clone(), r"C:\\Work\\Beryl");
        let runtime_registration = RuntimeRegistration::new(
            runtime,
            admitted_host_path(r"C:\\Codex\\codex.exe"),
            mode.clone(),
            runtime_path(mode.clone(), r"C:\\Codex\\codex.exe"),
            UnixMillis::new(1),
            AvailabilitySnapshot::observed(Availability::Available, UnixMillis::new(2)).unwrap(),
        )
        .unwrap();
        let root_registration = RootRegistration::new(
            root,
            root_path.clone(),
            admitted_host_path(r"C:\\Work\\Beryl"),
            UnixMillis::new(1),
            AvailabilitySnapshot::unknown(),
        );
        let mut command = HomeCommand::new(store.home_revision().unwrap());
        command
            .add(state.runtime_roots().create_runtime_with_home_root(
                state.runtime_roots().revision(&store).unwrap(),
                CreateRuntimeWithHomeRoot::new(runtime_registration, root_registration).unwrap(),
            ))
            .unwrap();
        command
            .add(syndic.create_thread(
                syndic.revision(&store).unwrap(),
                CreateThread::ordinary(
                    thread,
                    SyndicDraftId::from_bytes([78; 16]),
                    ExecutionBinding::new(runtime, root, root_path),
                    syndic_storage::SyndicTimestamp::from_unix_millis(3),
                    history_policy(),
                ),
            ))
            .unwrap();
        execute(&store, command);
        let fixture = Self {
            home,
            store,
            state,
            syndic,
            thread,
            runtime,
            root,
        };
        fixture.publish_valid_binding();
        fixture
    }

    fn execution_binding(&self) -> ExecutionBinding {
        ExecutionBinding::new(
            self.runtime,
            self.root,
            runtime_path(RuntimeMode::host(), r"C:\\Work\\Beryl"),
        )
    }

    fn publish_valid_binding(&self) {
        let current = self
            .syndic
            .current_binding(&self.store, self.thread, syndic_limit())
            .unwrap()
            .unwrap();
        let selected_path = current.binding().selected_path();
        let represented_prefix = CasRepresentedPrefixProof::new(
            None,
            selected_path.thread_revision(),
            empty_selected_path_digest(),
        );
        execute_contribution(
            &self.store,
            self.syndic.publish_valid_binding(
                self.syndic.revision(&self.store).unwrap(),
                PublishValidBinding::new(
                    self.thread,
                    current.binding().revision(),
                    selected_path,
                    self.execution_binding(),
                    CasThreadId::new("phase75-catalog-history").unwrap(),
                    represented_prefix,
                    CasNativeTurnCount::ZERO,
                    CasConversationToolProfile::v1([75; 32]),
                    CasLineageProof::native(NativeCasLineage::Fresh, represented_prefix).unwrap(),
                ),
            ),
        );
    }

    fn operation(&self, id: CompactionOperationId) -> CompactionOperationRecord {
        self.syndic
            .compaction_operation(&self.store, id, syndic_limit())
            .unwrap()
            .unwrap()
    }

    fn publish_provider(&self, id: CompactionOperationId, event: CompactionProviderEvent, at: u64) {
        let operation = self.operation(id);
        let sequence = operation
            .provider_frontier()
            .map_or(CompactionProviderSequence::FIRST, |frontier| {
                frontier.checked_next().unwrap()
            });
        execute_current(
            &self.store,
            self.syndic.current_publish_compaction_provider_event(
                PublishCompactionProviderEvent::new(
                    id,
                    operation.revision(),
                    sequence,
                    event,
                    timestamp(at),
                ),
            ),
        );
    }

    fn advance_history_summary_with_lifecycle_continuation(&self) {
        let CompactionAdmissionRead::Admissible(candidate) = self
            .syndic
            .compaction_admission_read(&self.store, self.thread, syndic_limit())
            .unwrap()
        else {
            panic!("current idle thread must admit lifecycle compaction");
        };
        let admission = candidate.admission(
            CompactionOperationNonce::from_bytes([79; 16]),
            CompactionAttemptNonce::from_bytes([80; 16]),
            CasLoadedSessionGeneration::new(
                CasProcessGeneration::new(75).unwrap(),
                CasLoadedThreadGeneration::new(1).unwrap(),
            ),
            timestamp(10),
        );
        let id = admission.operation_id();
        execute_current(
            &self.store,
            self.syndic.current_admit_compaction_operation(admission),
        );
        let operation = self.operation(id);
        execute_current(
            &self.store,
            self.syndic
                .current_claim_compaction_dispatch(ClaimCompactionDispatch::new(
                    id,
                    operation.revision(),
                    operation.attempt(),
                )),
        );
        let operation = self.operation(id);
        execute_current(
            &self.store,
            self.syndic.current_publish_compaction_request_disposition(
                PublishCompactionRequestDisposition::new(
                    id,
                    operation.revision(),
                    operation.attempt(),
                    CompactionRequestDisposition::Accepted,
                ),
            ),
        );
        self.publish_provider(
            id,
            CompactionProviderEvent::ThreadStatus(CompactionThreadStatus::Active),
            20,
        );
        self.publish_provider(
            id,
            CompactionProviderEvent::TurnStarted(CasTurnId::new("phase75-compaction").unwrap()),
            21,
        );
        let marker = SyndicItemId::from_bytes([81; 16]);
        self.publish_provider(
            id,
            CompactionProviderEvent::Marker {
                item_id: marker,
                lifecycle: CompactionMarkerLifecycle::Started,
            },
            22,
        );
        self.publish_provider(
            id,
            CompactionProviderEvent::Marker {
                item_id: marker,
                lifecycle: CompactionMarkerLifecycle::Completed,
            },
            23,
        );
        self.publish_provider(
            id,
            CompactionProviderEvent::ThreadStatus(CompactionThreadStatus::Idle),
            24,
        );
        self.publish_provider(
            id,
            CompactionProviderEvent::Terminal(
                TurnEndStatus::new(TurnTerminalOutcome::Complete, None).unwrap(),
            ),
            25,
        );
        let prepared = prepare_lifecycle_continuation_content().unwrap();
        let manifest = stage_prepared_content(&self.store, self.syndic.clone(), &prepared);
        execute_current(
            &self.store,
            self.syndic.current_seal_lifecycle_continuation_content(
                SealLifecycleContinuationContent::new(manifest),
            ),
        );
        let content = self
            .syndic
            .content_manifest(&self.store, prepared.id(), syndic_limit())
            .unwrap()
            .unwrap()
            .sealed_reference()
            .unwrap();
        let operation = self.operation(id);
        execute_current(
            &self.store,
            self.syndic
                .current_settle_lifecycle_compaction(SettleLifecycleCompaction::new(
                    &operation,
                    content,
                    timestamp(30),
                )),
        );
    }
}

fn admitted_host_path(value: &str) -> AdmittedHostPath {
    AdmittedHostPath::from_admitted(PathFlavor::Windows, value).unwrap()
}

fn runtime_path(mode: RuntimeMode, value: &str) -> RuntimeNativePath {
    RuntimeNativePath::from_admitted(mode, PathFlavor::Windows, value).unwrap()
}

fn history_policy() -> DraftEditHistoryPolicyV1 {
    DraftEditHistoryPolicyV1::new(65_536, 1).unwrap()
}

fn timestamp(value: u64) -> SyndicTimestamp {
    SyndicTimestamp::from_unix_millis(value)
}

fn syndic_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(1_000_000).unwrap()
}

fn execute(store: &HomeStore, command: HomeCommand) {
    match store.execute(command) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected a clean catalog command, got {outcome:?}"),
    }
}

fn execute_rejected(store: &HomeStore, command: HomeCommand) {
    match store.execute(command) {
        CommandOutcome::NotCommitted { .. } => {}
        CommandOutcome::Indeterminate { reconciliation, .. } => {
            reconciliation.install();
            panic!("expected rejected catalog command, got Indeterminate");
        }
        outcome => panic!("expected rejected catalog command, got {outcome:?}"),
    }
}

fn execute_contribution(store: &HomeStore, contribution: beryl_home_store::MutationContribution) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    execute(store, command);
}

fn execute_current(store: &HomeStore, command: beryl_home_store::CurrentDomainCommand) {
    match store.execute_current(command) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected a clean current-domain command, got {outcome:?}"),
    }
}

fn stage_prepared_content(
    store: &HomeStore,
    storage: SyndicStorage,
    prepared: &PreparedContent,
) -> ContentManifestRecord {
    let mut manifest = prepared.building_manifest();
    execute_contribution(
        store,
        storage.begin_content(
            storage.revision(store).unwrap(),
            ContentBuild::from_prepared(prepared),
        ),
    );
    loop {
        let Some(append) = ContentAppend::prepare(&manifest, prepared).unwrap() else {
            break;
        };
        manifest = append.next_manifest().clone();
        execute_contribution(
            store,
            storage.append_content(storage.revision(store).unwrap(), append),
        );
    }
    manifest
}

fn catalog_facts(
    summary: &syndic_storage::ThreadCatalogSummaryRecord,
    source: &beryl_state::RuntimeRootCatalogSource,
) -> CatalogFacts {
    let title = match summary.title() {
        None => CatalogResolvedTitle::absent(),
        Some(title) => match title.source() {
            ThreadCatalogTitleSource::Generated => {
                CatalogResolvedTitle::generated(title.text()).unwrap()
            }
            ThreadCatalogTitleSource::HistoryDerived => {
                CatalogResolvedTitle::history_derived(title.text()).unwrap()
            }
        },
    };
    let archive = match summary.archive() {
        ThreadArchiveState::Ordinary => CatalogArchiveSummary::Ordinary,
        ThreadArchiveState::BranchDiscussionOpen => CatalogArchiveSummary::BranchDiscussionOpen,
        ThreadArchiveState::BranchDiscussionArchived { .. } => {
            CatalogArchiveSummary::BranchDiscussionArchived
        }
    };
    let lineage = match (summary.parent_thread_id(), summary.lineage_depth()) {
        (None, depth) if depth == ThreadLineageDepth::FIRST => CatalogLineageSummary::TopLevel,
        (Some(parent), depth) => {
            CatalogLineageSummary::descendant(parent, depth.get(), summary.lineage_digest())
                .unwrap()
        }
        (None, _) => panic!("top-level summary has non-root lineage"),
    };
    let runtime = source.runtime();
    let root = source.root();
    CatalogFacts::new(
        title,
        CatalogExecutionSummary::new(
            runtime.runtime_id(),
            root.root_id(),
            runtime.environment_label(),
            runtime.canonical_executable().clone(),
            root.display_path().clone(),
            CatalogAvailabilitySummary::new(
                runtime.availability().availability(),
                root.availability().availability(),
            ),
        )
        .unwrap(),
        archive,
        UnixMillis::new(summary.last_activity_at().unix_millis()),
        summary.complete(),
        CatalogClaimSummary::Unclaimed,
        lineage,
    )
    .unwrap()
}

fn publish_exact_current(fixture: &Fixture, exact: ExactThreadCatalogSummary) {
    let summary = exact.summary().clone();
    let runtime_revision = fixture
        .state
        .runtime_roots()
        .revision(&fixture.store)
        .unwrap();
    let runtime_source = fixture
        .state
        .runtime_roots()
        .catalog_source(&fixture.store, fixture.runtime, fixture.root)
        .unwrap();
    let session_revision = fixture.state.session().revision(&fixture.store).unwrap();
    let claim_source = fixture
        .state
        .session()
        .thread_claim_catalog_source(&fixture.store, fixture.thread)
        .unwrap();
    let publication = beryl_state::PublishCatalogRow::new(
        fixture.thread,
        CatalogRowExpectation::Missing,
        CatalogSourceRevisions::new(
            summary.revision(),
            runtime_source.runtime().revision(),
            runtime_source.root().revision(),
            None,
        ),
        catalog_facts(&summary, &runtime_source),
    )
    .unwrap();
    let mut command = HomeCommand::new(fixture.store.home_revision().unwrap());
    command
        .add(fixture.state.catalog().publish(
            fixture.state.catalog().revision(&fixture.store).unwrap(),
            publication,
        ))
        .unwrap();
    command
        .add_validation(
            fixture
                .syndic
                .validate_current_thread_catalog_summary(exact),
        )
        .unwrap();
    command
        .add_validation(
            fixture
                .state
                .runtime_roots()
                .validate_catalog_source(runtime_revision, runtime_source),
        )
        .unwrap();
    command
        .add_validation(
            fixture
                .state
                .session()
                .validate_thread_claim_catalog_source(session_revision, claim_source),
        )
        .unwrap();
    execute(&fixture.store, command);
}

fn current_catalog_row(fixture: &Fixture) -> CatalogRow {
    fixture
        .state
        .catalog()
        .row(
            &fixture.store,
            fixture.thread,
            CatalogPointReadLimit::schema_maximum(),
        )
        .unwrap()
        .unwrap()
}

fn publish_replacement(
    fixture: &Fixture,
    expected_row: &CatalogRow,
    prepared: PreparedThreadCatalogSummaryReplacement,
) {
    let replacement = prepared.replacement().clone();
    let runtime_revision = fixture
        .state
        .runtime_roots()
        .revision(&fixture.store)
        .unwrap();
    let runtime_source = fixture
        .state
        .runtime_roots()
        .catalog_source(&fixture.store, fixture.runtime, fixture.root)
        .unwrap();
    let session_revision = fixture.state.session().revision(&fixture.store).unwrap();
    let claim_source = fixture
        .state
        .session()
        .thread_claim_catalog_source(&fixture.store, fixture.thread)
        .unwrap();
    let publication = beryl_state::PublishCatalogRow::new(
        fixture.thread,
        CatalogRowExpectation::Revision(expected_row.revision()),
        CatalogSourceRevisions::new(
            replacement.revision(),
            runtime_source.runtime().revision(),
            runtime_source.root().revision(),
            None,
        ),
        catalog_facts(&replacement, &runtime_source),
    )
    .unwrap();
    let mut command = HomeCommand::new(fixture.store.home_revision().unwrap());
    command
        .add(fixture.state.catalog().publish(
            fixture.state.catalog().revision(&fixture.store).unwrap(),
            publication,
        ))
        .unwrap();
    command
        .add(fixture.syndic.rebuild_thread_catalog_summary(prepared))
        .unwrap();
    command
        .add_validation(
            fixture
                .state
                .runtime_roots()
                .validate_catalog_source(runtime_revision, runtime_source),
        )
        .unwrap();
    command
        .add_validation(
            fixture
                .state
                .session()
                .validate_thread_claim_catalog_source(session_revision, claim_source),
        )
        .unwrap();
    execute(&fixture.store, command);
}

#[test]
fn ordinary_creation_has_an_exact_compact_summary_and_cross_domain_publication() {
    let fixture = Fixture::new("creation-publication");
    let exact = match fixture
        .syndic
        .prepare_thread_catalog_summary(&fixture.store, fixture.thread)
        .unwrap()
        .unwrap()
    {
        ThreadCatalogSummaryPreparation::ExactCurrent(exact) => exact,
        ThreadCatalogSummaryPreparation::PreparedReplacement(_) => {
            panic!("ordinary creation must publish an exact compact summary")
        }
    };
    assert!(exact.summary().title().is_none());
    assert!(exact.summary().complete());
    publish_exact_current(&fixture, exact);
    let row = fixture
        .state
        .catalog()
        .row(
            &fixture.store,
            fixture.thread,
            CatalogPointReadLimit::schema_maximum(),
        )
        .unwrap()
        .unwrap();
    assert_eq!(
        row.sources().syndic_summary(),
        ProjectionRevision::new(1).unwrap()
    );
    assert_eq!(row.facts().execution().runtime_id(), fixture.runtime);
    assert_eq!(row.facts().execution().root_id(), fixture.root);
    assert!(matches!(
        fixture
            .syndic
            .prepare_thread_catalog_summary(&fixture.store, fixture.thread)
            .unwrap()
            .unwrap(),
        ThreadCatalogSummaryPreparation::ExactCurrent(_)
    ));
    fixture
        .store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
}

#[test]
fn current_catalog_summary_and_cross_domain_row_survive_reopen() {
    let fixture = Fixture::new("reopen");
    let exact = match fixture
        .syndic
        .prepare_thread_catalog_summary(&fixture.store, fixture.thread)
        .unwrap()
        .unwrap()
    {
        ThreadCatalogSummaryPreparation::ExactCurrent(exact) => exact,
        ThreadCatalogSummaryPreparation::PreparedReplacement(_) => unreachable!(),
    };
    publish_exact_current(&fixture, exact);
    let path = fixture.home.path().to_owned();
    let thread = fixture.thread;
    fixture.store.close().unwrap();
    let mut reopened =
        HomeStore::open(HomeOpenOptions::new(&path, HomeSchemaVersion::CURRENT)).unwrap();
    let syndic = SyndicStorage::register(&mut reopened).unwrap();
    assert!(matches!(
        syndic
            .prepare_thread_catalog_summary(&reopened, thread)
            .unwrap()
            .unwrap(),
        ThreadCatalogSummaryPreparation::ExactCurrent(_)
    ));
    reopened
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
}

#[test]
fn history_change_rebuilds_catalog_and_stale_replacement_is_atomic() {
    let fixture = Fixture::new("history-replacement");
    let initial_exact = match fixture
        .syndic
        .prepare_thread_catalog_summary(&fixture.store, fixture.thread)
        .unwrap()
        .unwrap()
    {
        ThreadCatalogSummaryPreparation::ExactCurrent(exact) => exact,
        ThreadCatalogSummaryPreparation::PreparedReplacement(_) => unreachable!(),
    };
    let initial_summary = initial_exact.summary().clone();
    publish_exact_current(&fixture, initial_exact);
    let initial_row = current_catalog_row(&fixture);

    fixture.advance_history_summary_with_lifecycle_continuation();
    let prepared = match fixture
        .syndic
        .prepare_thread_catalog_summary(&fixture.store, fixture.thread)
        .unwrap()
        .unwrap()
    {
        ThreadCatalogSummaryPreparation::PreparedReplacement(prepared) => prepared,
        ThreadCatalogSummaryPreparation::ExactCurrent(_) => {
            panic!("current history change must make the compact summary stale")
        }
    };
    assert_eq!(
        prepared.replacement().revision(),
        initial_summary.revision().checked_next().unwrap()
    );
    assert!(
        prepared.replacement().sources().history_summary_revision()
            > initial_summary.sources().history_summary_revision()
    );
    let expected_replacement = prepared.replacement().clone();
    let stale_prepared = prepared.clone();
    publish_replacement(&fixture, &initial_row, prepared);

    let exact_after_rebuild = match fixture
        .syndic
        .prepare_thread_catalog_summary(&fixture.store, fixture.thread)
        .unwrap()
        .unwrap()
    {
        ThreadCatalogSummaryPreparation::ExactCurrent(exact) => exact,
        ThreadCatalogSummaryPreparation::PreparedReplacement(_) => {
            panic!("checked replacement must publish the exact current summary")
        }
    };
    assert_eq!(exact_after_rebuild.summary(), &expected_replacement);
    let valid_summary = exact_after_rebuild.summary().clone();
    let valid_row = current_catalog_row(&fixture);
    assert_eq!(
        valid_row.sources().syndic_summary(),
        valid_summary.revision()
    );
    assert_eq!(
        valid_row.facts().last_activity_at(),
        UnixMillis::new(valid_summary.last_activity_at().unix_millis())
    );

    let mut failed_replacement = HomeCommand::new(fixture.store.home_revision().unwrap());
    failed_replacement
        .add(fixture.state.catalog().mark_stale(
            fixture.state.catalog().revision(&fixture.store).unwrap(),
            MarkCatalogRowStale::new(fixture.thread, valid_row.revision()),
        ))
        .unwrap();
    failed_replacement
        .add(
            fixture
                .syndic
                .rebuild_thread_catalog_summary(stale_prepared),
        )
        .unwrap();
    execute_rejected(&fixture.store, failed_replacement);
    assert_eq!(
        fixture
            .syndic
            .thread_catalog_summary(&fixture.store, fixture.thread, syndic_limit())
            .unwrap()
            .unwrap(),
        valid_summary
    );
    assert_eq!(current_catalog_row(&fixture), valid_row);

    let runtime_source = fixture
        .state
        .runtime_roots()
        .catalog_source(&fixture.store, fixture.runtime, fixture.root)
        .unwrap();
    let stale_publication = beryl_state::PublishCatalogRow::new(
        fixture.thread,
        CatalogRowExpectation::Revision(initial_row.revision()),
        CatalogSourceRevisions::new(
            valid_summary.revision(),
            runtime_source.runtime().revision(),
            runtime_source.root().revision(),
            None,
        ),
        catalog_facts(&valid_summary, &runtime_source),
    )
    .unwrap();
    let mut command = HomeCommand::new(fixture.store.home_revision().unwrap());
    command
        .add(fixture.state.catalog().publish(
            fixture.state.catalog().revision(&fixture.store).unwrap(),
            stale_publication,
        ))
        .unwrap();
    execute_rejected(&fixture.store, command);
    assert_eq!(
        fixture
            .syndic
            .thread_catalog_summary(&fixture.store, fixture.thread, syndic_limit())
            .unwrap()
            .unwrap(),
        valid_summary
    );
    assert_eq!(current_catalog_row(&fixture), valid_row);
    fixture
        .store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
}
