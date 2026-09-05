use std::{
    env,
    path::{Path, PathBuf},
};

use beryl_model::workspace::RuntimeMode;
use thiserror::Error;

const HOST_WINDOWS_STANDALONE_APP_SERVER: &str = "codex-app-server.exe";
const HOST_WINDOWS_CODEX_CLI: &str = "codex.exe";

/// A deterministic view of the Host-Windows executable search path.
///
/// The normal managed launch captures the current process `PATH`. Tests and
/// embedders that construct launch specifications can provide explicit path
/// entries without mutating process-global environment state.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BackendPathResolver {
    host_windows_path_entries: Vec<PathBuf>,
}

impl BackendPathResolver {
    pub fn from_current_process_path() -> Self {
        let path = env::var_os("PATH");
        let entries = path.as_deref().map(env::split_paths).into_iter().flatten();
        let discovery_cwd = env::current_dir().ok();

        Self::from_host_windows_path_entries_relative_to(entries, discovery_cwd.as_deref())
    }

    pub fn from_host_windows_path_entries(entries: impl IntoIterator<Item = PathBuf>) -> Self {
        let discovery_cwd = env::current_dir().ok();
        Self::from_host_windows_path_entries_relative_to(entries, discovery_cwd.as_deref())
    }

    /// Captures PATH entries relative to the supplied executable-discovery directory.
    ///
    /// Relative entries are made absolute while the launch specification is
    /// built, so a later backend working-directory change cannot redirect a
    /// selected standalone executable.
    pub fn from_host_windows_path_entries_relative_to(
        entries: impl IntoIterator<Item = PathBuf>,
        discovery_cwd: Option<&Path>,
    ) -> Self {
        Self {
            host_windows_path_entries: entries
                .into_iter()
                .filter_map(|entry| {
                    entry
                        .is_absolute()
                        .then_some(entry.clone())
                        .or_else(|| discovery_cwd.map(|cwd| cwd.join(entry)))
                })
                .collect(),
        }
    }

    fn standalone_app_server(&self) -> Option<PathBuf> {
        self.host_windows_path_entries
            .iter()
            .map(|entry| entry.join(HOST_WINDOWS_STANDALONE_APP_SERVER))
            .find(|candidate| candidate.is_file())
    }
}

/// Validated launch customization for a Beryl-managed backend process.
///
/// By default, Host Windows launches select `codex-app-server.exe` from
/// `PATH` when it is present, then use `codex.exe app-server`. Callers that
/// need a known executable may opt into an absolute exact Host Windows Codex
/// CLI path with [`Self::with_exact_host_windows_program`], or select an exact
/// standalone app-server executable with
/// [`Self::with_exact_host_windows_standalone_app_server`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ManagedBackendLaunchOptions {
    host_windows_program: HostWindowsProgram,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
enum HostWindowsProgram {
    #[default]
    DefaultPath,
    ExactCodexCli(PathBuf),
    ExactStandaloneAppServer(PathBuf),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum HostWindowsAppServerCommand {
    CodexCli,
    Standalone,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SelectedHostWindowsProgram {
    pub(super) program: String,
    pub(super) app_server_command: HostWindowsAppServerCommand,
}

/// An invalid managed backend launch customization.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum ManagedBackendLaunchOptionsError {
    #[error("the exact Host Windows backend program must not be empty")]
    EmptyExactHostWindowsProgram,
    #[error("the exact Host Windows backend program must be absolute: {path:?}")]
    RelativeExactHostWindowsProgram { path: PathBuf },
    #[error("the exact Host Windows backend program must be Unicode: {path:?}")]
    NonUnicodeExactHostWindowsProgram { path: PathBuf },
    #[error("the discovered Host Windows backend program must be Unicode: {path:?}")]
    NonUnicodeDiscoveredHostWindowsProgram { path: PathBuf },
    #[error("an exact Host Windows backend program is unsupported for runtime {runtime_mode:?}")]
    ExactHostWindowsProgramUnsupportedRuntime { runtime_mode: RuntimeMode },
}

impl ManagedBackendLaunchOptions {
    /// Selects an absolute Host Windows Codex CLI executable.
    pub fn with_exact_host_windows_program(
        program: impl Into<PathBuf>,
    ) -> Result<Self, ManagedBackendLaunchOptionsError> {
        let program = program.into();
        validate_exact_host_windows_program(&program)?;

        Ok(Self {
            host_windows_program: HostWindowsProgram::ExactCodexCli(program),
        })
    }

