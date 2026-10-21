use std::{
    cell::Cell,
    fs,
    io::Read,
    num::NonZeroUsize,
    os::windows::fs::OpenOptionsExt,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use beryl_backend::{
    ForegroundSessionConfig, GeneratedOutputTarget, GeneratedReaderAllowance,
    ManagedBackendLaunchSpec, ManagedBackendServer, WslSupervisorArtifact,
};
use beryl_home_store::{
    HomeDomainRequirements, HomeOpenCandidate, HomeOpenOptions, HomeSchemaVersion, SidecarError,
};
use beryl_model::{
    AdmittedHostPath, CasItemId, CasTurnId, PathFlavor, RuntimeId, RuntimeLaunchForm, RuntimeMode,
    RuntimeNativePath, WslDistributionName,
};
use sha2::{Digest, Sha256};

use super::{limit, namespace};

const TIMEOUT: Duration = Duration::from_secs(10);

#[test]
#[ignore = "requires exact root-supplied WSL qualification artifact and workload"]
fn native_original_namespace_source_supports_home_admission_reuse_and_cancellation() {
    let directory = tempfile::Builder::new()
        .prefix("beryl-generated-wsl-consumer-")
        .tempdir()
        .unwrap();
    eprintln!(
        "generated-output WSL fixture: {}",
        directory.path().display()
    );
    let root = directory.path().join("root");
    let tokens = directory.path().join("tokens");
    fs::create_dir(&root).unwrap();
    fs::create_dir(&tokens).unwrap();
    fs::write(root.join("generated-output-fixture"), []).unwrap();
    let bytes: Vec<u8> = (0..100_001).map(|index| (index % 251) as u8).collect();
    let path = root.join("generated.bin");
    fs::write(&path, &bytes).unwrap();
    let mode = RuntimeMode::Wsl(
        WslDistributionName::new(required("BERYL_WSL_TEST_DISTRIBUTION")).unwrap(),
    );
    let native = |value: &str| {
        RuntimeNativePath::from_admitted(mode.clone(), PathFlavor::Posix, value).unwrap()
    };
    let artifact_host = required("BERYL_WSL_TEST_ARTIFACT_HOST");
    let mut pin = fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(&artifact_host)
        .unwrap();
    let mut hash = Sha256::new();
    let mut page = [0; 8192];
    loop {
        let length = pin.read(&mut page).unwrap();
        if length == 0 {
            break;
        }
        hash.update(&page[..length]);
    }
    let artifact = Arc::new(
        WslSupervisorArtifact::from_verified_release(
            artifact_host.into(),
            required("BERYL_WSL_TEST_ARTIFACT_LINUX"),
            hash.finalize().into(),
            2,
            Arc::new(pin),
        )
        .unwrap(),
    );
    let launch = ManagedBackendLaunchSpec::new(
        RuntimeId::from_bytes([98; 16]),
        host(&required("BERYL_WSL_TEST_WORKLOAD_HOST")),
        mode.clone(),
        RuntimeLaunchForm::CodexCli,
        native(&required("BERYL_WSL_TEST_WORKLOAD_LINUX")),
        native(&projection(&root)),
        host(&canonical(&tokens)),
        native(&projection(&tokens)),
    )
    .unwrap()
    .with_wsl_supervisor_artifact(artifact);
    let mut server = ManagedBackendServer::launch(launch).unwrap();
    let connector = server.client_connector();
    let mut session = connector
        .connect_foreground_candidate(
            ForegroundSessionConfig::new(NonZeroUsize::new(8).unwrap()),
            TIMEOUT,
        )
        .unwrap();
    session.initialize_foreground(TIMEOUT).unwrap();
    let release = session
        .admit_release(Path::new(&projection(&root)), TIMEOUT)
        .unwrap();
    let loaded = session
        .start_thread(Path::new(&projection(&root)), TIMEOUT)
        .unwrap()
        .into_loaded();
    let home = HomeOpenCandidate::open(HomeOpenOptions::new(
        directory.path().join("home"),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap()
    .prepare_publication(HomeDomainRequirements::new())
    .unwrap()
    .publish()
    .unwrap();
    let cancellation = Arc::new(AtomicBool::new(false));
    let permit = || {
        session
            .authorize_generated_output(
                &release,
                &loaded,
                GeneratedOutputTarget::new(
                    [18; 16],
                    CasTurnId::new("generated-wsl-turn").unwrap(),
                    CasItemId::new("generated-wsl-item").unwrap(),
                ),
            )
            .unwrap()
    };
    let source = server
        .open_generated_output(
            permit(),
            &native(&projection(&path)),
            GeneratedReaderAllowance::new(2).unwrap(),
            cancellation.clone(),
        )
        .unwrap();
    let passes = Cell::new(0);
    let mut factory = || {
        passes.set(passes.get() + 1);
        Ok(source.cursor())
    };
    let admitted = home
        .admit_sidecar_stream(
            namespace(),
            source.length(),
            limit(),
            NonZeroUsize::new(257).unwrap(),
            &mut factory,
            || false,
        )
        .unwrap();
    assert_eq!(passes.get(), 3);
    assert_eq!(fs::read(admitted.path()).unwrap(), bytes);
    let reused = home
        .admit_sidecar_stream(
            namespace(),
            source.length(),
            limit(),
            NonZeroUsize::new(8192).unwrap(),
            &mut factory,
            || false,
        )
        .unwrap();
    assert_eq!(passes.get(), 5);
    assert_eq!(reused.address(), admitted.address());
    drop(factory);
    source.close().unwrap();
    let source = server
        .open_generated_output(
            permit(),
            &native(&projection(&path)),
            GeneratedReaderAllowance::new(2).unwrap(),
            cancellation.clone(),
        )
        .unwrap();
    let mut cursor = source.cursor();
    cursor.read_exact(&mut [0; 257]).unwrap();
    drop(cursor);
    cancellation.store(true, Ordering::Release);
    let result = home.admit_sidecar_stream(
        namespace(),
        source.length(),
        limit(),
        NonZeroUsize::new(257).unwrap(),
        || Ok(source.cursor()),
        || cancellation.load(Ordering::Acquire),
    );
    assert!(matches!(result, Err(SidecarError::Cancelled)));
    assert!(source.cursor().read(&mut [0; 1]).is_err());
    source.close().unwrap();
    server.shutdown().unwrap();
    assert!(!server.is_process_alive());
    assert_eq!(fs::read_dir(tokens).unwrap().count(), 0);
}

fn required(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("missing native qualification input {name}"))
}
fn host(path: &str) -> AdmittedHostPath {
    AdmittedHostPath::from_admitted(PathFlavor::Windows, path).unwrap()
}
fn canonical(path: &Path) -> String {
    fs::canonicalize(path)
        .unwrap()
        .to_str()
        .unwrap()
        .trim_start_matches(r"\\?\")
        .to_owned()
}
fn projection(path: &Path) -> String {
    let path = canonical(path);
    let bytes = path.as_bytes();
    assert!(bytes.len() >= 3 && bytes[1] == b':');
    format!(
        "/mnt/{}/{}",
        (bytes[0] as char).to_ascii_lowercase(),
        path[3..].replace('\\', "/")
    )
}
