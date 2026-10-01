use crate::main_window::*;
use crate::theme_runtime::GpuiAppearanceWindowSet;
use crate::window_acquisition::*;
use beryl_home_store::{
    CommandCancellation, HomeRecoveryCandidate, HomeStore, test_faults::FaultPoint,
};
use beryl_model::{
    ExecutionBinding, RootId, RuntimeId, RuntimeMode, RuntimeNativePath, SyndicDraftId,
    SyndicThreadId, WindowBounds, WindowDisplayState, WindowId, WindowPlacement,
};
use beryl_state::{BerylState, RememberedTarget};
use gpui::{AppContext, AsyncApp, Entity, WindowHandle};
use shell_support::{native_path, placement};
use std::{
    num::NonZeroUsize,
    sync::Arc,
    time::{Duration, Instant},
};
use syndic_storage::{
    DraftEditHistoryPolicyV1, DraftEditorCandidateSessionIdV1,
    DraftEditorCandidateSessionReadOutcomeV1, SyndicStorage, SyndicTimestamp,
};

#[path = "../../pending_composer_activation/support.rs"]
pub(super) mod composer_support;
#[path = "../../main_window_shell/support.rs"]
mod home_support;
#[path = "../../support/native_shell_appearance.rs"]
mod native_appearance;
#[path = "../../initial_composer/support.rs"]
mod shell_support;
pub(super) use composer_support as widget_support;

pub(in super::super) fn selected_home() -> tempfile::TempDir {
    selected_home_with_windows(1)
}

pub(in super::super) fn selected_home_with_windows(count: u8) -> tempfile::TempDir {
    let fixture = shell_support::Fixture::new(194);
    native_appearance::install_native_theme(&fixture.store, &fixture.state);
    for index in 0..count {
        drop(fixture.acquire(195 + index));
    }
    let shell_support::Fixture {
        directory,
        store,
        state,
        storage,
        service,
        process,
        ..
    } = fixture;
    drop((state, storage, service, process));
    Arc::try_unwrap(store).ok().unwrap().close().unwrap();
    directory
}

pub(in super::super) fn selected_inputs() -> crate::app_services::MainWindowServiceInputs {
    crate::app_services::MainWindowServiceInputs {
        request_source: Arc::new(|_, _| panic!("selected fixture must restore")),
        activation_source: Arc::new(|_| panic!("selected fixture must restore")),
        restored_activation_source: Arc::new(|record| {
            Ok((
                composer_support::activation(
                    record.selected_thread().unwrap().thread_id(),
                    201,
                    202,
                    1,
                    0,
                ),
                composer_support::fixture::operation_id(203),
            ))
        }),
        configurator_source: Arc::new(|| Box::new(configure)),
    }
}

pub(super) struct Resident {
    pub window: WindowHandle<MainWindowShellRoot>,
    pub composer: Entity<MainWindowConversationComposer>,
    pub close: MainWindowConversationComposerCloseTicket,
    pub candidate: HomeRecoveryCandidate,
    pub retired: MainWindowComposerRetiredClose,
    pub directory: tempfile::TempDir,
    pub mount: Entity<MainWindowConversationComposerMount>,
    pub drafts: crate::running_owner::RunningShutdownDrafts,
    pub shell: MainWindowShell,
}

pub(super) async fn prepare(cx: &mut AsyncApp) -> Resident {
    prepare_inner(cx, false).await.0
}

pub(super) async fn prepare_for_binding(
    cx: &mut AsyncApp,
) -> (Resident, Entity<GpuiAppearanceWindowSet>) {
    prepare_inner(cx, true).await
}

