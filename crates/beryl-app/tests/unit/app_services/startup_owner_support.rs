mod native_appearance {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/support/native_shell_appearance.rs"
    ));
}
use super::*;
use beryl_state::*;

pub(super) fn native_home() -> tempfile::TempDir {
    let (directory, candidate, state, _, _) = fixture();
    let store = candidate.publish().unwrap();
    native_appearance::install_native_theme(&store, &state);
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
