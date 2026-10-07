use std::{
    process::{Command, Stdio},
    sync::{Arc, atomic::AtomicBool},
    time::{Duration, Instant},
};

use beryl_wsl_supervisor::{Frame, Nonce, Role};

use crate::{ManagedBackendError, WslSupervisorArtifact};

mod companion;
pub(crate) mod progress;
use companion::Companion;
use progress::ControlProgress;

pub(crate) const ATTEMPT_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug)]
pub(crate) struct WslSupervision {
    artifact: Arc<WslSupervisorArtifact>,
    distribution: String,
    execution_root: String,
    nonce: Nonce,
    broker: Option<Companion>,
    supervisor: Option<Companion>,
    broker_ready: bool,
    broker_invalid: bool,
    broker_closed_record: bool,
    progress: ControlProgress,
    complete: bool,
}

impl WslSupervision {
    pub(crate) fn new(
        artifact: Arc<WslSupervisorArtifact>,
        distribution: String,
        execution_root: String,
    ) -> Result<Self, ManagedBackendError> {
        let mut nonce = [0; 32];
        getrandom::fill(&mut nonce)
            .map_err(|source| ManagedBackendError::GenerateWslNonce { source })?;
        Ok(Self {
            artifact,
            distribution,
            execution_root,
            nonce,
            broker: None,
            supervisor: None,
            broker_ready: false,
            broker_invalid: false,
            broker_closed_record: false,
            progress: ControlProgress::default(),
            complete: false,
        })
    }

    pub(crate) fn launch(
        &mut self,
        request: Frame,
        deadline: Instant,
        cancellation: Option<&AtomicBool>,
    ) -> Result<(), ManagedBackendError> {
        let broker = self.spawn_role(Role::ContextBroker)?;
        self.broker = Some(broker);
        let broker = self.broker.as_mut().expect("original broker installed");
        broker.prepare(self.nonce)?;
        broker.send(&self.nonce, &Frame::Initialize)?;
        match broker.receive(deadline, cancellation)? {
            Frame::Ready {
                role: Role::ContextBroker,
            } => self.broker_ready = true,
            Frame::Failure { kind, errno } => {
                return Err(ManagedBackendError::WslSupervisionFailure { kind, errno });
            }
            _ => {
                self.broker_invalid = true;
                return Err(ManagedBackendError::WslSupervisionUnavailable);
            }
        }
        let supervisor = self.spawn_role(Role::Supervisor)?;
        self.supervisor = Some(supervisor);
        let supervisor = self
            .supervisor
            .as_mut()
            .expect("original supervisor installed");
        supervisor.prepare(self.nonce)?;
        supervisor.send(&self.nonce, &request)?;
        self.receive_until(deadline, cancellation, |owner| {
            owner.progress.workload_started
        })
    }

    fn spawn_role(&self, role: Role) -> Result<Companion, ManagedBackendError> {
        let mut command = Command::new("wsl.exe");
        command.args(["--distribution", &self.distribution]);
        if role == Role::Supervisor {
            command.args(["--user", "root"]);
        }
        command.args([
            "--cd",
            &self.execution_root,
            "--exec",
            self.artifact.linux_path(),
            if role == Role::Supervisor {
                "supervise"
            } else {
                "context-broker"
            },
        ]);
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let child = command
            .spawn()
            .map_err(|source| ManagedBackendError::Spawn {
                program: "wsl.exe".into(),
                source,
            })?;
        Ok(Companion::new(child))
    }

    fn receive_until(
        &mut self,
        deadline: Instant,
        cancellation: Option<&AtomicBool>,
        done: impl Fn(&Self) -> bool,
    ) -> Result<(), ManagedBackendError> {
        while !done(self) {
            let frame = self
                .supervisor
                .as_mut()
                .ok_or(ManagedBackendError::WslSupervisionUnavailable)?
                .receive(deadline, cancellation)?;
            self.progress.accept(frame)?;
        }
        Ok(())
    }