async fn prepare_inner(
    cx: &mut AsyncApp,
    published: bool,
) -> (Resident, Entity<GpuiAppearanceWindowSet>) {
    let (fixture, prepared) = cx
        .background_executor()
        .spawn(async move {
            let fixture = shell_support::Fixture::new(194);
            if published {
                native_appearance::install_native_theme(&fixture.store, &fixture.state);
            }
            let mut initial = fixture.begin(195);
            initial.advance(&CommandCancellation::new()).unwrap();
            let appearance = crate::theme_runtime::AppearanceCoordinator::new(
                crate::theme_runtime::AppearanceCoordinatorConfig::new(
                    NonZeroUsize::new(4).unwrap(),
                ),
                native_appearance::system_font_appearance(&fixture.state),
            )
            .current();
            let prepared =
                shell_support::prepared_shell_with_appearance(&fixture, initial, appearance);
            (fixture, prepared)
        })
        .await;
    let (mut shell, appearance) = cx
        .update(|app| {
            gpui_text_input::ensure_text_input_bindings(app);
            let appearance = GpuiAppearanceWindowSet::new(
                prepared.appearance().clone(),
                NonZeroUsize::new(4).unwrap(),
                app,
            );
            let shell = GpuiMainWindowShellHost::new(app, appearance.clone())
                .construct_hidden(prepared)
                .unwrap_or_else(|_| panic!("shell construction"));
            (shell, appearance)
        })
        .unwrap();
    let window = shell.window();
    window
        .update(cx, |_, window, cx| window.publish(cx).unwrap())
        .unwrap();
    let mount = window
        .read_with(cx, |root, _| {
            root.controller().unwrap().composer_mount().unwrap()
        })
        .unwrap();
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
    if published {
        while !mount
            .read_with(cx, |mount, app| mount.selected_first_presentable(app))
            .unwrap()
        {
            assert!(Instant::now() < deadline);
            cx.background_executor()
                .timer(Duration::from_millis(5))
                .await;
        }
        cx.update(|app| {
            shell.gate_startup_interaction(app).unwrap();
            shell.publish(app).unwrap();
            MainWindowShell::release_startup_interaction(std::slice::from_ref(&shell), app)
                .unwrap();
        })
        .unwrap();
    }
    let mut draft = window
        .update(cx, |root, window, cx| {
            root.set_shutdown_interaction_gated(true, cx).unwrap();
            root.begin_shutdown_draft(window, cx).unwrap()
        })
        .unwrap();
    let close = draft.test_ticket().unwrap();
    loop {
        let ready = window
            .update(cx, |root, window, cx| {
                root.advance_shutdown_draft(&draft, window, cx).unwrap()
            })
            .unwrap();
        let quiescent = composer
            .read_with(cx, |composer, app| {
                composer.gpui_input().read(app).is_quiescent()
            })
            .unwrap();
        if ready
            == MainWindowShutdownDraftAdvance::Resident(
                MainWindowConversationComposerCloseAdvance::Ready,
            )
            && quiescent
        {
            break;
        }
        assert!(Instant::now() < deadline);
        cx.background_executor()
            .timer(Duration::from_millis(5))
            .await;
    }
    let shell_support::Fixture {
        directory,
        store,
        faults,
        state,
        storage,
        service,
        ..
    } = fixture;
    drop((state, storage, service));
    let store = cx
        .background_executor()
        .spawn(async move {
            faults.fail_next(FaultPoint::BeforeReadConfirmation);
            assert!(store.home_revision().is_err());
            store
        })
        .await;
    let retired = loop {
        let retired = window
            .update(cx, |root, _, cx| {
                if !root.retire_shutdown_draft(&mut draft, cx).unwrap() {
                    return None;
                }
                mount
                    .update(cx, |mount, cx| {
                        mount.take_interrupted_exit_retirement(close, cx)
                    })
                    .unwrap()
            })
            .unwrap();
        if let Some(retired) = retired {
            break retired;
        }
        assert!(Instant::now() < deadline);
        cx.background_executor()
            .timer(Duration::from_millis(5))
            .await;
    };
    let candidate = cx
        .background_executor()
        .spawn(async move {
            Arc::try_unwrap(store)
                .ok()
                .unwrap()
                .recover_same_home()
                .unwrap()
        })
        .await;
    (
        Resident {
            window,
            composer,
            close,
            candidate,
            retired,
            directory,
            mount,
            drafts: crate::running_owner::RunningShutdownDrafts::test_recovery_drafts(
                window, draft,
            ),
            shell,
        },
        appearance,
    )
}

pub(in super::super) use shell_support::config as configure;

pub(in super::super) fn recovery_configuration(
    selection: MainWindowComposerSelectionIdentity,
) -> Result<
    (
        MainWindowConversationComposerConfig,
        gpui_text_input::RangeSurfaceCharge,
    ),
    String,
> {
    let config = configure(selection)?;
    let current = config.native_lineage_current();
    let capacity = gpui_text_input::RangeSurfaceCharge {
        bytes: current.available_capacity.bytes * 2,
        items: current.available_capacity.items * 2,
    };
    Ok((config, capacity))
}

pub(in super::super) fn environment(
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
