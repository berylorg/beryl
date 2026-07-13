mod support;

use std::num::NonZeroU64;

use beryl_home_store::{HomeCommand, SidecarNamespace};
use beryl_model::AssetId;
use beryl_state::{
    AddAssetReference, AdmitBranchHandoffJob, ApplySettings, AssetMediaType,
    BranchHandoffJobLifecycle, CatalogFreshness, CatalogPointReadLimit, CatalogRowExpectation,
    CompleteResolvingTurn, CreateAssetWithReference, ExpectedSettingRevision,
    HandoffFailureEvidence, HandoffFailureKind, MarkCatalogRowStale, PublishCatalogRow,
    RecordTerminalHandoffFailure, RemoveAssetReference, SettingKey, SettingUpdate, SettingValue,
    UnixMillis,
};
use tempfile::tempdir;

use support::execute;
use support::phase9::{
    admission, assert_one_success_one_conflict, asset_owner, catalog_facts, catalog_sources,
    command, race_commands, sidecar_limit, thread,
};

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
    execute(
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
    )
    .unwrap();

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
    execute(
        &store,
        state.durable_jobs().admit_branch_handoff(
            state.durable_jobs().revision(&store).unwrap(),
            AdmitBranchHandoffJob::new(admitted),
        ),
    )
    .unwrap();
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
    execute(
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
    )
    .unwrap();
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
            MarkCatalogRowStale::new(thread_id, initial.row().revision()),
        ),
    );
    let rebuild = command(
        &store,
        state.catalog().publish(
            expected_domain,
            PublishCatalogRow::new(
                thread_id,
                CatalogRowExpectation::Revision(initial.row().revision()),
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
    if raced.row().freshness() == CatalogFreshness::Stale {
        execute(
            &store,
            state.catalog().publish(
                state.catalog().revision(&store).unwrap(),
                PublishCatalogRow::new(
                    thread_id,
                    CatalogRowExpectation::Revision(raced.row().revision()),
                    catalog_sources(2),
                    catalog_facts(50, 2, 200),
                )
                .unwrap(),
            ),
        )
        .unwrap();
    }
    let rebuilt = state
        .catalog()
        .row(&store, thread_id, CatalogPointReadLimit::schema_maximum())
        .unwrap()
        .unwrap();
    assert_eq!(rebuilt.row().freshness(), CatalogFreshness::Current);
    assert_eq!(rebuilt.row().sources(), catalog_sources(2));

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
    assert_eq!(reopened_row.row(), rebuilt.row());
}

#[test]
fn asset_add_vs_remove_race_keeps_metadata_and_both_reference_indexes_coherent() {
    let directory = tempdir().unwrap();
    let (store, state) = support::open(directory.path());
    let first_owner = asset_owner(60);
    let second_owner = asset_owner(61);
    let sidecar = store
        .admit_sidecar(
            SidecarNamespace::new("images").unwrap(),
            b"concurrent asset references",
            sidecar_limit(),
        )
        .unwrap();
    let sidecar_path = sidecar.path().to_path_buf();
    let asset_id = AssetId::sha256_v1(
        sidecar.address().digest().as_bytes(),
        NonZeroU64::new(sidecar.address().length()).unwrap(),
    );
    let expected_domain = state.assets().revision(&store).unwrap();
    let first = state
        .assets()
        .create_with_reference(
            expected_domain,
            sidecar,
            CreateAssetWithReference::new(
                asset_id,
                AssetMediaType::new("image/png").unwrap(),
                None,
                expected_domain.checked_next().unwrap(),
                first_owner,
                UnixMillis::new(1),
            )
            .unwrap(),
        )
        .unwrap();
    let mut first_command = HomeCommand::new(store.home_revision().unwrap());
    first.add_to(&mut first_command).unwrap();
    store.execute(first_command).unwrap();

    let metadata = state.assets().metadata(&store, asset_id).unwrap().unwrap();
    let expected_domain = state.assets().revision(&store).unwrap();
    let add = command(
        &store,
        state.assets().add_reference(
            expected_domain,
            AddAssetReference::new(
                asset_id,
                metadata.revision(),
                second_owner,
                UnixMillis::new(2),
            )
            .unwrap(),
        ),
    );
    let remove = command(
        &store,
        state.assets().remove_reference(
            expected_domain,
            RemoveAssetReference::new(first_owner, asset_id, metadata.revision()),
        ),
    );
    let results = race_commands(&store, add, remove);
    assert_one_success_one_conflict(&results);

    let reference_count = state
        .assets()
        .metadata(&store, asset_id)
        .unwrap()
        .unwrap()
        .reference_count();
    let first_present = state
        .assets()
        .reference(&store, first_owner)
        .unwrap()
        .is_some();
    let second_present = state
        .assets()
        .reference(&store, second_owner)
        .unwrap()
        .is_some();
    assert!(
        (reference_count == 0 && !first_present && !second_present)
            || (reference_count == 2 && first_present && second_present)
    );
    assert!(sidecar_path.is_file());

    store.close().unwrap();
    let (reopened, state) = support::open(directory.path());
    assert_eq!(
        state
            .assets()
            .metadata(&reopened, asset_id)
            .unwrap()
            .unwrap()
            .reference_count(),
        reference_count
    );
    assert_eq!(
        state
            .assets()
            .reference(&reopened, first_owner)
            .unwrap()
            .is_some(),
        first_present
    );
    assert_eq!(
        state
            .assets()
            .reference(&reopened, second_owner)
            .unwrap()
            .is_some(),
        second_present
    );
    assert!(sidecar_path.is_file());
}
