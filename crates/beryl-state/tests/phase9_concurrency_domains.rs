mod support;

use beryl_home_store::{CommandOutcome, HomeCommand};
use beryl_state::{
    AdmitBranchHandoffJob, ApplySettings, BranchHandoffJobLifecycle, CatalogFreshness,
    CatalogPointReadLimit, CatalogRowExpectation, CompleteResolvingTurn, ExpectedSettingRevision,
    HandoffFailureEvidence, HandoffFailureKind, MarkCatalogRowStale, PublishCatalogRow,
    RecordTerminalHandoffFailure, SettingKey, SettingUpdate, SettingValue,
};
use tempfile::tempdir;

use support::execute;
use support::phase9::{
    admission, assert_one_success_one_conflict, catalog_facts, catalog_sources, command,
    race_commands, thread,
};

macro_rules! expect_committed {
    ($outcome:expr) => {{
        let outcome = $outcome;
        match outcome {
            CommandOutcome::Committed {
                receipt,
                later_failure: None,
            } => receipt,
            outcome => panic!("expected committed command, got {outcome:?}"),
        }
    }};
}

fn absent(key: SettingKey, value: SettingValue) -> SettingUpdate {
    SettingUpdate::new(key, ExpectedSettingRevision::Absent, value)
}

fn exact(
    key: SettingKey,
    revision: beryl_state::RecordRevision,
    value: SettingValue,
) -> SettingUpdate {
    SettingUpdate::new(key, ExpectedSettingRevision::Exact(revision), value)
}

#[test]
fn concurrent_multi_setting_apply_never_publishes_a_mixed_pair() {
    let directory = tempdir().unwrap();
    let (store, state) = support::open(directory.path());
    expect_committed!(execute(
        &store,
        state.settings().apply(
            state.settings().revision(&store).unwrap(),
            ApplySettings::new(vec![
                absent(
                    SettingKey::ActiveThemeId,
                    SettingValue::active_theme_id("initial").unwrap(),
                ),
                absent(
                    SettingKey::DraftAutosaveInterval,
                    SettingValue::draft_autosave_interval_seconds(1),
                ),
            ])
            .unwrap(),
        ),
    ));

    let theme = state
        .settings()
        .setting(&store, SettingKey::ActiveThemeId)
        .unwrap()
        .unwrap();
    let autosave = state
        .settings()
        .setting(&store, SettingKey::DraftAutosaveInterval)
        .unwrap()
        .unwrap();
    let expected_home = store.home_revision().unwrap();
    let expected_domain = state.settings().revision(&store).unwrap();
    let build = |name: &str, seconds: u64| {
        let mut command = HomeCommand::new(expected_home);
        command
            .add(
                state.settings().apply(
                    expected_domain,
                    ApplySettings::new(vec![
                        exact(
                            SettingKey::ActiveThemeId,
                            theme.revision(),
                            SettingValue::active_theme_id(name).unwrap(),
                        ),
                        exact(
                            SettingKey::DraftAutosaveInterval,
                            autosave.revision(),
                            SettingValue::draft_autosave_interval_seconds(seconds),
                        ),
                    ])
                    .unwrap(),
                ),
            )
            .unwrap();
        command
    };
    let results = race_commands(&store, build("apply-a", 10), build("apply-b", 20));
    assert_one_success_one_conflict(&results);

    let final_theme = state
        .settings()
        .setting(&store, SettingKey::ActiveThemeId)
        .unwrap()
        .unwrap()
        .value()
        .as_active_theme_id()
        .unwrap()
        .to_owned();
    let final_autosave = state
        .settings()
        .setting(&store, SettingKey::DraftAutosaveInterval)
        .unwrap()
        .unwrap()
        .value()
        .as_draft_autosave_interval_seconds()
        .unwrap();
    assert!(
        (final_theme == "apply-a" && final_autosave == 10)
            || (final_theme == "apply-b" && final_autosave == 20)
    );

    store.close().unwrap();
    let (reopened, state) = support::open(directory.path());
    assert_eq!(
        state
            .settings()
            .setting(&reopened, SettingKey::ActiveThemeId)
            .unwrap()
            .unwrap()
            .value()
            .as_active_theme_id(),
        Some(final_theme.as_str())
    );
    assert_eq!(
        state
            .settings()
            .setting(&reopened, SettingKey::DraftAutosaveInterval)
            .unwrap()
            .unwrap()
            .value()
            .as_draft_autosave_interval_seconds(),
        Some(final_autosave)
    );
}

