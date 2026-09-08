use std::num::NonZeroU64;

use beryl_app::composer_host::{
    ComposerHostActivationOutcome, ComposerHostActivationRequest, ComposerHostBinding,
    SyndicComposerHost,
};
use beryl_home_store::{
    CommandCancellation, CommandOutcome, HomeCommand, HomeOpenOptions, HomeSchemaVersion,
    HomeStore, MutationContribution, SidecarByteLimit, SidecarNamespace,
};
use beryl_model::{
    AssetId, ExecutionBinding, PathFlavor, RootId, RuntimeId, RuntimeMode, RuntimeNativePath,
    SyndicDraftId, SyndicThreadId,
};
use beryl_state::{AssetMediaType, AssetState, BerylState, PublishAssetMetadata};
use syndic_storage::{
    CreateThread, DraftEditHistoryPolicyV1, DraftEditorCandidateSessionIdV1,
    DraftPieceOperationIdV1, SyndicStorage, SyndicTimestamp,
};

pub struct Fixture {
    _home: tempfile::TempDir,
    pub store: HomeStore,
    pub storage: SyndicStorage,
    pub assets: AssetState,
    pub thread: SyndicThreadId,
    pub faults: beryl_home_store::test_faults::FaultController,
}

pub fn fixture(name: &str, seed: u8) -> Fixture {
    let home = tempfile::Builder::new().prefix(name).tempdir().unwrap();
    let faults = beryl_home_store::test_faults::FaultController::new();
    let mut store = HomeStore::open_with_faults(
        HomeOpenOptions::new(home.path(), HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let assets = BerylState::register(&mut store).unwrap().assets();
    let storage = SyndicStorage::register(&mut store).unwrap();
    let thread = SyndicThreadId::from_bytes([seed; 16]);
    let draft = SyndicDraftId::from_bytes([seed.wrapping_add(1); 16]);
    committed(execute(
        &store,
        storage.create_thread(
            storage.revision(&store).unwrap(),
            CreateThread::ordinary(
                thread,
                draft,
                execution(),
                SyndicTimestamp::from_unix_millis(1),
                DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
            ),
        ),
    ));
    Fixture {
        _home: home,
        store,
        storage,
        assets,
        thread,
        faults,
    }
}

pub fn activate(
    storage: SyndicStorage,
    store: &HomeStore,
    thread: SyndicThreadId,
    session: u8,
    operation: u8,
) -> (SyndicComposerHost, ComposerHostBinding) {
    let mut host = SyndicComposerHost::new(storage);
    let ComposerHostActivationOutcome::Activated { binding, .. } = host
        .test_activate(
            store,
            ComposerHostActivationRequest::new(
                thread,
                DraftEditorCandidateSessionIdV1::from_bytes([session; 16]),
                DraftPieceOperationIdV1::from_bytes([operation; 16]),
                NonZeroU64::MIN,
                None,
                Box::new([]),
            ),
            &CommandCancellation::new(),
        )
        .unwrap()
    else {
        panic!("activation did not yield a composer binding");
    };
    (host, binding)
}

pub fn publish_image_asset(store: &HomeStore, assets: AssetState, bytes: &[u8]) -> AssetId {
    let sidecar = store
        .admit_sidecar(
            SidecarNamespace::new("images").unwrap(),
            bytes,
            SidecarByteLimit::new(NonZeroU64::new(1_024).unwrap()),
        )
        .unwrap();
    let asset = AssetId::sha256_v1(
        sidecar.address().digest().as_bytes(),
        NonZeroU64::new(sidecar.address().length()).unwrap(),
    );
    let expected = assets.revision(store).unwrap();
    let contribution = assets
        .publish_metadata(
            expected,
            sidecar,
            PublishAssetMetadata::new(
                asset,
                AssetMediaType::new("image/png").unwrap(),
                None,
                expected.checked_next().unwrap(),
            ),
        )
        .unwrap();
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    contribution.add_to(&mut command).unwrap();
    committed(store.execute(command));
    asset
}

fn execute(store: &HomeStore, contribution: MutationContribution) -> CommandOutcome {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    store.execute(command)
}

fn committed(outcome: CommandOutcome) {
    assert!(
        matches!(
            outcome,
            CommandOutcome::Committed {
                later_failure: None,
                ..
            }
        ),
        "command did not commit cleanly: {outcome:?}"
    );
}

fn execution() -> ExecutionBinding {
    ExecutionBinding::new(
        RuntimeId::from_bytes([171; 16]),
        RootId::from_bytes([172; 16]),
        RuntimeNativePath::from_admitted(
            RuntimeMode::host(),
            PathFlavor::Windows,
            "C:\\beryl-app-",
        )
        .unwrap(),
    )
}
