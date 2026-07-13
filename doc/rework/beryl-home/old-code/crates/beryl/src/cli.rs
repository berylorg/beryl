use std::{
    ffi::OsString,
    num::NonZeroU64,
    path::{Path, PathBuf},
};

use anyhow::Result;
use beryl_backend::{canonicalize_host_path, canonicalize_wsl_path};
use beryl_model::workspace::WorkspaceId;
use clap::{Args, Parser};

pub const DEFAULT_PROBE_TIMEOUT_MS: u64 = 10_000;

#[derive(Clone, Debug, Parser)]
#[command(name = "beryl", about = "Start the Beryl workspace shell.")]
pub struct BootstrapCli {
    #[command(flatten)]
    target: RuntimeTargetArgs,

    #[arg(
        long = "beryl-home-dir",
        short = 'H',
        value_name = "PATH",
        help = "Use PATH as the Beryl GUI app-state directory"
    )]
    beryl_home_dir: Option<PathBuf>,

    #[arg(
        long = "probe-timeout-ms",
        value_name = "MS",
        default_value = "10000",
        value_parser = clap::value_parser!(NonZeroU64),
        help = "Managed probe timeout in milliseconds"
    )]
    probe_timeout_ms: NonZeroU64,

    #[arg(
        long = "memory-milestones",
        help = "Emit narrow process-memory milestone diagnostics"
    )]
    memory_milestones: bool,

    #[arg(
        long = "diagnostic-target-stdio",
        requires = "beryl_home_dir",
        help = "Run as a diagnostic child target using newline-delimited JSON over stdio"
    )]
    diagnostic_target_stdio: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RuntimeTarget {
    Picker,
    Host { path: PathBuf },
    Wsl { distro: String, path: PathBuf },
}

#[derive(Clone, Debug, Args)]
struct RuntimeTargetArgs {
    #[arg(
        long = "host-path",
        value_name = "PATH",
        conflicts_with_all = ["wsl_distro", "wsl_path"],
        help = "Skip the picker and open this host-Windows workspace directly"
    )]
    host_path: Option<PathBuf>,

    #[arg(
        long = "wsl-distro",
        value_name = "DISTRO",
        requires = "wsl_path",
        help = "Skip the picker and target this WSL-Linux distro"
    )]
    wsl_distro: Option<String>,

    #[arg(
        long = "wsl-path",
        value_name = "PATH",
        requires = "wsl_distro",
        help = "Workspace path inside the selected WSL distro"
    )]
    wsl_path: Option<PathBuf>,
}

impl BootstrapCli {
    pub fn parse_from_env() -> Self {
        <Self as Parser>::parse()
    }

    pub fn try_parse_from<I, T>(args: I) -> Result<Self, clap::Error>
    where
        I: IntoIterator<Item = T>,
        T: Into<OsString> + Clone,
    {
        <Self as Parser>::try_parse_from(args)
    }

    pub fn target(&self) -> RuntimeTarget {
        match (
            self.target.host_path.clone(),
            self.target.wsl_distro.clone(),
            self.target.wsl_path.clone(),
        ) {
            (Some(path), None, None) => RuntimeTarget::Host { path },
            (None, Some(distro), Some(path)) => RuntimeTarget::Wsl { distro, path },
            _ => RuntimeTarget::Picker,
        }
    }

    pub fn beryl_home_dir(&self) -> Option<&Path> {
        self.beryl_home_dir.as_deref()
    }

    pub fn probe_timeout_ms(&self) -> u64 {
        self.probe_timeout_ms.get()
    }

    pub fn memory_milestones(&self) -> bool {
        self.memory_milestones
    }

    pub fn diagnostic_target_stdio(&self) -> bool {
        self.diagnostic_target_stdio
    }

    pub fn resolve_workspace(&self) -> Result<Option<WorkspaceId>> {
        match self.target() {
            RuntimeTarget::Picker => Ok(None),
            RuntimeTarget::Host { path } => canonicalize_host_path(&path)
                .map(WorkspaceId::host_windows)
                .map(Some)
                .map_err(Into::into),
            RuntimeTarget::Wsl { distro, path } => {
                let canonical_path = canonicalize_wsl_path(&distro, &path)?;
                Ok(Some(WorkspaceId::wsl_linux(distro, canonical_path)))
            }
        }
    }
}
