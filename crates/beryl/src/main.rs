use std::time::Duration;

use anyhow::Result;
use beryl::cli::BootstrapCli;
use beryl_app::{AppBootstrap, run_app};
use beryl_model::workspace::WorkspaceId;
use tracing::info;
use tracing_subscriber::EnvFilter;

fn main() -> Result<()> {
    init_tracing();
    run(BootstrapCli::parse_from_env())
}

fn run(cli: BootstrapCli) -> Result<()> {
    let initial_workspace = cli.resolve_workspace()?;
    let mut bootstrap = AppBootstrap::new(initial_workspace.clone())
        .with_probe_timeout(Duration::from_millis(cli.probe_timeout_ms()))?
        .with_memory_milestones(cli.memory_milestones());

    if let Some(beryl_home_dir) = cli.beryl_home_dir() {
        bootstrap = bootstrap.with_beryl_home_dir(beryl_home_dir.to_path_buf())?;
    }

    let beryl_home_dir_label = cli
        .beryl_home_dir()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "default".to_string());

    info!(
        startup_target = %initial_workspace
            .as_ref()
            .map(WorkspaceId::display_label)
            .unwrap_or_else(|| "startup-resolution".to_string()),
        probe_timeout_ms = cli.probe_timeout_ms(),
        beryl_home_dir = %beryl_home_dir_label,
        memory_milestones = cli.memory_milestones(),
        "starting beryl workspace shell"
    );

    run_app(bootstrap);
    Ok(())
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();
}