    /// Selects an absolute standalone Host Windows app-server executable.
    ///
    /// Unlike [`Self::with_exact_host_windows_program`], the selected program
    /// is invoked directly and does not receive the Codex CLI `app-server`
    /// subcommand.
    pub fn with_exact_host_windows_standalone_app_server(
        program: impl Into<PathBuf>,
    ) -> Result<Self, ManagedBackendLaunchOptionsError> {
        let program = program.into();
        validate_exact_host_windows_program(&program)?;

        Ok(Self {
            host_windows_program: HostWindowsProgram::ExactStandaloneAppServer(program),
        })
    }

    /// Returns the configured exact Host Windows Codex CLI executable, if any.
    pub fn exact_host_windows_program(&self) -> Option<&Path> {
        match &self.host_windows_program {
            HostWindowsProgram::ExactCodexCli(program) => Some(program),
            HostWindowsProgram::DefaultPath | HostWindowsProgram::ExactStandaloneAppServer(_) => {
                None
            }
        }
    }

    /// Returns the configured exact standalone Host Windows app-server executable, if any.
    pub fn exact_host_windows_standalone_app_server(&self) -> Option<&Path> {
        match &self.host_windows_program {
            HostWindowsProgram::ExactStandaloneAppServer(program) => Some(program),
            HostWindowsProgram::DefaultPath | HostWindowsProgram::ExactCodexCli(_) => None,
        }
    }

    /// Verifies that this customization is valid for `runtime_mode`.
    pub fn validate_for_runtime(
        &self,
        runtime_mode: &RuntimeMode,
    ) -> Result<(), ManagedBackendLaunchOptionsError> {
        let Some(program) = self
            .exact_host_windows_program()
            .or_else(|| self.exact_host_windows_standalone_app_server())
        else {
            return Ok(());
        };
        validate_exact_host_windows_program(program)?;

        if !matches!(runtime_mode, RuntimeMode::HostWindows) {
            return Err(
                ManagedBackendLaunchOptionsError::ExactHostWindowsProgramUnsupportedRuntime {
                    runtime_mode: runtime_mode.clone(),
                },
            );
        }

        Ok(())
    }

    pub(super) fn selected_host_windows_program(
        &self,
        resolver: &BackendPathResolver,
    ) -> Result<SelectedHostWindowsProgram, ManagedBackendLaunchOptionsError> {
        match &self.host_windows_program {
            HostWindowsProgram::DefaultPath => match resolver.standalone_app_server() {
                Some(program) => Ok(SelectedHostWindowsProgram {
                    program: discovered_host_windows_program_as_string(&program)?,
                    app_server_command: HostWindowsAppServerCommand::Standalone,
                }),
                None => Ok(SelectedHostWindowsProgram {
                    program: HOST_WINDOWS_CODEX_CLI.to_string(),
                    app_server_command: HostWindowsAppServerCommand::CodexCli,
                }),
            },
            HostWindowsProgram::ExactCodexCli(program) => Ok(SelectedHostWindowsProgram {
                program: exact_host_windows_program_as_string(program)?,
                app_server_command: HostWindowsAppServerCommand::CodexCli,
            }),
            HostWindowsProgram::ExactStandaloneAppServer(program) => {
                Ok(SelectedHostWindowsProgram {
                    program: exact_host_windows_program_as_string(program)?,
                    app_server_command: HostWindowsAppServerCommand::Standalone,
                })
            }
        }
    }
}

fn exact_host_windows_program_as_string(
    program: &Path,
) -> Result<String, ManagedBackendLaunchOptionsError> {
    program.to_str().map(str::to_owned).ok_or_else(|| {
        ManagedBackendLaunchOptionsError::NonUnicodeExactHostWindowsProgram {
            path: program.to_path_buf(),
        }
    })
}

fn discovered_host_windows_program_as_string(
    program: &Path,
) -> Result<String, ManagedBackendLaunchOptionsError> {
    program.to_str().map(str::to_owned).ok_or_else(|| {
        ManagedBackendLaunchOptionsError::NonUnicodeDiscoveredHostWindowsProgram {
            path: program.to_path_buf(),
        }
    })
}

fn validate_exact_host_windows_program(
    program: &Path,
) -> Result<(), ManagedBackendLaunchOptionsError> {
    if program.as_os_str().is_empty() {
        return Err(ManagedBackendLaunchOptionsError::EmptyExactHostWindowsProgram);
    }
    if !program.is_absolute() {
        return Err(
            ManagedBackendLaunchOptionsError::RelativeExactHostWindowsProgram {
                path: program.to_path_buf(),
            },
        );
    }
    if program.to_str().is_none() {
        return Err(
            ManagedBackendLaunchOptionsError::NonUnicodeExactHostWindowsProgram {
                path: program.to_path_buf(),
            },
        );
    }

    Ok(())
}
