use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};

use beryl_backend::{
    ForegroundSessionConfig, GeneratedOutputSource, GeneratedOutputTarget,
    GeneratedReaderAllowance, LoadedThreadSession, ManagedBackendLaunchSpec,
    ManagedBackendReleaseAdmission, ManagedBackendServer, ManagedBackendSession,
};
use beryl_home_store::{
    HomeDomainRequirements, HomeOpenCandidate, HomeOpenOptions, HomeSchemaVersion, HomeStore,
};
use beryl_model::{
    AdmittedHostPath, CasItemId, CasTurnId, PathFlavor, RuntimeId, RuntimeLaunchForm, RuntimeMode,
    RuntimeNativePath,
};

const TIMEOUT: Duration = Duration::from_secs(10);

pub struct Fixture {
    pub home: HomeStore,
    pub server: ManagedBackendServer,
    pub session: ManagedBackendSession,
    pub loaded: LoadedThreadSession,
    pub release: ManagedBackendReleaseAdmission,
    pub cancelled: Arc<AtomicBool>,
    pub directory: tempfile::TempDir,
    _probe: ManagedBackendSession,
}

impl Fixture {
    pub fn new() -> Self {
        let directory = tempfile::Builder::new()
            .prefix("beryl-generated-output-source-")
            .tempdir()
            .unwrap();
        eprintln!("generated-output fixture: {}", directory.path().display());
        let root = directory.path().join("root");
        let tokens = directory.path().join("tokens");
        fs::create_dir(&root).unwrap();
        fs::create_dir(&tokens).unwrap();
        fs::write(root.join("fixture-mode"), "projection-lifetime").unwrap();
        let executable = canonical(Path::new(env!("CARGO_BIN_EXE_managed-runtime-fixture")));
        let launch = ManagedBackendLaunchSpec::new(
            RuntimeId::from_bytes([99; 16]),
            host(&executable),
            RuntimeMode::Host,
            RuntimeLaunchForm::CodexCli,
            native(&executable),
            native(&canonical(&root)),
            host(&canonical(&tokens)),
            native(&canonical(&tokens)),
        )
        .unwrap();
        let server = ManagedBackendServer::launch(launch).unwrap();
        let connector = server.client_connector();
        let mut probe = connector
            .connect_foreground_candidate(config(), TIMEOUT)
            .unwrap();
        probe.initialize_foreground(TIMEOUT).unwrap();
        probe.admit_release(&root, TIMEOUT).unwrap();
        let mut session = connector
            .connect_foreground_candidate(config(), TIMEOUT)
            .unwrap();
        session.initialize_foreground(TIMEOUT).unwrap();
        let release = session.admit_release(&root, TIMEOUT).unwrap();
        let loaded = session.start_thread(&root, TIMEOUT).unwrap().into_loaded();
        let home = HomeOpenCandidate::open(HomeOpenOptions::new(
            directory.path().join("home"),
            HomeSchemaVersion::CURRENT,
        ))
        .unwrap()
        .prepare_publication(HomeDomainRequirements::new())
        .unwrap()
        .publish()
        .unwrap();
        Self {
            home,
            server,
            session,
            loaded,
            release,
            cancelled: Arc::new(AtomicBool::new(false)),
            directory,
            _probe: probe,
        }
    }

    pub fn path(&self) -> PathBuf {
        self.directory.path().join("generated.bin")
    }

    pub fn open(&mut self) -> GeneratedOutputSource {
        let target = GeneratedOutputTarget::new(
            [17; 16],
            CasTurnId::new("generated-turn").unwrap(),
            CasItemId::new("generated-item").unwrap(),
        );
        let permit = self
            .session
            .authorize_generated_output(&self.release, &self.loaded, target)
            .unwrap();
        self.server
            .open_generated_output(
                permit,
                &native(&canonical(&self.path())),
                GeneratedReaderAllowance::new(2).unwrap(),
                self.cancelled.clone(),
            )
            .unwrap_or_else(|_| panic!("original authenticated source should open"))
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.server
            .shutdown()
            .expect("original managed process must join");
    }
}

fn config() -> ForegroundSessionConfig {
    ForegroundSessionConfig::new(std::num::NonZeroUsize::new(8).unwrap())
}
fn host(path: &str) -> AdmittedHostPath {
    AdmittedHostPath::from_admitted(PathFlavor::Windows, path).unwrap()
}
fn native(path: &str) -> RuntimeNativePath {
    RuntimeNativePath::from_admitted(RuntimeMode::Host, PathFlavor::Windows, path).unwrap()
}
fn canonical(path: &Path) -> String {
    fs::canonicalize(path)
        .unwrap()
        .to_str()
        .unwrap()
        .trim_start_matches(r"\\?\")
        .to_owned()
}
