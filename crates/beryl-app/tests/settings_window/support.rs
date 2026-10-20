use std::{
    num::{NonZeroU64, NonZeroUsize},
    sync::Arc,
    time::{Duration, Instant},
};

use beryl_app::{
    settings_service::{SettingsProcessOwner, SettingsService},
    settings_window::SettingsWindowController,
    theme_runtime::{
        AppearanceCoordinatorConfig, GpuiAppearanceWindowSet, SettingsThemeBroker, ThemeRuntime,
        ThemeRuntimeConfig,
    },
};
use beryl_home_store::{HomeOpenCandidate, HomeOpenOptions, HomeSchemaVersion, HomeStore};
use beryl_state::{BerylState, ThemeManifestReadLimits, ThemePageLimits};
use gpui::{AppContext as _, Entity, TestAppContext};

pub struct Fixture {
    pub broker: SettingsThemeBroker,
    pub runtime: ThemeRuntime,
    pub service: Arc<SettingsService>,
    pub owner: SettingsProcessOwner,
    pub state: BerylState,
    pub store: HomeStore,
    _directory: tempfile::TempDir,
}

impl Fixture {
    pub fn new() -> Self {
        Self::with_themes(0)
    }

    pub fn with_themes(count: usize) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let mut candidate = HomeOpenCandidate::open(HomeOpenOptions::new(
            directory.path(),
            HomeSchemaVersion::CURRENT,
        ))
        .unwrap();
        let state = BerylState::register(&mut candidate).unwrap();
        let store = candidate
            .prepare_publication(BerylState::required_domains().unwrap())
            .unwrap()
            .publish()
            .unwrap();
        let owner = SettingsProcessOwner::new(store.home_id());
        if count != 0 {
            install_themes(&store, &state, count);
        }
        let service = Arc::new(
            SettingsService::prepare(
                store.service_reference(),
                state.settings(),
                owner.clone(),
                1,
            )
            .unwrap(),
        );
        let runtime = ThemeRuntime::start(
            &store,
            state.themes(),
            state.settings().revision(&store).unwrap(),
            state
                .settings()
                .setting(&store, beryl_state::SettingKey::ActiveThemeId)
                .unwrap()
                .as_ref(),
            config(),
        )
        .unwrap();
        let source = runtime.settings_source().unwrap();
        service.bind_theme_source(source.clone()).unwrap();
        let broker = SettingsThemeBroker::start(store.service_reference(), source).unwrap();
        let baseline = service.snapshot().unwrap();
        broker
            .set_active_selection(Some((
                beryl_state::ThemeSettingsIdentity::new(
                    broker.reader().home(),
                    baseline.identity().settings_revision,
                    baseline.record_revision(beryl_state::SettingKey::ActiveThemeId),
                ),
                baseline.applied().active_theme().unwrap().cloned(),
            )))
            .unwrap();
        Self {
            broker,
            runtime,
            service,
            owner,
            state,
            store,
            _directory: directory,
        }
    }

    pub fn mount(
        &self,
        cx: &mut TestAppContext,
    ) -> (
        Entity<SettingsWindowController>,
        Entity<GpuiAppearanceWindowSet>,
    ) {
        let current = self.runtime.current().unwrap();
        cx.update(|app| {
            let appearance =
                GpuiAppearanceWindowSet::new(current.clone(), NonZeroUsize::new(4).unwrap(), app);
            let window = SettingsWindowController::preheat(
                self.service.clone(),
                self.broker.reader(),
                appearance.clone(),
                current,
                app,
            )
            .unwrap();
            (window, appearance)
        })
    }

    pub fn dispose(mut self) {
        self.broker.retire();
        self.runtime.retire();
        self.service.retire().unwrap();
    }

    pub fn attach_appearance(
        mut self,
        cx: &mut TestAppContext,
        appearance: &Entity<GpuiAppearanceWindowSet>,
    ) -> Self {
        let target = appearance.read_with(cx, |owner, _| owner.target());
        on_worker(cx, move || {
            self.runtime.attach_publication_target(target).unwrap();
            self
        })
    }

    #[track_caller]
    pub fn consume_repository(
        mut self,
        cx: &mut TestAppContext,
        owner: &Entity<SettingsWindowController>,
    ) -> Self {
        let completion = self
            .broker
            .take_completion()
            .expect("original repository completion remains process-owned");
        self.consume_repository_completion(cx, owner, completion)
    }

    pub fn consume_repository_completion(
        mut self,
        cx: &mut TestAppContext,
        owner: &Entity<SettingsWindowController>,
        completion: beryl_app::theme_runtime::ThemeSettingsCompletion,
    ) -> Self {
        self = on_worker(cx, move || {
            self.runtime
                .consume_repository_completion(&self.store, completion)
                .unwrap();
            self
        });
        cx.update(|app| owner.update(app, |owner, cx| owner.set_appearance_busy(false, cx)));
        self
    }

    pub fn consume_settings(
        mut self,
        cx: &mut TestAppContext,
        owner: &Entity<SettingsWindowController>,
        proof: beryl_app::theme_runtime::ConfirmedSettingsTheme,
    ) -> Self {
        self = on_worker(cx, move || {
            self.runtime
                .consume_settings_outcome(
                    &self.store,
                    beryl_app::theme_runtime::SettingsThemeOutcome::Committed(proof),
                )
                .unwrap();
            let baseline = self.service.snapshot().unwrap();
            self.broker
                .set_active_selection(Some((
                    beryl_state::ThemeSettingsIdentity::new(
                        self.broker.reader().home(),
                        baseline.identity().settings_revision,
                        baseline.record_revision(beryl_state::SettingKey::ActiveThemeId),
                    ),
                    baseline.applied().active_theme().unwrap().cloned(),
                )))
                .unwrap();
            self
        });
        cx.update(|app| owner.update(app, |owner, cx| owner.set_appearance_busy(false, cx)));
        self
    }

    pub fn documents(&self) -> Vec<beryl_state::ThemeObservedDocument> {
        use beryl_state::*;
        let themes = self.state.themes();
        let limits = ThemeManifestReadLimits::new(
            NonZeroUsize::new(4096).unwrap(),
            NonZeroUsize::new(16 * 1024).unwrap(),
            NonZeroUsize::new(256 * 1024).unwrap(),
        )
        .unwrap();
        let observation = themes
            .observe_repository(
                &self.store,
                NonZeroU64::new(1024 * 1024).unwrap(),
                limits,
                None,
            )
            .unwrap();
        let mut session = themes
            .open_manifest(
                &self.store,
                &observation,
                NonZeroU64::new(1024 * 1024).unwrap(),
                limits,
            )
            .unwrap();
        let mut cursor = ThemeManifestCursor::first(observation.manifest());
        let mut result = Vec::new();
        loop {
            let page = session
                .read_page(
                    cursor,
                    ThemePageLimits::new(
                        NonZeroUsize::new(32).unwrap(),
                        NonZeroUsize::new(16 * 1024).unwrap(),
                    )
                    .unwrap(),
                )
                .unwrap();
            for index in 0..page.records().len() {
                let selection = page.selection(index).unwrap();
                result.push(
                    themes
                        .load_document(&self.store, &observation, &selection, None)
                        .unwrap(),
                );
            }
            match page.next() {
                Some(next) => cursor = next,
                None => break,
            }
        }
        result
    }

    pub fn has_installed_name(&self, expected: &str) -> bool {
        use beryl_state::*;
        let themes = self.state.themes();
        let limits = ThemeManifestReadLimits::new(
            NonZeroUsize::new(4096).unwrap(),
            NonZeroUsize::new(16 * 1024).unwrap(),
            NonZeroUsize::new(256 * 1024).unwrap(),
        )
        .unwrap();
        let observation = themes
            .observe_repository(
                &self.store,
                NonZeroU64::new(1024 * 1024).unwrap(),
                limits,
                None,
            )
            .unwrap();
        let mut session = themes
            .open_manifest(
                &self.store,
                &observation,
                NonZeroU64::new(1024 * 1024).unwrap(),
                limits,
            )
            .unwrap();
        let mut cursor = ThemeManifestCursor::first(observation.manifest());
        loop {
            let page = session
                .read_page(
                    cursor,
                    ThemePageLimits::new(
                        NonZeroUsize::new(32).unwrap(),
                        NonZeroUsize::new(16 * 1024).unwrap(),
                    )
                    .unwrap(),
                )
                .unwrap();
            if page
                .records()
                .iter()
                .any(|record| record.name().as_str() == expected)
            {
                return true;
            }
            match page.next() {
                Some(next) => cursor = next,
                None => return false,
            }
        }
    }
}

