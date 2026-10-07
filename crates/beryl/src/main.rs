#[cfg(target_os = "windows")]
mod home_open;
mod options;
mod wsl_supervisor_artifact;

#[cfg(target_os = "windows")]
use std::sync::Arc;

use anyhow::{Context, Result};
#[cfg(target_os = "windows")]
use beryl_app::bootstrap;
use beryl_app::crash_reporting;
use clap::Parser;

fn main() -> std::process::ExitCode {
    let arguments = std::env::args_os().collect::<Vec<_>>();
    if arguments
        .get(1)
        .is_some_and(|argument| argument == crash_reporting::REPORTER_ARGUMENT)
    {
        crash_reporting::run_reporter(&arguments[2..], crash_reporting::present);
    }
    match run(arguments) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Beryl could not start: {error:#}");
            std::process::ExitCode::FAILURE
        }
    }
}

fn run(arguments: Vec<std::ffi::OsString>) -> Result<()> {
    let executable = std::env::current_exe().context("cannot locate the Beryl executable")?;
    let reporter = crash_reporting::install(&executable);
    let options = match options::Options::try_parse_from(arguments) {
        Ok(options) => options,
        Err(error) => error.exit(),
    };
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .try_init()
        .map_err(|error| anyhow::anyhow!("cannot initialize logging: {error}"))?;
    if let Err(error) = reporter {
        tracing::warn!(%error, "Crash reporter unavailable; fatal abort handling remains installed");
    }
    let options = options.resolve(std::env::home_dir)?;
    start(options)
}

#[cfg(target_os = "windows")]
fn start(options: options::Configuration) -> Result<()> {
    let configuration = bootstrap::Configuration::new(
        options.home,
        Arc::new(home_open::open),
        options.diagnostic_target_stdio,
    )?
    .with_wsl_supervisor_artifact(wsl_supervisor_artifact::bundled());
    bootstrap::run(configuration)?;
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn start(_options: options::Configuration) -> Result<()> {
    anyhow::bail!("Beryl home startup is unavailable on this platform")
}
