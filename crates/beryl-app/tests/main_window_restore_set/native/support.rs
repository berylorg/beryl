use super::*;
use windows::{Win32::UI::WindowsAndMessaging::FindWindowW, core::PCWSTR};

#[path = "../../support/shell_desktop_flight.rs"]
mod flight;
pub(super) use flight::native as windows_native;

#[derive(Clone, Copy)]
pub(super) enum Kind {
    Restored,
    Replacement,
    Threadless,
}

pub(super) struct NativeFixture {
    pub fixture: Fixture,
    pub lifetime: RestoredWindowServiceTestLifetime,
    pub prepared: PreparedNativeMainWindowRestoreSet,
    pub appearance: Arc<AppearanceGeneration>,
    pub saved: beryl_state::MinimalSessionBootstrap,
    pub ids: Vec<WindowId>,
    pub cancellation: CommandCancellation,
}

pub(super) fn fixture(kind: Kind, seed: u8) -> NativeFixture {
    let fixture = match kind {
        Kind::Threadless => zero_runtime(None),
        Kind::Restored | Kind::Replacement => Fixture::new(seed),
    };
    if matches!(kind, Kind::Restored) {
        drop(fixture.acquire(seed + 1));
        drop(fixture.acquire(seed + 2));
    } else if matches!(kind, Kind::Replacement) {
        let acquired = fixture.acquire(seed + 1);
        let window = acquired.window_id();
        drop(acquired);
        fixture.remove_session_window(window);
    }
    let (services, _) = creation_support::services(&fixture);
    let appearance = AppearanceCoordinator::new(
        AppearanceCoordinatorConfig::new(NonZeroUsize::new(256).unwrap()),
        flight::system_font_appearance(&fixture.state),
    )
    .current();
    let (attempt, lifetime) = attempt(&fixture);
    let work = MainWindowRestoreSet::new(
        services,
        attempt,
        activation_source(),
        appearance.clone(),
        WindowId::from_bytes([240; 16]),
        placement(),
    )
    .unwrap();
    let cancellation = work.cancellation();
    let prepared = prepare(work);
    let ids = prepared
        .members()
        .iter()
        .map(|member| member.window_id())
        .collect();
    let saved = snapshot(&fixture);
    let prepared = prepared
        .prepare_native()
        .unwrap_or_else(|failure| panic!("{}", failure.error));
    NativeFixture {
        fixture,
        lifetime,
        prepared,
        appearance,
        saved,
        ids,
        cancellation,
    }
}

pub(super) fn raw_handle(index: usize, shell: &MainWindowShell, app: &mut App) -> usize {
    let title = format!("Beryl restore set {} {index}", std::process::id());
    shell
        .window()
        .update(app, |_, window, _| window.set_window_title(&title))
        .unwrap();
    let wide = title.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
    unsafe { FindWindowW(None, PCWSTR(wide.as_ptr())) }
        .unwrap()
        .0 as usize
}

pub(super) fn assert_gated(shell: &MainWindowShell, expected: bool, app: &App) {
    shell
        .window()
        .read_with(app, |root, app| {
            assert_eq!(root.startup_interaction_gated(), expected);
            if let Some(composer) = root
                .controller()
                .unwrap()
                .composer_mount()
                .and_then(|mount| mount.read(app).contribution())
            {
                assert_eq!(
                    composer.read(app).gpui_input().read(app).is_enabled(),
                    !expected
                );
            }
        })
        .unwrap();
}

pub(super) async fn wait_for<T>(completion: &Rc<RefCell<Option<T>>>, cx: &mut AsyncApp) -> T {
    let deadline = Instant::now() + Duration::from_secs(30);
    while completion.borrow().is_none() {
        assert!(
            Instant::now() < deadline,
            "native restore-set completion timeout"
        );
        windows_native::pump(cx).await;
    }
    completion.borrow_mut().take().unwrap()
}

pub(super) fn change_saved_placement(store: &HomeStore, state: &BerylState, id: WindowId) {
    let before = state.session().minimal_bootstrap(store).unwrap().unwrap();
    let member = before
        .windows()
        .iter()
        .find(|member| member.window_id() == id)
        .unwrap();
    execute(
        store,
        state.session().update_placement(
            state.session().revision(store).unwrap(),
            beryl_state::UpdateWindowPlacement::new(
                before.header().revision(),
                id,
                member.revision(),
                WindowPlacement::new(
                    WindowBounds::new(40, 50, 800, 600).unwrap(),
                    WindowDisplayState::Normal,
                    None,
                    None,
                ),
            ),
        ),
    );
}
