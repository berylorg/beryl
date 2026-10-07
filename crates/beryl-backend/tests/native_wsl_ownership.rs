#![cfg(all(target_os = "windows", feature = "lifecycle-test-support"))]

use beryl_backend::{
    ManagedBackendError, ManagedBackendLaunchSpec, ManagedBackendServer, WslFilesystemObservation,
    WslFilesystemOperation, WslSupervisorArtifact,
};
use beryl_model::{
    AdmittedHostPath, PathFlavor, RuntimeId, RuntimeMode, RuntimeNativePath, WslDistributionName,
};
use sha2::{Digest, Sha256};
use std::{
    fs::OpenOptions,
    io::{BufRead, BufReader, Read, Write},
    net::{SocketAddr, TcpStream},
    os::windows::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Arc, atomic::AtomicBool},
    time::{Duration, Instant},
};
use wait_timeout::ChildExt;

const WAIT: Duration = Duration::from_secs(5);
const IDENTITY_MAX_BYTES: u64 = 16 * 1024;

struct NativeContext {
    artifact: Arc<WslSupervisorArtifact>,
    distribution: WslDistributionName,
    workload_host: String,
    workload_linux: String,
}

impl NativeContext {
    fn load() -> Self {
        let artifact_host = PathBuf::from(required("BERYL_WSL_TEST_ARTIFACT_HOST"));
        let mut file = OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(&artifact_host)
            .unwrap();
        let mut digest = Sha256::new();
        let mut scratch = [0; 8192];
        loop {
            let count = file.read(&mut scratch).unwrap();
            if count == 0 {
                break;
            }
            digest.update(&scratch[..count]);
        }
        let artifact = Arc::new(
            WslSupervisorArtifact::from_verified_release(
                artifact_host,
                required("BERYL_WSL_TEST_ARTIFACT_LINUX"),
                digest.finalize().into(),
                1,
                Arc::new(file),
            )
            .unwrap(),
        );
        Self {
            artifact,
            distribution: WslDistributionName::new(required("BERYL_WSL_TEST_DISTRIBUTION"))
                .unwrap(),
            workload_host: required("BERYL_WSL_TEST_WORKLOAD_HOST"),
            workload_linux: required("BERYL_WSL_TEST_WORKLOAD_LINUX"),
        }
    }
    fn launch_spec(&self, token_directory: &Path) -> ManagedBackendLaunchSpec {
        let mode = RuntimeMode::Wsl(self.distribution.clone());
        let native = |value: &str| {
            RuntimeNativePath::from_admitted(mode.clone(), PathFlavor::Posix, value).unwrap()
        };
        ManagedBackendLaunchSpec::new(
            RuntimeId::from_bytes([0x62; 16]),
            host_path(&self.workload_host),
            mode.clone(),
            beryl_model::RuntimeLaunchForm::CodexCli,
            native(&self.workload_linux),
            native("/"),
            host_path(token_directory.to_str().unwrap()),
            native(&drive_projection(token_directory)),
        )
        .unwrap()
        .with_wsl_supervisor_artifact(self.artifact.clone())
    }
}

#[test]
#[ignore = "requires root-supplied exact WSL qualification inputs"]
fn native_managed_owner_joins_detached_nested_descendants_and_preserves_unrelated_work() {
    let context = NativeContext::load();
    let unrelated_tokens = tempfile::tempdir().unwrap();
    let mut unrelated = launch(&context, unrelated_tokens.path());
    let unrelated_port = unrelated.endpoint().port();
    assert_eq!(request_until(unrelated_port, "ping").trim(), "alive");
    let tokens = tempfile::tempdir().unwrap();
    let mut server = match ManagedBackendServer::launch(context.launch_spec(tokens.path())) {
        Ok(server) => server,
        Err(mut failure) => {
            let error = failure.to_string();
            failure
                .shutdown()
                .expect("failed original launch must settle");
            panic!("native launch failed: {error}");
        }
    };
    let parent = server.endpoint().port();
    let detached = request_until(parent, "descendant")
        .trim()
        .parse::<u16>()
        .unwrap();
    let nested = request_until(detached, "descendant")
        .trim()
        .parse::<u16>()
        .unwrap();
    assert_ne!(nested, 0);
    assert_eq!(request_until(parent, "ping").trim(), "alive");
    assert_eq!(request_until(detached, "ping").trim(), "alive");
    assert_eq!(request_until(nested, "ping").trim(), "alive");
    assert!(std::fs::read_dir(tokens.path()).unwrap().next().is_some());
    server
        .shutdown()
        .expect("original Linux and Windows owners must join");
    server
        .shutdown()
        .expect("joined disposal must be idempotent");
    assert_closed(parent);
    assert_closed(detached);
    assert_closed(nested);
    assert!(std::fs::read_dir(tokens.path()).unwrap().next().is_none());
    assert_eq!(request_until(unrelated_port, "ping").trim(), "alive");
    unrelated
        .shutdown()
        .expect("separate original unrelated namespace must join");
}

