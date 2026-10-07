use std::{
    cell::{Cell, RefCell},
    num::NonZeroUsize,
    path::{Path, PathBuf},
    rc::Rc,
    sync::Arc,
};

use crate::{
    app_services::MainWindowServiceInputs,
    running_owner::RunningProcessOwner,
    startup_owner::{self, StartupCommands, StartupCompletion, StartupConfiguration},
};
use beryl_home_store::CommandCancellation;
use gpui::{App, Application};

mod diagnostic;
mod inputs;

pub(crate) fn thread_activation_request(
    thread: beryl_model::SyndicThreadId,
) -> Result<
    (
        crate::composer_host::ComposerHostActivationRequest,
        syndic_storage::DraftPieceOperationIdV1,
    ),
    String,
> {
    inputs::activation(thread)
}

pub use crate::startup_owner::StartupHomeOpen as HomeOpenOutcome;
pub type HomeOpener = Arc<dyn Fn(&Path, CommandCancellation) -> HomeOpenOutcome + Send + Sync>;

pub struct Configuration {
    startup: StartupConfiguration,
    diagnostic_target: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum BootstrapError {
    #[error("Beryl home must be an absolute path")]
    RelativeHome,
    #[error("bootstrap inputs are unavailable: {0}")]
    Inputs(String),
    #[error("diagnostic endpoint could not start: {0}")]
    Diagnostic(#[from] std::io::Error),
    #[error("Beryl startup exited unsuccessfully")]
    Startup,
}

impl Configuration {
    pub fn with_wsl_supervisor_artifact(
        mut self,
        artifact: Option<Arc<beryl_backend::WslSupervisorArtifact>>,
    ) -> Self {
        self.startup.services.wsl_supervisor_artifact = artifact;
        self
    }

    pub fn new(
        home: PathBuf,
        open: HomeOpener,
        diagnostic_target: bool,
    ) -> Result<Self, BootstrapError> {
        if !home.is_absolute() {
            return Err(BootstrapError::RelativeHome);
        }
        let services = inputs::services()?;
        if std::fs::canonicalize(&home).is_ok_and(|canonical_home| {
            Path::new(services.token_directory.host().as_str()).starts_with(canonical_home)
        }) {
            return Err(BootstrapError::Inputs(
                "the Beryl home must not contain the launch token temporary directory".into(),
            ));
        }
        Ok(Self {
            startup: StartupConfiguration {
                home,
                open,
                services,
                windows: inputs::windows(),
                enrollment_slots: NonZeroUsize::new(4).unwrap(),
                settlement_slots: NonZeroUsize::new(4).unwrap(),
                initial_window: beryl_model::WindowId::from_bytes(inputs::identity()?),
                initial_placement: inputs::placement(),
                #[cfg(test)]
                native_hook: None,
                #[cfg(test)]
                service_hook: None,
            },
            diagnostic_target,
        })
    }
}

struct BootstrapLifetime {
    home: PathBuf,
    executable: Option<PathBuf>,
    commands: StartupCommands,
    running: Option<Rc<RefCell<RunningProcessOwner>>>,
    unavailable: Option<startup_owner::UnavailableStartup>,
}
impl gpui::Global for BootstrapLifetime {}

pub fn run(configuration: Configuration) -> Result<(), BootstrapError> {
    let endpoint = configuration
        .diagnostic_target
        .then(crate::diagnostic_child_target::spawn_diagnostic_target_stdio_server)
        .transpose()?;
    let unsuccessful = Rc::new(Cell::new(false));
    let outcome = unsuccessful.clone();
    Application::new()
        .with_quit_on_last_window_close(false)
        .run(move |app| {
            launch(configuration.startup, outcome, endpoint, app);
        });
    if unsuccessful.get() {
        Err(BootstrapError::Startup)
    } else {
        Ok(())
    }
}

fn launch(
    configuration: StartupConfiguration,
    unsuccessful: Rc<Cell<bool>>,
    endpoint: Option<crate::diagnostic_child_target::DiagnosticTargetServer>,
    app: &mut App,
) {
    let home = configuration.home.clone();
    let executable = std::env::current_exe().ok();
    let windows = configuration.windows.clone();
    let commands = startup_owner::start(
        configuration,
        move |completion, app| match completion {
            StartupCompletion::Running(process) => {
                install_creation(&process, windows, app);
                let owner = RunningProcessOwner::start(process, app);
                app.global_mut::<BootstrapLifetime>().running = Some(owner);
            }
            StartupCompletion::Exit {
                unsuccessful: failed,
            } => {
                unsuccessful.set(failed);
                app.quit();
            }
            StartupCompletion::Unavailable { detail, custody } => {
                unsuccessful.set(true);
                tracing::error!(detail, "startup retains unavailable custody");
                app.global_mut::<BootstrapLifetime>().unavailable = Some(custody);
            }
        },
        app,
    );
    app.set_global(BootstrapLifetime {
        home,
        executable,
        commands,
        running: None,
        unavailable: None,
    });
    if let Some(endpoint) = endpoint {
        diagnostic::attach(endpoint, app);
    }
}

fn install_creation(
    process: &startup_owner::StartedProcess,
    inputs: MainWindowServiceInputs,
    app: &mut App,
) {
    let creation = process
        .services
        .window_services(inputs)
        .and_then(|services| {
            crate::main_window::MainWindowCreationOwner::install(
                services.creation_services(),
                process.appearance.clone(),
                crate::main_window::MainWindowCreationGate::Ready,
                app,
            )
        });
    match creation {
        Ok(owner) => {
            for shell in process.windows.shells() {
                shell.attach_creation(owner.clone(), app);
            }
        }
        Err(error) => tracing::error!(error, "window creation is unavailable"),
    }
}

#[cfg(test)]
#[path = "../tests/unit/bootstrap.rs"]
mod tests;
