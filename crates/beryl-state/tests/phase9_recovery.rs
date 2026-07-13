mod support;

use std::num::NonZeroU64;

use beryl_home_store::{
    CommandError, HealthVerificationError, HomeCommand, HomeHealthState, ReadError,
    SidecarNamespace,
    test_faults::{FaultController, FaultPoint},
};
use beryl_model::{AssetId, RuntimeId, SyndicThreadId, WindowId};
use beryl_state::{
    AdmitBranchHandoffJob, ApplySettings, AssetMediaType, BerylState, CatalogPointReadLimit,
    CatalogRowExpectation, CreateAssetWithReference, CreateThreadMetadata, ExpectedSettingRevision,
    InitializeThreadlessWindow, PublishCatalogRow, ReplaceWindowClaim, SettingKey, SettingUpdate,
    SettingValue, ThreadMetadataKind, UnixMillis,
};
use tempfile::tempdir;

use support::phase9::{
    admission, asset_owner, catalog_facts, catalog_sources, open_with_faults, placement,
    sidecar_limit, target,
};
use support::{binding, execute, host_runtime};

fn create_setting(key: SettingKey, value: SettingValue) -> SettingUpdate {
    SettingUpdate::new(key, ExpectedSettingRevision::Absent, value)
}

#[test]
fn all_domains_reopen_fail_recover_and_reject_prior_generation_authority() {
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let (store, state) = open_with_faults(directory.path(), faults.clone());
    let home_id = store.home_id();

    execute(
        &store,
        state.runtime_roots().create_runtime_with_home_root(
            state.runtime_roots().revision(&store).unwrap(),
            host_runtime(1, 2, r"C:\Codex\codex.exe", r"C:\Work\beryl"),
        ),
    )
    .unwrap();

    let window_id = WindowId::from_bytes([3; 16]);
    execute(
        &store,
        state.session().initialize_threadless(
            state.session().revision(&store).unwrap(),
            InitializeThreadlessWindow::new(window_id, placement(3)),
        ),
    )
    .unwrap();

    let thread_id = SyndicThreadId::from_bytes([7; 16]);
    let mut cross_domain = HomeCommand::new(store.home_revision().unwrap());
    cross_domain
        .add(state.thread_metadata().create(
            state.thread_metadata().revision(&store).unwrap(),
            CreateThreadMetadata::new(
                thread_id,
                binding(1, 2, r"C:\Work\beryl"),
                ThreadMetadataKind::Ordinary,
            ),
        ))
        .unwrap();
    cross_domain
        .add(
            state.settings().apply(
                state.settings().revision(&store).unwrap(),
                ApplySettings::new(vec![create_setting(
                    SettingKey::ActiveThemeId,
                    SettingValue::active_theme_id("checkpoint-two").unwrap(),
                )])
                .unwrap(),
            ),
        )
        .unwrap();
    store.execute(cross_domain).unwrap();

    let initial_session = state.session().minimal_bootstrap(&store).unwrap().unwrap();
    execute(
        &store,
        state.session().replace_claim(
            state.session().revision(&store).unwrap(),
            ReplaceWindowClaim::new(
                initial_session.header().revision(),
                window_id,
                initial_session.windows()[0].revision(),
                None,
                target(1, 2),
                thread_id,
            ),
        ),
    )
    .unwrap();

    let admitted_job = admission(20);
    let job_id = admitted_job.job_id();
    execute(
        &store,
        state.durable_jobs().admit_branch_handoff(
            state.durable_jobs().revision(&store).unwrap(),
            AdmitBranchHandoffJob::new(admitted_job),
        ),
    )
    .unwrap();

    execute(
        &store,
        state.catalog().publish(
            state.catalog().revision(&store).unwrap(),
            PublishCatalogRow::new(
                thread_id,
                CatalogRowExpectation::Missing,
                catalog_sources(1),
                catalog_facts(7, 1, 100),
            )
            .unwrap(),
        ),
    )
    .unwrap();

    let sidecar = store
        .admit_sidecar(
            SidecarNamespace::new("images").unwrap(),
            b"checkpoint two image",
            sidecar_limit(),
        )
        .unwrap();
    let asset_id = AssetId::sha256_v1(
        sidecar.address().digest().as_bytes(),
        NonZeroU64::new(sidecar.address().length()).unwrap(),
    );
    let asset_revision = state.assets().revision(&store).unwrap();
    let first_asset = state
        .assets()
        .create_with_reference(
            asset_revision,
            sidecar,
            CreateAssetWithReference::new(
                asset_id,
                AssetMediaType::new("image/png").unwrap(),
                None,
                asset_revision.checked_next().unwrap(),
                asset_owner(30),
                UnixMillis::new(100),
            )
            .unwrap(),
        )
        .unwrap();
    let mut asset_command = HomeCommand::new(store.home_revision().unwrap());
    first_asset.add_to(&mut asset_command).unwrap();
    store.execute(asset_command).unwrap();

    let expected_runtime = state
        .runtime_roots()
        .runtime(&store, RuntimeId::from_bytes([1; 16]))
        .unwrap()
        .unwrap();
    let expected_metadata = state
        .thread_metadata()
        .metadata(&store, thread_id)
        .unwrap()
        .unwrap();
    let expected_job = state.durable_jobs().job(&store, job_id).unwrap().unwrap();
    let expected_catalog = state
        .catalog()
        .row(&store, thread_id, CatalogPointReadLimit::schema_maximum())
        .unwrap()
        .unwrap();
    let expected_asset = state.assets().metadata(&store, asset_id).unwrap().unwrap();
    assert_eq!(expected_asset.reference_count(), 1);

    store.close().unwrap();

    let (store, prior_state) = open_with_faults(directory.path(), faults.clone());
    assert_eq!(store.home_id(), home_id);
    let caller_snapshot = prior_state
        .session()
        .minimal_bootstrap(&store)
        .unwrap()
        .unwrap();
    let saved_header = caller_snapshot.header().clone();
    let saved_windows = caller_snapshot.windows().to_vec();
    assert_eq!(saved_windows.len(), 1);
    assert_eq!(saved_windows[0].window_id(), window_id);

    let prior_generation = store.health().generation().unwrap();
    let stale_sidecar = store
        .admit_sidecar(
            SidecarNamespace::new("images").unwrap(),
            b"stale generation image",
            sidecar_limit(),
        )
        .unwrap();
    let stale_asset_id = AssetId::sha256_v1(
        stale_sidecar.address().digest().as_bytes(),
        NonZeroU64::new(stale_sidecar.address().length()).unwrap(),
    );

    let stale_contribution = prior_state.settings().apply(
        prior_state.settings().revision(&store).unwrap(),
        ApplySettings::new(vec![create_setting(
            SettingKey::DeveloperInstructions,
            SettingValue::developer_instructions("prepared before recovery").unwrap(),
        )])
        .unwrap(),
    );
    let mut stale_command = HomeCommand::new(store.home_revision().unwrap());
    stale_command.add(stale_contribution).unwrap();

    let theme = prior_state
        .settings()
        .setting(&store, SettingKey::ActiveThemeId)
        .unwrap()
        .unwrap();
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let surfaced = execute(
        &store,
        prior_state.settings().apply(
            prior_state.settings().revision(&store).unwrap(),
            ApplySettings::new(vec![SettingUpdate::new(
                SettingKey::ActiveThemeId,
                ExpectedSettingRevision::Exact(theme.revision()),
                SettingValue::active_theme_id("indeterminate-until-recovery").unwrap(),
            )])
            .unwrap(),
        ),
    )
    .unwrap_err();
    assert!(matches!(surfaced, CommandError::Persistence { .. }));
    assert_eq!(store.health().state(), HomeHealthState::Verifying);
    assert!(matches!(
        prior_state.settings().revision(&store),
        Err(ReadError::HealthGate(_))
    ));
    assert_eq!(caller_snapshot.header(), &saved_header);
    assert_eq!(caller_snapshot.windows(), saved_windows.as_slice());

    faults.fail_next(FaultPoint::BeforeVerification);
    assert!(matches!(
        store.verify_health(),
        Err(HealthVerificationError::Persistence { .. })
    ));
    assert_eq!(store.health().state(), HomeHealthState::Failed);

    let recovery = store.recover_same_home().unwrap();
    assert!(recovery.generation() > prior_generation);
    assert_eq!(store.home_id(), home_id);
    assert_eq!(store.health().state(), HomeHealthState::Healthy);
    let current_state = BerylState::reacquire(&store).unwrap();

    assert!(matches!(
        prior_state.settings().revision(&store),
        Err(ReadError::ForeignDomain {
            domain: "beryl-settings"
        })
    ));
    assert!(matches!(
        store.execute(stale_command),
        Err(CommandError::ForeignDomain {
            domain: "beryl-settings"
        })
    ));

    let current_asset_revision = current_state.assets().revision(&store).unwrap();
    let stale_first_asset = current_state
        .assets()
        .create_with_reference(
            current_asset_revision,
            stale_sidecar,
            CreateAssetWithReference::new(
                stale_asset_id,
                AssetMediaType::new("image/png").unwrap(),
                None,
                current_asset_revision.checked_next().unwrap(),
                asset_owner(31),
                UnixMillis::new(101),
            )
            .unwrap(),
        )
        .unwrap();
    let mut stale_token_command = HomeCommand::new(store.home_revision().unwrap());
    stale_first_asset.add_to(&mut stale_token_command).unwrap();
    assert!(matches!(
        store.execute(stale_token_command),
        Err(CommandError::ForeignSidecar)
    ));

    execute(
        &store,
        current_state.settings().apply(
            current_state.settings().revision(&store).unwrap(),
            ApplySettings::new(vec![create_setting(
                SettingKey::DeveloperInstructions,
                SettingValue::developer_instructions("current generation accepted").unwrap(),
            )])
            .unwrap(),
        ),
    )
    .unwrap();
    assert_eq!(
        current_state
            .settings()
            .setting(&store, SettingKey::DeveloperInstructions)
            .unwrap()
            .unwrap()
            .value()
            .as_developer_instructions(),
        Some("current generation accepted")
    );

    let current_snapshot = current_state
        .session()
        .minimal_bootstrap(&store)
        .unwrap()
        .unwrap();
    assert_eq!(current_snapshot.header(), &saved_header);
    assert_eq!(current_snapshot.windows(), saved_windows.as_slice());
    assert_eq!(caller_snapshot.header(), &saved_header);
    assert_eq!(caller_snapshot.windows(), saved_windows.as_slice());

    store.close().unwrap();
    let (reopened, state) = support::open(directory.path());
    let final_snapshot = state
        .session()
        .minimal_bootstrap(&reopened)
        .unwrap()
        .unwrap();
    assert_eq!(final_snapshot.header(), &saved_header);
    assert_eq!(final_snapshot.windows(), saved_windows.as_slice());
    assert_eq!(
        state
            .runtime_roots()
            .runtime(&reopened, RuntimeId::from_bytes([1; 16]))
            .unwrap(),
        Some(expected_runtime)
    );
    assert_eq!(
        state
            .thread_metadata()
            .metadata(&reopened, thread_id)
            .unwrap(),
        Some(expected_metadata)
    );
    assert_eq!(
        state.durable_jobs().job(&reopened, job_id).unwrap(),
        Some(expected_job)
    );
    assert_eq!(
        state
            .catalog()
            .row(
                &reopened,
                thread_id,
                CatalogPointReadLimit::schema_maximum(),
            )
            .unwrap(),
        Some(expected_catalog)
    );
    assert_eq!(
        state
            .settings()
            .setting(&reopened, SettingKey::DeveloperInstructions)
            .unwrap()
            .unwrap()
            .value()
            .as_developer_instructions(),
        Some("current generation accepted")
    );
    assert_eq!(
        state.assets().metadata(&reopened, asset_id).unwrap(),
        Some(expected_asset)
    );
}
