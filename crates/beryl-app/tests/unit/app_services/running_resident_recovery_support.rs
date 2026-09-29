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
    let (fixture, prepared) = cx
        .background_executor()
        .spawn(async {
            let fixture = shell_support::Fixture::new(194);
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
    let shell = cx
        .update(|app| {
            gpui_text_input::ensure_text_input_bindings(app);
            let appearance = GpuiAppearanceWindowSet::new(
                prepared.appearance().clone(),
                NonZeroUsize::new(4).unwrap(),
                app,
            );
            GpuiMainWindowShellHost::new(app, appearance)
                .construct_hidden(prepared)
                .unwrap_or_else(|_| panic!("shell construction"))
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
    Resident {
        window,
        composer,
        close,
        candidate,
        retired,
        directory,
        mount,
        drafts: crate::running_owner::RunningShutdownDrafts::test_recovery_drafts(window, draft),
        shell,
    }
}

pub(super) use shell_support::config as configure;

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
