#![cfg(feature = "test-faults")]

#[path = "theme_runtime_cases/support.rs"]
mod support;

use beryl_app::theme_runtime::{
    AppearanceCoordinatorConfig, GpuiAppearanceWindowSet, RepositoryAppearanceResult, ThemeRuntime,
    ThemeRuntimeConfig, ThemeRuntimeFailureClass,
};
use beryl_home_store::{
    CommandOutcome, HomeCommand, HomeOpenCandidate, HomeOpenOptions, HomeSchemaVersion, HomeStore,
    test_faults::{FaultController, FaultPoint},
};
use beryl_state::{
    ApplySettings, BerylState, ExpectedSettingRevision, InstallTheme, InstalledThemeId, SettingKey,
    SettingUpdate, SettingValue, ThemeAppearanceSource, ThemeChangeHint, ThemeDocument,
    ThemeManifestReadLimits, ThemeName, ThemePageLimits, ThemeParseMode, ThemeReferenceSnapshot,
    ThemeReferenceSnapshotProvider, ThemeReferenceSnapshotUnavailable, ThemeRepositoryCommand,
    ThemeRepositoryOperationOutcome,
};
use std::{
    collections::BTreeSet,
    num::{NonZeroU64, NonZeroUsize},
    sync::Arc,
    time::{Duration, Instant},
};

fn document(color: &str) -> String {
    format!(
        "schema = 1\nid = \"active\"\nname = \"Active\"\n[[role]]\nid = \"app.window\"\nbackground = \"{color}\"\n"
    )
}

fn manifest_limits() -> ThemeManifestReadLimits {
    ThemeManifestReadLimits::new(
        NonZeroUsize::new(4096).unwrap(),
        NonZeroUsize::new(16 * 1024).unwrap(),
        NonZeroUsize::new(256 * 1024).unwrap(),
    )
    .unwrap()
}

fn config(maximum_hints: usize) -> ThemeRuntimeConfig {
    ThemeRuntimeConfig::new(
        AppearanceCoordinatorConfig::new(NonZeroUsize::new(4).unwrap()),
        NonZeroU64::new(1024 * 1024).unwrap(),
        manifest_limits(),
        ThemePageLimits::new(
            NonZeroUsize::new(8).unwrap(),
            NonZeroUsize::new(4096).unwrap(),
        )
        .unwrap(),
        Duration::from_millis(10),
        NonZeroUsize::new(8).unwrap(),
        NonZeroUsize::new(32).unwrap(),
        NonZeroU64::new(1024 * 1024).unwrap(),
        NonZeroUsize::new(maximum_hints).unwrap(),
    )
}

struct NoReferences;
impl ThemeReferenceSnapshotProvider for NoReferences {
    fn current_theme_references(
        &self,
    ) -> Result<ThemeReferenceSnapshot, ThemeReferenceSnapshotUnavailable> {
        Err(ThemeReferenceSnapshotUnavailable)
    }
}

struct Fixture {
    _directory: tempfile::TempDir,
    store: HomeStore,
    state: BerylState,
    faults: FaultController,
}

impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let faults = FaultController::new();
        let mut candidate = HomeOpenCandidate::open_with_faults(
            HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
            faults.clone(),
        )
        .unwrap();
        let state = BerylState::register(&mut candidate).unwrap();
        let store = candidate
            .prepare_publication(BerylState::required_domains().unwrap())
            .unwrap()
            .publish()
            .unwrap();
        Self {
            _directory: directory,
            store,
            state,
            faults,
        }
    }

    fn start(&self, maximum_hints: usize) -> ThemeRuntime {
        let settings = self.state.settings();
        let active = settings
            .setting(&self.store, SettingKey::ActiveThemeId)
            .unwrap();
        ThemeRuntime::start(
            &self.store,
            self.state.themes(),
            settings.revision(&self.store).unwrap(),
            active.as_ref(),
            config(maximum_hints),
        )
        .unwrap()
    }

    fn path(&self, id: &str) -> std::path::PathBuf {
        self.store
            .canonical_path()
            .join("themes")
            .join("installed")
            .join(format!("{id}.toml"))
    }

    fn install_active(&self) {
        let service = self.state.themes();
        let observation = service
            .observe_repository(
                &self.store,
                NonZeroU64::new(1024 * 1024).unwrap(),
                manifest_limits(),
                None,
            )
            .unwrap();
        let command = ThemeRepositoryCommand::Install(
            InstallTheme::new(
                observation.manifest(),
                InstalledThemeId::new("active").unwrap(),
                ThemeName::new("Active").unwrap(),
                ThemeDocument::parse_bytes(
                    document("#102030").as_bytes(),
                    ThemeParseMode::StrictCandidate,
                )
                .unwrap(),
            )
            .unwrap(),
        );
        assert!(matches!(
            service
                .execute_command(
                    &self.store,
                    &observation,
                    &command,
                    NonZeroU64::new(1024 * 1024).unwrap(),
                    &NoReferences
                )
                .unwrap(),
            ThemeRepositoryOperationOutcome::Committed { .. }
        ));
        let settings = self.state.settings();
        let mut command = HomeCommand::new(self.store.home_revision().unwrap());
        command
            .add(
                settings.apply(
                    settings.revision(&self.store).unwrap(),
                    ApplySettings::new(vec![SettingUpdate::new(
                        SettingKey::ActiveThemeId,
                        ExpectedSettingRevision::Absent,
                        SettingValue::active_theme_id("active").unwrap(),
                    )])
                    .unwrap(),
                ),
            )
            .unwrap();
        assert!(matches!(
            self.store.execute(command),
            CommandOutcome::Committed {
                later_failure: None,
                ..
            }
        ));
    }
}

fn await_hints(runtime: &mut ThemeRuntime) -> Vec<ThemeChangeHint> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let hints = runtime.take_change_hints().unwrap();
        if !hints.is_empty() {
            return hints;
        }
        assert!(Instant::now() < deadline, "watcher produced no hint");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn on_worker<T: Send + 'static>(
    cx: &mut gpui::TestAppContext,
    work: impl FnOnce() -> T + Send + 'static,
) -> T {
    let worker = std::thread::spawn(work);
    let deadline = Instant::now() + Duration::from_secs(30);
    while !worker.is_finished() {
        assert!(
            Instant::now() < deadline,
            "theme watch worker did not settle"
        );
        cx.run_until_parked();
        std::thread::sleep(Duration::from_millis(1));
    }
    worker.join().unwrap()
}

