mod support;

use std::num::NonZeroU64;

use beryl_home_store::{CommandOutcome, HomeCommand, SidecarNamespace};
use beryl_model::{AssetId, SyndicThreadId, WindowId};
use beryl_state::{
    AdmitBranchHandoffJob, ApplySettings, AssetMediaType, CatalogRowExpectation,
    ExpectedSettingRevision, InitializeThreadlessWindow, PublishAssetMetadata, PublishCatalogRow,
    SettingKey, SettingUpdate, SettingValue,
};
use tempfile::tempdir;

use support::phase9::{admission, catalog_facts, catalog_sources, placement, sidecar_limit};
use support::{execute, host_runtime, open};

#[test]
fn beryl_domains_project_only_their_affected_receipt_revision() {
    let directory = tempdir().unwrap();
    let (store, state) = open(directory.path());
    let expected = state
        .runtime_roots()
        .revision(&store)
        .unwrap()
        .checked_next()
        .unwrap();
    let outcome = execute(
        &store,
        state.runtime_roots().create_runtime_with_home_root(
            state.runtime_roots().revision(&store).unwrap(),
            host_runtime(1, 2, r"C:\Codex\codex.exe", r"C:\Users\operator"),
        ),
    );
    let receipt = match outcome {
        CommandOutcome::Committed {
            receipt,
            later_failure: None,
        } => receipt,
        outcome => panic!("expected committed runtime receipt, got {outcome:?}"),
    };

    assert_eq!(receipt.generation(), store.health().generation().unwrap());
    assert_eq!(
        state
            .runtime_roots()
            .committed_revision(&store, &receipt)
            .unwrap(),
        Some(expected)
    );
    assert_eq!(
        state
            .session()
            .committed_revision(&store, &receipt)
            .unwrap(),
        None
    );
    assert_eq!(
        state
            .settings()
            .committed_revision(&store, &receipt)
            .unwrap(),
        None
    );
    assert_eq!(
        state
            .durable_jobs()
            .committed_revision(&store, &receipt)
            .unwrap(),
        None
    );
    assert_eq!(
        state
            .catalog()
            .committed_revision(&store, &receipt)
            .unwrap(),
        None
    );
    assert_eq!(
        state.assets().committed_revision(&store, &receipt).unwrap(),
        None
    );
}

#[test]
fn one_receipt_projects_every_affected_beryl_domain_revision() {
    let directory = tempdir().unwrap();
    let (store, state) = open(directory.path());
    let expected_runtime = state
        .runtime_roots()
        .revision(&store)
        .unwrap()
        .checked_next()
        .unwrap();
    let expected_session = state
        .session()
        .revision(&store)
        .unwrap()
        .checked_next()
        .unwrap();
    let expected_settings = state
        .settings()
        .revision(&store)
        .unwrap()
        .checked_next()
        .unwrap();
    let expected_jobs = state
        .durable_jobs()
        .revision(&store)
        .unwrap()
        .checked_next()
        .unwrap();
    let expected_catalog = state
        .catalog()
        .revision(&store)
        .unwrap()
        .checked_next()
        .unwrap();
    let asset_revision = state.assets().revision(&store).unwrap();
    let expected_assets = asset_revision.checked_next().unwrap();

    let sidecar = store
        .admit_sidecar(
            SidecarNamespace::new("images").unwrap(),
            b"receipt projection image",
            sidecar_limit(),
        )
        .unwrap();
    let asset_id = AssetId::sha256_v1(
        sidecar.address().digest().as_bytes(),
        NonZeroU64::new(sidecar.address().length()).unwrap(),
    );
    let first_asset = state
        .assets()
        .publish_metadata(
            asset_revision,
            sidecar,
            PublishAssetMetadata::new(
                asset_id,
                AssetMediaType::new("image/png").unwrap(),
                None,
                expected_assets,
            ),
        )
        .unwrap();

    let thread_id = SyndicThreadId::from_bytes([7; 16]);
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(state.runtime_roots().create_runtime_with_home_root(
            state.runtime_roots().revision(&store).unwrap(),
            host_runtime(1, 2, r"C:\Codex\codex.exe", r"C:\Work\beryl"),
        ))
        .unwrap();
    command
        .add(state.session().initialize_threadless(
            state.session().revision(&store).unwrap(),
            InitializeThreadlessWindow::new(WindowId::from_bytes([3; 16]), placement(3)),
        ))
        .unwrap();
    command
        .add(
            state.settings().apply(
                state.settings().revision(&store).unwrap(),
                ApplySettings::new(vec![SettingUpdate::new(
                    SettingKey::ActiveThemeId,
                    ExpectedSettingRevision::Absent,
                    SettingValue::active_theme_id("receipt-projection").unwrap(),
                )])
                .unwrap(),
            ),
        )
        .unwrap();
    command
        .add(state.durable_jobs().admit_branch_handoff(
            state.durable_jobs().revision(&store).unwrap(),
            AdmitBranchHandoffJob::new(admission(20)),
        ))
        .unwrap();
    command
        .add(
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
    first_asset.add_to(&mut command).unwrap();

    let receipt = match store.execute(command) {
        CommandOutcome::Committed {
            receipt,
            later_failure: None,
        } => receipt,
        outcome => panic!("expected committed receipt command, got {outcome:?}"),
    };
    assert_eq!(
        state
            .runtime_roots()
            .committed_revision(&store, &receipt)
            .unwrap(),
        Some(expected_runtime)
    );
    assert_eq!(
        state
            .session()
            .committed_revision(&store, &receipt)
            .unwrap(),
        Some(expected_session)
    );
    assert_eq!(
        state
            .settings()
            .committed_revision(&store, &receipt)
            .unwrap(),
        Some(expected_settings)
    );
    assert_eq!(
        state
            .durable_jobs()
            .committed_revision(&store, &receipt)
            .unwrap(),
        Some(expected_jobs)
    );
    assert_eq!(
        state
            .catalog()
            .committed_revision(&store, &receipt)
            .unwrap(),
        Some(expected_catalog)
    );
    assert_eq!(
        state.assets().committed_revision(&store, &receipt).unwrap(),
        Some(expected_assets)
    );
}