pub fn draw(cx: &mut TestAppContext, window: &gpui_settings_window::SettingsWindowHandle) {
    for _ in 0..4 {
        cx.run_until_parked();
        cx.update(|app| {
            app.update_window(window.window_handle().into(), |_, window, app| {
                window.draw(app).clear()
            })
            .unwrap()
        });
    }
}

pub fn page_action(
    cx: &mut TestAppContext,
    window: &gpui_settings_window::SettingsWindowHandle,
    page: &str,
    action: &str,
) -> bool {
    let result = window
        .window_handle()
        .update(cx, |view, _, cx| {
            view.request_page_action_for_test(page.into(), action.into(), cx)
        })
        .unwrap();
    cx.run_until_parked();
    result
}

#[track_caller]
pub fn committed_repository_completion(
    cx: &mut TestAppContext,
    fixture: &Fixture,
) -> beryl_app::theme_runtime::ThemeSettingsCompletion {
    let mut completion = None;
    wait_for(cx, |_| {
        completion = fixture.broker.take_completion();
        completion.is_some()
    });
    let completion = completion.unwrap();
    assert!(
        matches!(
            completion.repository.as_deref(),
            Some(beryl_state::ThemeRepositoryOperationOutcome::Committed { .. })
        ),
        "original repository operation did not commit: not_started={:?}, outcome={:?}",
        completion.not_started,
        completion.repository
    );
    completion
}

