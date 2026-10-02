#![cfg(target_os = "windows")]

#[path = "../src/home_open.rs"]
mod home_open;

use beryl_app::bootstrap::HomeOpenOutcome;
use beryl_home_store::CommandCancellation;

#[test]
fn complete_private_registration_releases_home_for_reopening() {
    let directory = tempfile::tempdir().unwrap();
    for _ in 0..2 {
        match home_open::open(directory.path(), CommandCancellation::new()) {
            HomeOpenOutcome::Ready {
                candidate,
                state,
                syndic,
            } => {
                drop((state, syndic));
                candidate.close().unwrap();
            }
            _ => panic!("complete registration must return one private publication candidate"),
        }
    }
}

#[test]
fn cancelled_open_does_not_create_home() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("unopened");
    let cancellation = CommandCancellation::new();
    cancellation.cancel();
    assert!(matches!(
        home_open::open(&path, cancellation),
        HomeOpenOutcome::Failed { retained: None, .. }
    ));
    assert!(!path.exists());
}

#[test]
fn unusable_home_returns_failure_without_replacement() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("existing-file");
    std::fs::write(&path, b"preserve").unwrap();
    assert!(matches!(
        home_open::open(&path, CommandCancellation::new()),
        HomeOpenOutcome::Failed { retained: None, .. }
    ));
    assert_eq!(std::fs::read(path).unwrap(), b"preserve");
}
