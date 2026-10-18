use super::*;
use beryl_backend::ManagedBackendLaunchSpec;
use std::path::Path;

pub(super) struct Prepared {
    pub shell: MainWindowShellPrepared,
    pub acquisition: (
        RuntimeBackedWindowAcquisitionService,
        RuntimeBackedWindowProcessRegistry,
    ),
    pub composer: Arc<MainWindowConversationComposerService>,
    pub runtime: Runtime,
}

pub(super) struct Runtime {
    spec: ManagedBackendLaunchSpec,
    execution: ExecutionBinding,
    interest: Option<RuntimeInterest>,
    server: Option<server::NormalTerminalServer>,
    controlled: bool,
}

pub(super) fn prepare(
    fixture: &mut syndic::Fixture,
    theme: beryl_state::PreparedThemeAppearance,
    controlled: bool,
) -> Prepared {
    let root = fixture.home_path().join("managed-context-root");
    let tokens = fixture.home_path().join("managed-context-tokens");
    std::fs::create_dir(&root).unwrap();
    std::fs::create_dir(&tokens).unwrap();
    let root = canonical(&root);
    let executable = canonical(Path::new(env!("CARGO_BIN_EXE_managed-runtime-fixture")));
    let tokens = canonical(&tokens);
    let execution = ExecutionBinding::new(
        RuntimeId::from_bytes([246; 16]),
        RootId::from_bytes([247; 16]),
        native(&root),
    );
    let server = if controlled {
        server::NormalTerminalServer::spawn_managed_controlled_connection_loss(root.clone().into())
    } else {
        server::NormalTerminalServer::spawn_managed_admission_only(root.clone().into())
    };
    let endpoint = server.endpoint();
    std::fs::write(Path::new(&root).join("fixture-mode"), "context-proxy").unwrap();
    std::fs::write(
        Path::new(&root).join("context-proxy-endpoint"),
        format!("127.0.0.1:{}", endpoint.port()),
    )
    .unwrap();
    let (shell, acquisition, composer) =
        support::prepare_shell_with_runtime(fixture, theme, execution.clone(), &executable);
    let spec = ManagedBackendLaunchSpec::new(
        execution.runtime_id(),
        host(&executable),
        RuntimeMode::Host,
        RuntimeLaunchForm::CodexCli,
        native(&executable),
        execution.root_path().clone(),
        host(&tokens),
        native(&tokens),
    )
    .unwrap();
    Prepared {
        shell,
        acquisition,
        composer,
        runtime: Runtime {
            spec,
            execution,
            interest: None,
            server: Some(server),
            controlled,
        },
    }
}

impl Runtime {
    pub(super) fn activate(&mut self, fixture: &syndic::Fixture) {
        let observer = fixture.store.gui_runtime_submission_observer_for_test();
        assert!(!observer(&self.execution));
        let interest = fixture
            .store
            .acquire_runtime_interest(
                self.spec.clone(),
                self.execution.clone(),
                RuntimeInterestKind::View,
            )
            .unwrap();
        let status = interest.wait_for_change(RuntimeInterestStatus::Starting, server::TIMEOUT);
        assert!(
            matches!(status, RuntimeInterestStatus::Ready(_)),
            "original managed context runtime did not become ready: {status:?}"
        );
        assert!(observer(&self.execution));
        if !self.controlled {
            self.server.as_ref().unwrap().wait_for_admission();
        }
        self.interest = Some(interest);
    }

    pub(super) fn admit_session(&self, fixture: &syndic::Fixture) -> AdmittedProjectionSession {
        let interest = fixture
            .store
            .acquire_runtime_interest(
                self.spec.clone(),
                self.execution.clone(),
                RuntimeInterestKind::RequiredWork,
            )
            .unwrap();
        let RuntimeInterestStatus::Ready(ready) = interest.status() else {
            panic!("original managed runtime was not ready for its required-work session");
        };
        let session = fixture
            .store
            .admit_runtime_session(interest, server::TIMEOUT)
            .unwrap();
        assert_eq!(session.runtime_id(), self.execution.runtime_id());
        assert_eq!(session.process_generation(), ready.process_generation());
        session
    }

    pub(super) fn execution(&self) -> ExecutionBinding {
        self.execution.clone()
    }

    pub(super) fn take_server(&mut self) -> server::NormalTerminalServer {
        assert!(self.controlled);
        self.server.take().unwrap()
    }

    pub(super) fn release(&mut self) {
        drop(self.interest.take());
    }

    pub(super) fn close(mut self) {
        self.release();
        if let Some(server) = self.server.take() {
            server.join();
        }
    }
}

fn canonical(path: &Path) -> String {
    std::fs::canonicalize(path)
        .unwrap()
        .to_str()
        .unwrap()
        .trim_start_matches(r"\\?\")
        .to_owned()
}

fn host(path: &str) -> AdmittedHostPath {
    AdmittedHostPath::from_admitted(PathFlavor::Windows, path).unwrap()
}

fn native(path: &str) -> RuntimeNativePath {
    RuntimeNativePath::from_admitted(RuntimeMode::Host, PathFlavor::Windows, path).unwrap()
}