#[test]
#[ignore = "requires root-supplied exact WSL qualification inputs"]
fn native_fixed_observations_publish_only_after_joined_original_owners() {
    let context = NativeContext::load();
    let directory = observe(&context, WslFilesystemOperation::Directory("/".into()));
    assert_eq!(directory, "/");
    let executable = observe(
        &context,
        WslFilesystemOperation::Executable(context.workload_linux.clone()),
    );
    assert_eq!(executable, context.workload_linux);
    let home = observe(&context, WslFilesystemOperation::UserHome);
    assert!(home.starts_with('/'));
    assert!(!home.is_empty());
}

#[test]
#[ignore = "requires root-supplied exact WSL qualification inputs"]
fn native_missing_workload_retains_original_failure_and_disposes_exact_roles() {
    let context = NativeContext::load();
    let tokens = tempfile::tempdir().unwrap();
    let mut context = context;
    context.workload_linux = "/beryl-owned-no-such-executable".into();
    let mut failure = ManagedBackendServer::launch(context.launch_spec(tokens.path())).unwrap_err();
    assert!(failure.cleanup_pending());
    assert!(matches!(
        failure.error(),
        beryl_backend::ManagedBackendError::WslSupervisionFailure {
            kind: beryl_wsl_supervisor::FailureKind::Exec,
            ..
        }
    ));
    failure
        .shutdown()
        .expect("failed exec original owners must settle");
    assert!(!failure.cleanup_pending());
    failure.shutdown().unwrap();
    assert!(std::fs::read_dir(tokens.path()).unwrap().next().is_none());
}

#[test]
#[ignore = "requires root-supplied exact WSL qualification inputs"]
fn native_workload_identity_and_allowlisted_environment_match_ordinary_account() {
    let context = NativeContext::load();
    let expected = ordinary_identity(&context);
    let tokens = tempfile::tempdir().unwrap();
    let mut server = launch(&context, tokens.path());
    let observed = Identity::decode(&request_bounded_until(
        server.endpoint().port(),
        "identity",
        IDENTITY_MAX_BYTES,
    ));
    server
        .shutdown()
        .expect("identity workload original owners must join");
    assert_eq!(observed, expected);
    assert!(std::fs::read_dir(tokens.path()).unwrap().next().is_none());
}

#[test]
#[ignore = "requires root-supplied exact WSL qualification inputs"]
fn native_expired_disposal_retains_original_launchers_and_readers_for_retry() {
    let context = NativeContext::load();
    let tokens = tempfile::tempdir().unwrap();
    let mut server = launch(&context, tokens.path());
    let ports = owned_ports(&server);
    let identity = server.client_connector().launch_identity().unwrap().clone();
    let original = assert_retained_custody(&server);
    assert!(matches!(
        server.shutdown_wsl_with_timeout_for_lifecycle_test(Duration::ZERO),
        Err(ManagedBackendError::WslSupervisionTimeout)
    ));
    assert_eq!(
        server.wsl_resource_custody_for_lifecycle_test().unwrap(),
        original
    );
    assert!(std::fs::read_dir(tokens.path()).unwrap().next().is_some());
    for port in ports {
        wait_closed(port);
    }
    server
        .shutdown()
        .expect("expired attempt must join the same original owners on retry");
    assert_eq!(
        server.client_connector().launch_identity().unwrap(),
        &identity
    );
    assert_eq!(
        server.wsl_resource_custody_for_lifecycle_test().unwrap(),
        beryl_backend::lifecycle_test_support::WslOwnedResourceCustodyForLifecycleTest::default()
    );
    server.shutdown().unwrap();
    assert!(std::fs::read_dir(tokens.path()).unwrap().next().is_none());
}

