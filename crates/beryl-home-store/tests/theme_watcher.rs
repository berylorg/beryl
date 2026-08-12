use std::{
    fs,
    num::NonZeroUsize,
    time::{Duration, Instant},
};

use beryl_home_store::{
    HomeOpenOptions, HomeSchemaVersion, HomeStore, StableThemeFileId, ThemeWatchError,
    ThemeWatchHint, ThemeWatchLimits,
};

fn limits(capacity: usize, entries: usize) -> ThemeWatchLimits {
    ThemeWatchLimits::new(
        Duration::from_millis(15),
        NonZeroUsize::new(capacity).unwrap(),
        NonZeroUsize::new(entries).unwrap(),
        64 * 1024,
        NonZeroUsize::new(31).unwrap(),
    )
    .unwrap()
}

fn open(path: &std::path::Path) -> HomeStore {
    HomeStore::open(HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT)).unwrap()
}

fn receive_until(
    subscription: &beryl_home_store::ThemeWatchSubscription,
    expected: ThemeWatchHint,
) {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        assert!(!remaining.is_zero(), "did not receive {expected:?}");
        if subscription
            .recv_timeout(remaining.min(Duration::from_millis(100)))
            .unwrap()
            == Some(expected.clone())
        {
            return;
        }
    }
}

#[test]
fn watcher_reports_manifest_and_valid_stable_document_create_write_and_delete() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(directory.path());
    let subscription = store.subscribe_theme_changes(limits(8, 16)).unwrap();
    let themes = directory.path().join("themes");
    let installed = themes.join("installed");
    fs::create_dir_all(&installed).unwrap();

    fs::write(themes.join("manifest.toml"), b"one").unwrap();
    receive_until(&subscription, ThemeWatchHint::ManifestChanged);
    fs::write(themes.join("manifest.toml"), b"two").unwrap();
    receive_until(&subscription, ThemeWatchHint::ManifestChanged);

    let id = StableThemeFileId::new("external-edit").unwrap();
    let document = installed.join("external-edit.toml");
    fs::write(&document, b"one").unwrap();
    receive_until(&subscription, ThemeWatchHint::DocumentChanged(id.clone()));
    fs::write(&document, b"two-two").unwrap();
    receive_until(&subscription, ThemeWatchHint::DocumentChanged(id.clone()));
    fs::remove_file(&document).unwrap();
    receive_until(&subscription, ThemeWatchHint::DocumentChanged(id));
}

#[test]
fn watcher_ignores_temporary_and_invalid_names_and_coalesces_duplicates() {
    let directory = tempfile::tempdir().unwrap();
    let installed = directory.path().join("themes/installed");
    fs::create_dir_all(&installed).unwrap();
    let store = open(directory.path());
    let subscription = store.subscribe_theme_changes(limits(8, 16)).unwrap();

    fs::write(installed.join(".document-staged"), b"temporary").unwrap();
    fs::write(installed.join("Invalid.toml"), b"invalid id").unwrap();
    std::thread::sleep(Duration::from_millis(75));
    assert_eq!(subscription.try_recv().unwrap(), None);

    let id = StableThemeFileId::new("coalesced").unwrap();
    let document = installed.join("coalesced.toml");
    fs::write(&document, b"one").unwrap();
    fs::write(&document, b"two").unwrap();
    fs::write(&document, b"three").unwrap();
    receive_until(&subscription, ThemeWatchHint::DocumentChanged(id));
    std::thread::sleep(Duration::from_millis(60));
    assert_eq!(subscription.try_recv().unwrap(), None);
}

#[test]
fn bounded_enumeration_and_queue_pressure_collapse_to_overflow() {
    let directory = tempfile::tempdir().unwrap();
    let installed = directory.path().join("themes/installed");
    fs::create_dir_all(&installed).unwrap();
    let store = open(directory.path());
    let subscription = store.subscribe_theme_changes(limits(1, 1)).unwrap();

    fs::write(installed.join("one.toml"), b"one").unwrap();
    fs::write(installed.join("two.toml"), b"two").unwrap();
    receive_until(&subscription, ThemeWatchHint::Overflow);
    assert_eq!(subscription.try_recv().unwrap(), None);
}

#[test]
fn one_lane_per_generation_releases_on_subscription_drop_and_store_drop() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(directory.path());
    let first = store.subscribe_theme_changes(limits(4, 8)).unwrap();
    assert!(matches!(
        store.subscribe_theme_changes(limits(4, 8)),
        Err(ThemeWatchError::AlreadySubscribed)
    ));
    drop(first);
    let second = store.subscribe_theme_changes(limits(4, 8)).unwrap();
    drop(store);
    assert!(matches!(
        second.recv_timeout(Duration::from_secs(1)),
        Err(ThemeWatchError::ShutDown)
    ));
}