    pub(crate) fn wait_observation(
        &mut self,
        deadline: Instant,
        cancellation: &AtomicBool,
    ) -> Result<String, ManagedBackendError> {
        self.receive_until(deadline, Some(cancellation), |owner| {
            owner.progress.namespace_closed.is_some()
        })?;
        let result = self
            .progress
            .namespace_closed
            .as_ref()
            .ok_or(ManagedBackendError::WslObservationInvalid)?;
        match (&result.exit, &result.observation) {
            (beryl_wsl_supervisor::ExitStatus::Exited(0), Some(path))
                if path.starts_with('/') && path.len() <= 4096 && !path.contains('\0') =>
            {
                Ok(path.clone())
            }
            _ => Err(ManagedBackendError::WslObservationInvalid),
        }
    }

    pub(crate) fn shutdown(&mut self, deadline: Instant) -> Result<(), ManagedBackendError> {
        if self.complete {
            return Ok(());
        }
        if self.supervisor.is_some() && !self.progress.pre_context_failure {
            if !self.progress.companions_closed {
                let supervisor = self.supervisor.as_mut().expect("supervisor retained");
                let request = supervisor.send(&self.nonce, &Frame::Stop);
                self.progress
                    .retire(request, || supervisor.receive(deadline, None))?;
            }
        }
        if self.supervisor.is_none() || self.progress.pre_context_failure {
            if let Some(broker) = self.broker.as_mut() {
                if !self.progress.companions_closed {
                    let _ = broker.send(&self.nonce, &Frame::BrokerClose);
                    if self.broker_invalid {
                        return Err(ManagedBackendError::WslSupervisionUnavailable);
                    }
                    loop {
                        match broker.receive(deadline, None)? {
                            Frame::Ready {
                                role: Role::ContextBroker,
                            } if !self.broker_ready => self.broker_ready = true,
                            Frame::LinuxCompanionsClosed => {
                                self.broker_closed_record = true;
                                self.progress.companions_closed = true;
                                break;
                            }
                            Frame::Failure { .. } => {}
                            _ => {
                                self.broker_invalid = true;
                                return Err(ManagedBackendError::WslSupervisionUnavailable);
                            }
                        }
                    }
                }
            } else {
                self.progress.companions_closed = true;
            }
        }
        if !self.progress.companions_closed {
            return Err(ManagedBackendError::WslSupervisionUnavailable);
        }
        if let Some(supervisor) = self.supervisor.as_mut() {
            supervisor.join(deadline, false)?;
        }
        if let Some(broker) = self.broker.as_mut() {
            broker.join(deadline, !self.broker_closed_record)?;
        }
        self.complete = true;
        Ok(())
    }

    pub(crate) fn process_id(&self) -> Option<u32> {
        self.supervisor
            .as_ref()
            .or(self.broker.as_ref())
            .map(Companion::process_id)
    }

    #[cfg(feature = "lifecycle-test-support")]
    pub(crate) fn close_control_for_lifecycle_test(
        &mut self,
        role: Role,
    ) -> Result<(), ManagedBackendError> {
        let companion = match role {
            Role::ContextBroker => self.broker.as_mut(),
            Role::Supervisor => self.supervisor.as_mut(),
        }
        .ok_or(ManagedBackendError::WslSupervisionUnavailable)?;
        companion.close_control_for_lifecycle_test();
        Ok(())
    }

    #[cfg(feature = "lifecycle-test-support")]
    pub(crate) fn resource_custody_for_lifecycle_test(
        &self,
    ) -> crate::lifecycle_test_support::WslOwnedResourceCustodyForLifecycleTest {
        let mut custody =
            crate::lifecycle_test_support::WslOwnedResourceCustodyForLifecycleTest::default();
        for companion in [self.broker.as_ref(), self.supervisor.as_ref()]
            .into_iter()
            .flatten()
        {
            let original = companion.resource_custody_for_lifecycle_test();
            custody.pending_launcher_joins += original.pending_launcher_joins;
            custody.retained_control_readers += original.retained_control_readers;
            custody.retained_diagnostic_readers += original.retained_diagnostic_readers;
            custody.retained_control_writers += original.retained_control_writers;
        }
        custody
    }
    pub(crate) fn has_exited(&mut self) -> bool {
        self.complete
            || self.progress.namespace_closed.is_some()
            || self.supervisor.as_mut().is_some_and(Companion::has_exited)
    }
}

impl Drop for WslSupervision {
    fn drop(&mut self) {
        if let Err(error) = self.shutdown(Instant::now() + ATTEMPT_TIMEOUT) {
            tracing::warn!(%error, "WSL original owner disposal remains unproved");
        }
    }
}