#[test]
#[ignore = "requires root-supplied exact WSL qualification inputs"]
fn native_supervisor_control_loss_keeps_original_reader_and_launcher_joins() {
    control_loss(beryl_wsl_supervisor::Role::Supervisor);
}

#[test]
#[ignore = "requires root-supplied exact WSL qualification inputs"]
fn native_broker_control_loss_keeps_original_reader_and_launcher_joins() {
    control_loss(beryl_wsl_supervisor::Role::ContextBroker);
}

fn control_loss(role: beryl_wsl_supervisor::Role) {
    let context = NativeContext::load();
    let tokens = tempfile::tempdir().unwrap();
    let mut server = launch(&context, tokens.path());
    let ports = owned_ports(&server);
    let original = assert_retained_custody(&server);
    server.close_wsl_control_for_lifecycle_test(role).unwrap();
    assert_eq!(
        server.wsl_resource_custody_for_lifecycle_test().unwrap(),
        original
    );
    for port in ports {
        wait_closed(port);
    }
    server
        .shutdown()
        .expect("control loss must join both original Windows launchers and every retained reader");
    assert_eq!(
        server.wsl_resource_custody_for_lifecycle_test().unwrap(),
        beryl_backend::lifecycle_test_support::WslOwnedResourceCustodyForLifecycleTest::default()
    );
    server.shutdown().unwrap();
    assert!(std::fs::read_dir(tokens.path()).unwrap().next().is_none());
}

fn assert_retained_custody(
    server: &ManagedBackendServer,
) -> beryl_backend::lifecycle_test_support::WslOwnedResourceCustodyForLifecycleTest {
    let custody = server.wsl_resource_custody_for_lifecycle_test().unwrap();
    assert_eq!(custody.pending_launcher_joins, 2);
    assert_eq!(custody.retained_control_readers, 2);
    assert_eq!(custody.retained_diagnostic_readers, 2);
    assert_eq!(custody.retained_control_writers, 2);
    custody
}

fn owned_ports(server: &ManagedBackendServer) -> [u16; 3] {
    let parent = server.endpoint().port();
    let detached = request_until(parent, "descendant")
        .trim()
        .parse::<u16>()
        .unwrap();
    let nested = request_until(detached, "descendant")
        .trim()
        .parse::<u16>()
        .unwrap();
    assert_ne!(nested, 0);
    for port in [parent, detached, nested] {
        assert_eq!(request_until(port, "ping").trim(), "alive");
    }
    [parent, detached, nested]
}

