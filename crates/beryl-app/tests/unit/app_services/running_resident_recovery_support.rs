use crate::main_window::*;
use beryl_home_store::{CommandCancellation, HomeRecoveryCandidate, test_faults::FaultPoint};
use gpui::{AppContext, AsyncApp, Entity, WindowHandle};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

#[path = "../../syndic_composer_history/support.rs"]
mod composer;
#[path = "../../resident_close_flush/support.rs"]
mod mounted_support;
#[path = "../../pending_composer_activation/support.rs"]
pub(super) mod widget_support;

pub(super) struct Resident {
    pub window: WindowHandle<mounted_support::MountRoot>,
    pub composer: Entity<MainWindowConversationComposer>,
    pub close: MainWindowConversationComposerCloseTicket,
    pub candidate: HomeRecoveryCandidate,
    pub retired: MainWindowComposerRetiredClose,
    pub directory: tempfile::TempDir,
    pub mount: Entity<MainWindowConversationComposerMount>,
}

pub(super) async fn prepare(cx: &mut AsyncApp) -> Resident {
    let fixture = widget_support::fixture::Fixture::new("owned-resident-recovery", 194);
    let faults = fixture.faults.clone();
    let claim = fixture.claims().0;
    let assets = fixture.assets();
    let seals = fixture.marker_seals();
    let marker = MainWindowComposerMarkerMetadataAuthority::new(assets.clone());
    let window_id = fixture.window_id;
    let thread = fixture.selected_thread;
    let (directory, store, storage) = fixture.into_store();
    let mut host = crate::composer_host::SyndicComposerHost::new(storage.clone());
    host.test_activate(
        &store,
        widget_support::activation(thread, 195, 196, 1, 0),
        &CommandCancellation::new(),
    )
    .unwrap();
    let slot =
        MainWindowComposerSlot::new(window_id, claim, host, storage.clone(), marker).unwrap();
    let service = Arc::new(MainWindowConversationComposerService::new(
        store.service_reference(),
        slot,
    ));
    let mounted_service = service.clone();
    let window = cx
        .update(|app| {
            gpui_text_input::ensure_text_input_bindings(app);
            app.open_window(Default::default(), |window, app| {
                app.new(|cx| {
                    let mount = cx.new(|cx| {
                        MainWindowConversationComposerMount::new(
                            mounted_service,
                            Box::new(mounted_support::configure),
                            seals,
                            mounted_support::submission_source(),
                            window,
                            cx,
                        )
                        .unwrap()
                    });
                    mounted_support::MountRoot { mount }
                })
            })
            .unwrap()
        })
        .unwrap();
    let mount = window.update(cx, |root, _, _| root.mount.clone()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let composer = loop {
        if let Some(composer) = mount
            .read_with(cx, |mount, _| mount.contribution())
            .unwrap()
        {
            break composer;
        }
        assert!(Instant::now() < deadline);
        cx.background_executor()
            .timer(Duration::from_millis(5))
            .await;
    };
    let close = window
        .update(cx, |_, window, app| {
            mount
                .update(app, |mount, cx| mount.begin_window_close(window, cx))
                .unwrap()
        })
        .unwrap();
    loop {
        let ready = window
            .update(cx, |_, window, app| {
                mount
                    .update(app, |mount, cx| {
                        mount.advance_window_close(close.ticket, window, cx)
                    })
                    .unwrap()
            })
            .unwrap();
        let quiescent = composer
            .read_with(cx, |composer, app| {
                composer.gpui_input().read(app).is_quiescent()
            })
            .unwrap();
        if ready == MainWindowConversationComposerCloseAdvance::Ready && quiescent {
            break;
        }
        assert!(Instant::now() < deadline);
        cx.background_executor()
            .timer(Duration::from_millis(5))
            .await;
    }
    composer
        .update(cx, |composer, cx| {
            composer.test_set_shutdown_interaction_gated(true, cx)
        })
        .unwrap()
        .unwrap();
    let resources = mount
        .update(cx, |mount, cx| {
            assert!(
                mount
                    .fence_interrupted_exit_resident(close.ticket, cx)
                    .unwrap()
            );
            mount
                .detach_interrupted_exit_resources(close.ticket, cx)
                .unwrap()
                .unwrap()
        })
        .unwrap();
    drop(service);
    let retired = resources.retire().ok().unwrap();
    drop((storage, assets));
    let candidate = cx
        .background_executor()
        .spawn(async move {
            faults.fail_next(FaultPoint::BeforeReadConfirmation);
            assert!(store.home_revision().is_err());
            store.recover_same_home().unwrap()
        })
        .await;
    Resident {
        window,
        composer,
        close: close.ticket,
        candidate,
        retired,
        directory,
        mount,
    }
}

pub(super) fn environment(
    seed: gpui_text_input::RangeRestorationSeed,
    selection: MainWindowComposerSelectionIdentity,
    window: &gpui::Window,
) -> Result<
    (
        gpui_text_input::RangePrepublicationEnvironment,
        gpui_text_input::RangeSurfaceCharge,
    ),
    String,
> {
    use gpui_text_input::*;
    let mut config =
        widget_support::widget_config(seed.binding, selection.binding().presentation_generation());
    config.viewport_extent = gpui::px(96.);
    config.limits.max_realized_block_extent = config.viewport_extent;
    let capacity = RangeSurfaceCharge {
        bytes: config.limits.max_surface_bytes * 2,
        items: config.limits.max_surface_items * 2,
    };
    let cleanup = RangePrepublicationCleanupLedger::new(window.text_system(), 64).unwrap();
    Ok((
        RangePrepublicationEnvironment::new(13, config, window.text_system(), cleanup).unwrap(),
        capacity,
    ))
}
