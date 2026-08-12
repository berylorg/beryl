use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use beryl_home_store::{HomeOpenError, HomeOpenOptions, HomeSchemaVersion, HomeStore};
use wait_timeout::ChildExt;

const FIXTURE_MODE: &str = "BERYL_HOME_OWNERSHIP_FIXTURE";
const FIXTURE_HOME: &str = "BERYL_HOME_OWNERSHIP_HOME";
const FIXTURE_READY: &str = "BERYL_HOME_OWNERSHIP_READY";
const FIXTURE_RELEASE: &str = "BERYL_HOME_OWNERSHIP_RELEASE";
const CHILD_TIMEOUT: Duration = Duration::from_secs(10);

fn open(path: impl Into<PathBuf>) -> Result<HomeStore, HomeOpenError> {
    HomeStore::open(HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT))
}

#[test]
fn lock_holder_fixture() {
    if env::var_os(FIXTURE_MODE).is_none() {
        return;
    }

    let home = PathBuf::from(env::var_os(FIXTURE_HOME).expect("fixture home"));
    let ready = PathBuf::from(env::var_os(FIXTURE_READY).expect("fixture ready"));
    let release = PathBuf::from(env::var_os(FIXTURE_RELEASE).expect("fixture release"));
    let store = open(home).expect("fixture owns home");
    fs::write(ready, b"ready").expect("publish fixture readiness");

    let deadline = Instant::now() + CHILD_TIMEOUT;
    while !release.exists() {
        assert!(Instant::now() < deadline, "fixture release timed out");
        thread::sleep(Duration::from_millis(10));
    }
    store.close().expect("fixture releases ownership");
}

#[test]
fn concurrent_process_observes_typed_busy_until_orderly_release() {
    let directory = tempfile::tempdir().expect("temp directory");
    let home = directory.path().join("home");
    let mut holder = Holder::spawn(&home, directory.path());

    let error = open(&home).expect_err("second process must observe busy");
    assert!(matches!(error, HomeOpenError::Busy { .. }));

    holder.release_orderly();
    let reopened = open(&home).expect("lock is reusable after orderly release");
    reopened.close().expect("close reopened home");
}

#[test]
fn process_death_leaves_a_reusable_lock_file() {
    let directory = tempfile::tempdir().expect("temp directory");
    let home = directory.path().join("home");
    let mut holder = Holder::spawn(&home, directory.path());
    let lock_path = home.join("home.lock");
    assert!(lock_path.is_file());

    holder.kill();
    assert!(lock_path.is_file(), "lock path survives owner death");

    let reopened = open(&home).expect("OS released dead process lock");
    reopened.close().expect("close reopened home");
    assert!(lock_path.is_file(), "stale path is never deleted");
}

#[test]
fn retained_lock_handle_does_not_prevent_lock_path_rename() {
    let directory = tempfile::tempdir().expect("temp directory");
    let home = directory.path().join("home");
    let lock_path = home.join("home.lock");
    let moved_lock_path = home.join("moved-home.lock");
    let store = open(&home).expect("open home");

    fs::rename(&lock_path, &moved_lock_path)
        .expect("the lifetime lock handle does not retain the lock path against rename");
    fs::rename(&moved_lock_path, &lock_path).expect("restore lock path before orderly close");

    store.close().expect("close home");
}

struct Holder {
    child: Option<Child>,
    release: PathBuf,
}

impl Holder {
    fn spawn(home: &Path, fixture_directory: &Path) -> Self {
        let ready = fixture_directory.join("holder.ready");
        let release = fixture_directory.join("holder.release");
        let mut child = Command::new(env::current_exe().expect("current test executable"))
            .arg("--exact")
            .arg("lock_holder_fixture")
            .arg("--nocapture")
            .env(FIXTURE_MODE, "1")
            .env(FIXTURE_HOME, home)
            .env(FIXTURE_READY, &ready)
            .env(FIXTURE_RELEASE, &release)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn ownership fixture");

        let deadline = Instant::now() + CHILD_TIMEOUT;
        while !ready.exists() {
            if let Some(status) = child.try_wait().expect("poll fixture") {
                panic!("ownership fixture exited before readiness: {status}");
            }
            assert!(Instant::now() < deadline, "fixture readiness timed out");
            thread::sleep(Duration::from_millis(10));
        }

        Self {
            child: Some(child),
            release,
        }
    }

    fn release_orderly(&mut self) {
        fs::write(&self.release, b"release").expect("request orderly release");
        let status = self.wait().expect("fixture exits before timeout");
        assert!(status.success(), "ownership fixture failed: {status}");
    }

    fn kill(&mut self) {
        let child = self.child.as_mut().expect("live fixture");
        child.kill().expect("kill ownership fixture");
        let _ = self.wait().expect("killed fixture exits before timeout");
    }

    fn wait(&mut self) -> Option<std::process::ExitStatus> {
        let child = self.child.as_mut().expect("live fixture");
        let status = child.wait_timeout(CHILD_TIMEOUT).expect("wait for fixture");
        if status.is_some() {
            self.child = None;
        }
        status
    }
}

impl Drop for Holder {
    fn drop(&mut self) {
        if let Some(child) = self.child.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