#[derive(Debug, Eq, PartialEq)]
struct Identity {
    uid: u32,
    gid: u32,
    groups: Vec<u32>,
    home: Option<Vec<u8>>,
    user: Option<Vec<u8>>,
}
impl Identity {
    fn decode(line: &str) -> Self {
        assert!(line.len() <= IDENTITY_MAX_BYTES as usize);
        let fields: Vec<_> = line
            .strip_suffix('\n')
            .expect("identity line terminator")
            .split('|')
            .collect();
        assert_eq!(fields.len(), 6);
        assert_eq!(fields[0], "BERYL_IDENTITY_V1");
        let groups = if fields[3].is_empty() {
            Vec::new()
        } else {
            fields[3]
                .split(',')
                .map(|group| group.parse::<u32>().unwrap())
                .collect()
        };
        assert!(groups.len() <= 256);
        Self {
            uid: fields[1].parse().unwrap(),
            gid: fields[2].parse().unwrap(),
            groups,
            home: identity_environment(fields[4], 4096),
            user: identity_environment(fields[5], 256),
        }
    }
}
fn identity_environment(field: &str, bound: usize) -> Option<Vec<u8>> {
    if field == "-" {
        return None;
    }
    assert!(field.len() <= bound * 2);
    Some(hex::decode(field).unwrap())
}
fn ordinary_identity(context: &NativeContext) -> Identity {
    use std::os::windows::process::CommandExt;
    let mut child = Command::new("wsl.exe")
        .args([
            "--distribution",
            context.distribution.as_str(),
            "--cd",
            "/",
            "--exec",
            &context.workload_linux,
            "--identity",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .creation_flags(0x08000000)
        .spawn()
        .unwrap();
    let output = child.stdout.take().unwrap();
    let reader = std::thread::Builder::new()
        .name("beryl-wsl-ordinary-identity-reader".into())
        .spawn(move || {
            let mut text = String::new();
            BufReader::new(output)
                .take(IDENTITY_MAX_BYTES + 1)
                .read_to_string(&mut text)?;
            if text.len() > IDENTITY_MAX_BYTES as usize {
                return Err(std::io::ErrorKind::InvalidData.into());
            }
            Ok::<_, std::io::Error>(text)
        })
        .unwrap();
    drop(child.stdin.take());
    assert!(
        child.wait_timeout(WAIT).unwrap().is_some(),
        "ordinary fixed identity fixture did not join after original stdin EOF"
    );
    let deadline = Instant::now() + WAIT;
    while !reader.is_finished() {
        assert!(
            Instant::now() < deadline,
            "ordinary identity original reader did not join"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    let text = reader.join().unwrap().unwrap();
    let (identity, terminal) = text.split_once('\n').expect("ordinary identity result");
    assert_eq!(terminal, "BERYL_IDENTITY_CLOSED\n");
    Identity::decode(&format!("{identity}\n"))
}

fn observe(context: &NativeContext, operation: WslFilesystemOperation) -> String {
    match WslFilesystemObservation::observe(
        context.artifact.clone(),
        &context.distribution,
        operation,
        Duration::from_secs(15),
        &AtomicBool::new(false),
    ) {
        Ok(path) => path,
        Err(failure) => {
            let (error, owner) = failure.into_parts();
            if let Some(mut owner) = owner {
                owner
                    .shutdown()
                    .expect("failed observation must retain and join its original owner");
            }
            panic!("native observation failed: {error}");
        }
    }
}

fn launch(context: &NativeContext, tokens: &Path) -> ManagedBackendServer {
    match ManagedBackendServer::launch(context.launch_spec(tokens)) {
        Ok(server) => server,
        Err(mut failure) => {
            let error = failure.to_string();
            failure
                .shutdown()
                .expect("failed original launch must settle");
            panic!("native launch failed: {error}");
        }
    }
}
fn required(name: &str) -> String {
    std::env::var(name)
        .unwrap_or_else(|_| panic!("missing explicit native qualification input {name}"))
}
fn host_path(value: &str) -> AdmittedHostPath {
    AdmittedHostPath::from_admitted(PathFlavor::Windows, value).unwrap()
}
fn drive_projection(path: &Path) -> String {
    let text = path
        .to_str()
        .unwrap()
        .strip_prefix(r"\\?\")
        .unwrap_or(path.to_str().unwrap());
    assert_eq!(text.as_bytes()[1], b':');
    format!(
        "/mnt/{}/{}",
        text[..1].to_ascii_lowercase(),
        text[3..].replace('\\', "/")
    )
}
fn request_until(port: u16, command: &str) -> String {
    let deadline = Instant::now() + WAIT;
    loop {
        if let Ok(value) = request(port, command) {
            return value;
        }
        assert!(
            Instant::now() < deadline,
            "owned listener {port} unavailable"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}
fn request(port: u16, command: &str) -> std::io::Result<String> {
    request_bounded(port, command, 64)
}
fn request_bounded_until(port: u16, command: &str, bound: u64) -> String {
    let deadline = Instant::now() + WAIT;
    loop {
        if let Ok(value) = request_bounded(port, command, bound) {
            return value;
        }
        assert!(
            Instant::now() < deadline,
            "owned bounded identity response unavailable"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}
fn request_bounded(port: u16, command: &str, bound: u64) -> std::io::Result<String> {
    let address: SocketAddr = ([127, 0, 0, 1], port).into();
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_millis(100))?;
    stream.set_read_timeout(Some(Duration::from_secs(1)))?;
    stream.set_write_timeout(Some(Duration::from_secs(1)))?;
    writeln!(stream, "{command}")?;
    let mut output = String::new();
    BufReader::new(stream)
        .take(bound + 1)
        .read_line(&mut output)?;
    if output.is_empty() || output.len() > bound as usize || !output.ends_with('\n') {
        return Err(std::io::ErrorKind::UnexpectedEof.into());
    }
    Ok(output)
}
fn wait_closed(port: u16) {
    let deadline = Instant::now() + WAIT;
    while request(port, "ping").is_ok() {
        assert!(
            Instant::now() < deadline,
            "owned listener survived original control retirement"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}
fn assert_closed(port: u16) {
    assert!(
        request(port, "ping").is_err(),
        "owned listener {port} survived joined namespace disposal"
    );
}
