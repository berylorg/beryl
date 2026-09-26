use super::*;
use beryl_state::*;

struct NoReferences;
impl ThemeReferenceSnapshotProvider for NoReferences {
    fn current_theme_references(
        &self,
    ) -> Result<ThemeReferenceSnapshot, ThemeReferenceSnapshotUnavailable> {
        Err(ThemeReferenceSnapshotUnavailable)
    }
}

pub(super) fn native_home() -> tempfile::TempDir {
    let (directory, candidate, state, _, _) = fixture();
    let store = candidate.publish().unwrap();
    let mut source = String::from("schema = 1\nid = \"native-test\"\nname = \"Native Test\"\n");
    for (_, role) in canonical_theme_schema().roles() {
        if role.property(ThemePropertyId::FontFamily).is_some() {
            source.push_str(&format!(
                "\n[[role]]\nid = \"{}\"\nfont_family = \".SystemUIFont\"\n",
                role.id().as_str()
            ));
        }
    }
    let themes = state.themes();
    let max = NonZeroU64::new(1024 * 1024).unwrap();
    let limits = ThemeManifestReadLimits::new(
        NonZeroUsize::new(4096).unwrap(),
        NonZeroUsize::new(16 * 1024).unwrap(),
        NonZeroUsize::new(256 * 1024).unwrap(),
    )
    .unwrap();
    let observation = themes
        .observe_repository(&store, max, limits, None)
        .unwrap();
    let install = ThemeRepositoryCommand::Install(
        InstallTheme::new(
            observation.manifest(),
            InstalledThemeId::new("native-test").unwrap(),
            ThemeName::new("Native Test").unwrap(),
            ThemeDocument::parse_bytes(source.as_bytes(), ThemeParseMode::StrictCandidate).unwrap(),
        )
        .unwrap(),
    );
    assert!(matches!(
        themes
            .execute_command(&store, &observation, &install, max, &NoReferences)
            .unwrap(),
        ThemeRepositoryOperationOutcome::Committed { .. }
    ));
    let settings = state.settings();
    let mut command = beryl_home_store::HomeCommand::new(store.home_revision().unwrap());
    command
        .add(
            settings.apply(
                settings.revision(&store).unwrap(),
                ApplySettings::new(vec![SettingUpdate::new(
                    SettingKey::ActiveThemeId,
                    ExpectedSettingRevision::Absent,
                    SettingValue::active_theme_id("native-test").unwrap(),
                )])
                .unwrap(),
            ),
        )
        .unwrap();
    assert!(matches!(
        store.execute(command),
        beryl_home_store::CommandOutcome::Committed { .. }
    ));
    store.close().unwrap();
    directory
}

pub(super) fn open(path: &std::path::Path) -> StartupHomeOpen {
    let mut candidate =
        HomeOpenCandidate::open(HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT)).unwrap();
    let state = BerylState::register(&mut candidate).unwrap();
    let syndic = SyndicStorage::register(&mut candidate).unwrap();
    let candidate = candidate
        .prepare_publication(
            BerylState::required_domains()
                .unwrap()
                .merge(SyndicStorage::required_domains().unwrap())
                .unwrap(),
        )
        .unwrap();
    StartupHomeOpen::Ready {
        candidate,
        state,
        syndic,
    }
}

pub(super) async fn dispose_running(mut running: startup_owner::StartedProcess, cx: &mut AsyncApp) {
    assert!(running.startup_surface.is_none());
    let (sender, receiver) = futures_channel::oneshot::channel();
    cx.update(|app| {
        running.windows.test_dispose(
            move |result, _| {
                assert!(
                    result.retained.is_none(),
                    "test teardown retains no native or command custody"
                );
                sender.send(()).unwrap();
            },
            app,
        )
    })
    .unwrap();
    receiver.await.unwrap();
    running
        .appearance
        .update(cx, |appearance, _| appearance.retire())
        .unwrap();
    cx.background_executor()
        .spawn(async move {
            close(&mut running.services);
        })
        .await;
}

pub(super) fn watchdog(app: &mut gpui::App) {
    app.spawn(async move |cx| {
        cx.background_executor()
            .timer(Duration::from_secs(15))
            .await;
        panic!("native startup test completion deadline");
    })
    .detach();
}