pub fn config() -> ThemeRuntimeConfig {
    ThemeRuntimeConfig::new(
        AppearanceCoordinatorConfig::new(NonZeroUsize::new(4).unwrap()),
        NonZeroU64::new(1024 * 1024).unwrap(),
        ThemeManifestReadLimits::new(
            NonZeroUsize::new(4096).unwrap(),
            NonZeroUsize::new(16 * 1024).unwrap(),
            NonZeroUsize::new(256 * 1024).unwrap(),
        )
        .unwrap(),
        ThemePageLimits::new(
            NonZeroUsize::new(32).unwrap(),
            NonZeroUsize::new(16 * 1024).unwrap(),
        )
        .unwrap(),
        Duration::from_millis(20),
        NonZeroUsize::new(8).unwrap(),
        NonZeroUsize::new(32).unwrap(),
        NonZeroU64::new(1024 * 1024).unwrap(),
        NonZeroUsize::new(4).unwrap(),
    )
}

pub fn on_worker<T: Send + 'static>(
    cx: &mut TestAppContext,
    work: impl FnOnce() -> T + Send + 'static,
) -> T {
    let worker = std::thread::Builder::new()
        .name("settings-test-source".into())
        .stack_size(16 * 1024 * 1024)
        .spawn(work)
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    while !worker.is_finished() {
        assert!(
            Instant::now() < deadline,
            "settings source worker did not settle"
        );
        cx.run_until_parked();
        std::thread::sleep(Duration::from_millis(1));
    }
    worker.join().unwrap()
}

