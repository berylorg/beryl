use super::*;

pub(super) struct ScaleSetup {
    pub selected_thread: beryl_model::SyndicThreadId,
    pub target_thread: beryl_model::SyndicThreadId,
    pub third_thread: beryl_model::SyndicThreadId,
    pub target_claim: beryl_state::WindowClaimSelection,
    pub third_claim: beryl_state::WindowClaimSelection,
    pub marker_asset: beryl_model::AssetId,
    pub assets: beryl_state::AssetState,
    pub marker_seals: beryl_app::composer_marker_seal::DraftMarkerSealService,
    pub directory: tempfile::TempDir,
    pub store: Arc<beryl_home_store::HomeStore>,
    pub storage: syndic_storage::SyndicStorage,
    pub service: Arc<MainWindowConversationComposerService>,
}

#[inline(never)]
pub(super) fn prepare_scale_fixture() -> ScaleSetup {
    let fixture = Fixture::new("mounted-scale", 191);
    let (selected_claim, target_claim) = fixture.claims();
    let (third_thread, third_claim, selected_claim) =
        create_third_target(&fixture, 191, selected_claim);
    let window_id = fixture.window_id;
    let selected_thread = fixture.selected_thread;
    let target_thread = fixture.target_thread;
    let mut host = SyndicComposerHost::new(fixture.storage.clone());
    assert!(matches!(
        host.test_activate(
            &fixture.store,
            mounted_activation(selected_thread, 11, 12, 1, 0),
            &CommandCancellation::new(),
        )
        .unwrap(),
        ComposerHostActivationOutcome::Activated { .. }
    ));
    seed_large_published_draft(&fixture, target_thread);
    let marker_asset = publish_image_asset(&fixture, b"same-anchor-marker");
    let assets = fixture.assets();
    let marker_seals = fixture.marker_seals();
    let marker_authority = MainWindowComposerMarkerMetadataAuthority::new(assets.clone());
    let (directory, store, storage) = fixture.into_store();
    let slot = MainWindowComposerSlot::new(
        window_id,
        selected_claim,
        host,
        storage.clone(),
        marker_authority,
    )
    .unwrap();
    let store = Arc::new(store);
    let service = Arc::new(MainWindowConversationComposerService::new(
        store.clone(),
        slot,
    ));
    ScaleSetup {
        selected_thread,
        target_thread,
        third_thread,
        target_claim,
        third_claim,
        marker_asset,
        assets,
        marker_seals,
        directory,
        store,
        storage,
        service,
    }
}
