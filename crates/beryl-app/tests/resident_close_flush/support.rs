use std::sync::Arc;

use beryl_app::{
    cas_projection::ProjectionServiceConfig,
    composer_host::SyndicComposerHost,
    composer_marker_seal::DraftMarkerSealService,
    main_window::{
        MainWindowComposerMarkerMetadataAuthority, MainWindowComposerSelectionIdentity,
        MainWindowComposerSlot, MainWindowComposerSubmissionRequestSource,
        MainWindowConversationComposerConfig, MainWindowConversationComposerMount,
        MainWindowConversationComposerService,
    },
};
use beryl_home_store::{CommandCancellation, HomeStore, MinimumTurnCaptureReserve};
use beryl_state::AssetState;
use gpui::{
    AppContext, Entity, InteractiveElement, IntoElement, ParentElement, Render, Styled, div, px,
};
use syndic_storage::SyndicStorage;

use super::widget_support::{self, fixture::Fixture};

pub struct MountRoot {
    pub mount: Entity<MainWindowConversationComposerMount>,
}

impl Render for MountRoot {
    fn render(&mut self, _: &mut gpui::Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
        div()
            .id("composer-frame")
            .debug_selector(|| "composer-frame".to_owned())
            .flex()
            .w(px(320.))
            .h(px(96.))
            .child(self.mount.clone())
    }
}

pub struct Mounted {
    pub root: Entity<MountRoot>,
    pub mount: Entity<MainWindowConversationComposerMount>,
    pub service: Arc<MainWindowConversationComposerService>,
    pub store: Arc<HomeStore>,
    pub storage: SyndicStorage,
    pub assets: AssetState,
    pub seals: DraftMarkerSealService,
    pub directory: tempfile::TempDir,
}

pub struct Host {
    pub host: SyndicComposerHost,
    pub store: HomeStore,
    pub storage: SyndicStorage,
    pub thread: beryl_model::SyndicThreadId,
    pub assets: AssetState,
    pub seals: DraftMarkerSealService,
    pub faults: beryl_home_store::test_faults::FaultController,
    pub directory: tempfile::TempDir,
}

pub fn host(name: &str, seed: u8) -> Host {
    let fixture = Fixture::new(name, seed);
    let thread = fixture.selected_thread;
    let assets = fixture.assets();
    let seals = fixture.marker_seals();
    let faults = fixture.faults.clone();
    let (directory, store, storage) = fixture.into_store();
    let (host, _) = super::composer::activated(
        storage.clone(),
        &store,
        thread,
        seed.wrapping_add(1),
        seed.wrapping_add(2),
    );
    Host {
        host,
        store,
        storage,
        thread,
        assets,
        seals,
        faults,
        directory,
    }
}

pub fn mounted<'a>(
    cx: &'a mut gpui::TestAppContext,
    name: &str,
    seed: u8,
) -> (Mounted, &'a mut gpui::VisualTestContext) {
    cx.update(gpui_text_input::ensure_text_input_bindings);
    let fixture = Fixture::new(name, seed);
    let claim = fixture.claims().0;
    let assets = fixture.assets();
    let seals = fixture.marker_seals();
    let marker_authority = MainWindowComposerMarkerMetadataAuthority::new(assets.clone());
    let window_id = fixture.window_id;
    let thread = fixture.selected_thread;
    let (directory, store, storage) = fixture.into_store();
    let mut host = SyndicComposerHost::new(storage.clone());
    host.test_activate(
        &store,
        widget_support::activation(thread, seed.wrapping_add(1), seed.wrapping_add(2), 1, 0),
        &CommandCancellation::new(),
    )
    .unwrap();
    let slot =
        MainWindowComposerSlot::new(window_id, claim, host, storage.clone(), marker_authority)
            .unwrap();
    let store = Arc::new(store);
    let service = Arc::new(MainWindowConversationComposerService::new(
        store.clone(),
        slot,
    ));
    let mounted_service = service.clone();
    let mounted_seals = seals.clone();
    let (root, cx) = cx.add_window_view(|window, cx| {
        let mount = cx.new(|cx| {
            MainWindowConversationComposerMount::new(
                mounted_service,
                Box::new(configure),
                mounted_seals,
                submission_source(),
                window,
                cx,
            )
            .unwrap()
        });
        MountRoot { mount }
    });
    let mount = root.read_with(cx, |root, _| root.mount.clone());
    drive(cx, 16);
    (
        Mounted {
            root,
            mount,
            service,
            store,
            storage,
            assets,
            seals,
            directory,
        },
        cx,
    )
}

fn configure(
    selection: MainWindowComposerSelectionIdentity,
) -> Result<MainWindowConversationComposerConfig, String> {
    let mut config = widget_support::widget_config(
        selection.binding().range_binding(),
        selection.binding().presentation_generation(),
    );
    config.viewport_extent = px(96.);
    MainWindowConversationComposerConfig::new(selection, config).map_err(|error| error.to_string())
}

fn submission_source() -> MainWindowComposerSubmissionRequestSource {
    MainWindowComposerSubmissionRequestSource::new(
        ProjectionServiceConfig::try_new(1, 4, MinimumTurnCaptureReserve::try_new(1).unwrap())
            .unwrap()
            .turn_start_admission_requirement(),
    )
}

pub fn drive(cx: &mut gpui::VisualTestContext, rounds: usize) {
    widget_support::drive(cx, rounds);
}

pub fn drive_until(
    cx: &mut gpui::VisualTestContext,
    stage: &str,
    mut ready: impl FnMut(&mut gpui::VisualTestContext) -> bool,
) {
    for _ in 0..512 {
        drive(cx, 1);
        if ready(cx) {
            return;
        }
    }
    panic!(" {stage} did not settle within 512 steps");
}

pub fn assert_history_preserved(
    current: syndic_storage::DraftEditHistoryFrontierReferenceV1,
    previous: syndic_storage::DraftEditHistoryFrontierReferenceV1,
) {
    assert_eq!(current.root(), previous.root());
    assert_eq!(
        current.candidate_generation(),
        previous.candidate_generation()
    );
    assert_eq!(current.frontier_revision(), previous.frontier_revision());
    assert_eq!(current.byte_budget(), previous.byte_budget());
    assert_eq!(
        current.retention_policy_revision(),
        previous.retention_policy_revision()
    );
    assert_eq!(current.availability(), previous.availability());
}

pub fn finish(mounted: Mounted, cx: &mut gpui::VisualTestContext) {
    cx.update(|window, _| window.remove_window());
    let Mounted {
        root,
        mount,
        service,
        store,
        storage,
        assets,
        seals,
        directory,
    } = mounted;
    drop((root, mount, service, store, storage, assets, seals));
    cx.run_until_parked();
    directory.close().expect(" fixture resources released");
}