pub fn settle(cx: &mut TestAppContext, owner: &Entity<SettingsWindowController>) {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        cx.run_until_parked();
        cx.executor().advance_clock(Duration::from_millis(25));
        if !owner.read_with(cx, |owner, _| owner.exit_is_gated()) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "original settings operation did not settle"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[track_caller]
pub fn wait_for(cx: &mut TestAppContext, mut ready: impl FnMut(&mut TestAppContext) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        cx.run_until_parked();
        if ready(cx) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "mounted Settings route did not become ready"
        );
        cx.executor().advance_clock(Duration::from_millis(25));
        std::thread::sleep(Duration::from_millis(1));
    }
}

struct NoReferences;
impl beryl_state::ThemeReferenceSnapshotProvider for NoReferences {
    fn current_theme_references(
        &self,
    ) -> Result<beryl_state::ThemeReferenceSnapshot, beryl_state::ThemeReferenceSnapshotUnavailable>
    {
        Err(beryl_state::ThemeReferenceSnapshotUnavailable)
    }
}

fn install_themes(store: &HomeStore, state: &BerylState, count: usize) {
    use beryl_state::*;
    let themes = state.themes();
    for index in 0..count {
        let observation = themes
            .observe_repository(
                store,
                NonZeroU64::new(1024 * 1024).unwrap(),
                ThemeManifestReadLimits::new(
                    NonZeroUsize::new(4096).unwrap(),
                    NonZeroUsize::new(16 * 1024).unwrap(),
                    NonZeroUsize::new(256 * 1024).unwrap(),
                )
                .unwrap(),
                None,
            )
            .unwrap();
        let id = format!("theme-{index}");
        let document=ThemeDocument::parse_bytes(format!("schema = 1\nid = \"{id}\"\nname = \"Theme {index}\"\n[[role]]\nid = \"app.window\"\nbackground = \"#102030\"\n").as_bytes(),ThemeParseMode::StrictCandidate).unwrap();
        let command = ThemeRepositoryCommand::Install(
            InstallTheme::new(
                observation.manifest(),
                InstalledThemeId::new(id).unwrap(),
                ThemeName::new(format!("Theme {index}")).unwrap(),
                document,
            )
            .unwrap(),
        );
        assert!(matches!(
            themes
                .execute_command(
                    store,
                    &observation,
                    &command,
                    NonZeroU64::new(1024 * 1024).unwrap(),
                    &NoReferences
                )
                .unwrap(),
            ThemeRepositoryOperationOutcome::Committed { .. }
        ));
    }
    let settings = state.settings();
    let mut command = beryl_home_store::HomeCommand::new(store.home_revision().unwrap());
    command
        .add(
            settings.apply(
                settings.revision(store).unwrap(),
                ApplySettings::new(vec![SettingUpdate::new(
                    SettingKey::ActiveThemeId,
                    ExpectedSettingRevision::Absent,
                    SettingValue::active_theme_id("theme-0").unwrap(),
                )])
                .unwrap(),
            ),
        )
        .unwrap();
    assert!(matches!(
        store.execute(command),
        beryl_home_store::CommandOutcome::Committed { .. }
    ));
}

pub fn route(
    cx: &mut TestAppContext,
    window: &gpui_settings_window::SettingsWindowHandle,
    id: &str,
) {
    window
        .window_handle()
        .update(cx, |view, window, cx| {
            view.select_section_for_test(id.into(), window, cx)
        })
        .unwrap();
    cx.run_until_parked();
}

pub fn edit(
    cx: &mut TestAppContext,
    window: &gpui_settings_window::SettingsWindowHandle,
    key: beryl_state::SettingKey,
    value: &str,
) {
    window
        .window_handle()
        .update(cx, |view, _, cx| {
            view.replace_field_text_for_test(&key.stable_id().into(), value, cx)
        })
        .unwrap();
    cx.run_until_parked();
}