#[gpui::test]
fn idle_and_failed_queue_handoff_on_publication_thread_never_reads_home_or_retires_runtime(
    cx: &mut gpui::TestAppContext,
) {
    let (fixture, mut runtime) = on_worker(cx, || {
        let fixture = Fixture::new();
        let runtime = fixture.start(2);
        (fixture, runtime)
    });
    let current = runtime.current().unwrap();
    let owner = cx.update(|app| {
        GpuiAppearanceWindowSet::new(current.clone(), NonZeroUsize::new(4).unwrap(), app)
    });
    let target = owner.read_with(cx, |owner, _| owner.target());
    runtime = on_worker(cx, move || {
        runtime.attach_publication_target(target).unwrap();
        runtime
    });
    fixture.faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(
        cx.update(|_| runtime.take_change_hints())
            .unwrap()
            .is_empty()
    );
    assert!(
        cx.update(|_| runtime.take_change_hints())
            .unwrap()
            .is_empty()
    );
    assert_eq!(runtime.diagnostics().watch_batches(), 0);
    let fixture = on_worker(cx, move || {
        assert!(
            fixture.store.home_revision().is_err(),
            "memory drain must leave the injected Home read fault unconsumed"
        );
        fixture
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match cx.update(|_| runtime.take_change_hints()) {
            Err(ThemeRuntimeFailureClass::Subscription) => break,
            Ok(hints) => assert!(hints.is_empty()),
            other => panic!("unexpected queue result: {other:?}"),
        }
        assert!(
            Instant::now() < deadline,
            "failed Home watcher did not close its queue"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(!runtime.diagnostics().retired());
    assert_eq!(runtime.diagnostics().state().active_subscriptions(), 1);
    assert!(Arc::ptr_eq(&runtime.current().unwrap(), &current));
    on_worker(cx, move || {
        runtime.retire();
        drop((runtime, fixture));
    });
    drop(owner);
}

#[test]
fn finite_handoff_preserves_distinct_pending_file_hints_across_calls() {
    let fixture = Fixture::new();
    std::fs::create_dir_all(fixture.path("first").parent().unwrap()).unwrap();
    for id in ["first", "second", "third", "fourth"] {
        std::fs::write(fixture.path(id), b"initial").unwrap();
    }
    let mut runtime = fixture.start(1);
    for id in ["first", "second", "third", "fourth"] {
        std::fs::write(fixture.path(id), b"changed bytes").unwrap();
    }
    let mut observed = BTreeSet::new();
    while observed.len() < 4 {
        let hints = await_hints(&mut runtime);
        assert_eq!(hints.len(), 1);
        if let ThemeChangeHint::DocumentChanged(id) = &hints[0] {
            observed.insert(id.as_str().to_owned());
        }
    }
    assert_eq!(
        observed,
        ["first", "fourth", "second", "third"]
            .map(str::to_owned)
            .into_iter()
            .collect()
    );
    assert_eq!(runtime.diagnostics().watch_batches(), 0);
    assert!(matches!(
        runtime.current().unwrap().prepared().source(),
        ThemeAppearanceSource::BuiltinFallback(_)
    ));
    runtime.retire();
}

#[test]
fn actual_watch_handoff_publishes_valid_edits_retains_invalid_and_accepts_next_valid() {
    let fixture = Fixture::new();
    fixture.install_active();
    let mut runtime = fixture.start(4);
    let adapter = support::TestAdapter::new(1);
    support::attach_test_adapter(&mut runtime, adapter.clone());
    let initial = runtime.current().unwrap();
    let home_revision = fixture.store.home_revision().unwrap();
    for (bytes, expected) in [
        (document("#405060"), RepositoryAppearanceResult::Published),
        (
            "schema = not-a-number\n".into(),
            RepositoryAppearanceResult::Retained(ThemeRuntimeFailureClass::DocumentInvalid),
        ),
        (document("#708090"), RepositoryAppearanceResult::Published),
    ] {
        let prior = runtime.current().unwrap();
        std::fs::write(fixture.path("active"), bytes).unwrap();
        let hints = await_hints(&mut runtime);
        assert!(hints.len() <= 4);
        assert!(
            Arc::ptr_eq(&runtime.current().unwrap(), &prior),
            "taking hints cannot publish appearance"
        );
        assert_eq!(
            runtime
                .consume_change_hints(&fixture.store, &hints)
                .unwrap()
                .appearance(),
            expected
        );
        let current = runtime.current().unwrap();
        assert!(matches!(
            current.prepared().source(),
            ThemeAppearanceSource::Installed(_)
        ));
        if expected == RepositoryAppearanceResult::Published {
            assert!(current.number() > prior.number());
        } else {
            assert!(Arc::ptr_eq(&current, &prior));
        }
        assert!(Arc::ptr_eq(&adapter.state.current(), &current));
        let duplicate = runtime
            .consume_change_hints(&fixture.store, &hints)
            .unwrap()
            .appearance();
        assert!(matches!(
            duplicate,
            RepositoryAppearanceResult::Unchanged | RepositoryAppearanceResult::Retained(_)
        ));
        assert!(Arc::ptr_eq(&runtime.current().unwrap(), &current));
    }
    assert!(runtime.current().unwrap().number() > initial.number());
    assert_eq!(fixture.store.home_revision().unwrap(), home_revision);
    assert_eq!(
        fixture
            .state
            .settings()
            .setting(&fixture.store, SettingKey::ActiveThemeId)
            .unwrap()
            .unwrap()
            .value(),
        &SettingValue::active_theme_id("active").unwrap()
    );
    runtime.retire();
}