#[test]
fn concurrent_durable_job_transitions_leave_one_valid_lifecycle() {
    let directory = tempdir().unwrap();
    let (store, state) = support::open(directory.path());
    let admitted = admission(40);
    let job_id = admitted.job_id();
    expect_committed!(execute(
        &store,
        state.durable_jobs().admit_branch_handoff(
            state.durable_jobs().revision(&store).unwrap(),
            AdmitBranchHandoffJob::new(admitted),
        ),
    ));
    let job = state.durable_jobs().job(&store, job_id).unwrap().unwrap();
    let expected_home = store.home_revision().unwrap();
    let expected_domain = state.durable_jobs().revision(&store).unwrap();

    let mut complete = HomeCommand::new(expected_home);
    complete
        .add(state.durable_jobs().complete_resolving_turn(
            expected_domain,
            CompleteResolvingTurn::new(job_id, job.revision()),
        ))
        .unwrap();
    let mut fail = HomeCommand::new(expected_home);
    fail.add(
        state.durable_jobs().record_terminal_failure(
            expected_domain,
            RecordTerminalHandoffFailure::new(
                job_id,
                job.revision(),
                HandoffFailureEvidence::new(
                    HandoffFailureKind::InvariantViolation,
                    Some("concurrent terminal observation"),
                )
                .unwrap(),
            ),
        ),
    )
    .unwrap();
    let results = race_commands(&store, complete, fail);
    assert_one_success_one_conflict(&results);

    let lifecycle = state
        .durable_jobs()
        .job(&store, job_id)
        .unwrap()
        .unwrap()
        .lifecycle();
    assert!(matches!(
        lifecycle,
        BranchHandoffJobLifecycle::WaitingParent | BranchHandoffJobLifecycle::TerminalFailed
    ));

    store.close().unwrap();
    let (reopened, state) = support::open(directory.path());
    assert_eq!(
        state
            .durable_jobs()
            .job(&reopened, job_id)
            .unwrap()
            .unwrap()
            .lifecycle(),
        lifecycle
    );
}

#[test]
fn catalog_stale_marking_races_rebuild_publication_without_masking_authority() {
    let directory = tempdir().unwrap();
    let (store, state) = support::open(directory.path());
    let thread_id = thread(50);
    expect_committed!(execute(
        &store,
        state.catalog().publish(
            state.catalog().revision(&store).unwrap(),
            PublishCatalogRow::new(
                thread_id,
                CatalogRowExpectation::Missing,
                catalog_sources(1),
                catalog_facts(50, 1, 100),
            )
            .unwrap(),
        ),
    ));
    let initial = state
        .catalog()
        .row(&store, thread_id, CatalogPointReadLimit::schema_maximum())
        .unwrap()
        .unwrap();
    let expected_domain = state.catalog().revision(&store).unwrap();
    let stale = command(
        &store,
        state.catalog().mark_stale(
            expected_domain,
            MarkCatalogRowStale::new(thread_id, initial.revision()),
        ),
    );
    let rebuild = command(
        &store,
        state.catalog().publish(
            expected_domain,
            PublishCatalogRow::new(
                thread_id,
                CatalogRowExpectation::Revision(initial.revision()),
                catalog_sources(2),
                catalog_facts(50, 2, 200),
            )
            .unwrap(),
        ),
    );
    let results = race_commands(&store, stale, rebuild);
    assert_one_success_one_conflict(&results);

    let raced = state
        .catalog()
        .row(&store, thread_id, CatalogPointReadLimit::schema_maximum())
        .unwrap()
        .unwrap();
    if raced.freshness() == CatalogFreshness::Stale {
        expect_committed!(execute(
            &store,
            state.catalog().publish(
                state.catalog().revision(&store).unwrap(),
                PublishCatalogRow::new(
                    thread_id,
                    CatalogRowExpectation::Revision(raced.revision()),
                    catalog_sources(2),
                    catalog_facts(50, 2, 200),
                )
                .unwrap(),
            ),
        ));
    }
    let rebuilt = state
        .catalog()
        .row(&store, thread_id, CatalogPointReadLimit::schema_maximum())
        .unwrap()
        .unwrap();
    assert_eq!(rebuilt.freshness(), CatalogFreshness::Current);
    assert_eq!(rebuilt.sources(), catalog_sources(2));

    store.close().unwrap();
    let (reopened, state) = support::open(directory.path());
    let reopened_row = state
        .catalog()
        .row(
            &reopened,
            thread_id,
            CatalogPointReadLimit::schema_maximum(),
        )
        .unwrap()
        .unwrap();
    assert_eq!(reopened_row, rebuilt);
}
