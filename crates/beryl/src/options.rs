use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::Parser;

#[derive(Debug, Parser)]
#[command(name = "beryl", version, about = "Beryl desktop application")]
pub(crate) struct Options {
    #[arg(long, value_name = "PATH")]
    pub(crate) beryl_home_dir: Option<PathBuf>,
    #[arg(long, requires = "beryl_home_dir", hide = true)]
    pub(crate) diagnostic_target_stdio: bool,
}

pub(crate) struct Configuration {
    pub(crate) home: PathBuf,
    pub(crate) diagnostic_target_stdio: bool,
}

impl Options {
    pub(crate) fn resolve(
        self,
        user_home: impl FnOnce() -> Option<PathBuf>,
    ) -> Result<Configuration> {
        let selected = match self.beryl_home_dir {
            Some(path) if !path.as_os_str().is_empty() => path,
            Some(_) => bail!("Beryl home path must not be empty"),
            None if self.diagnostic_target_stdio => {
                bail!("diagnostic target requires --beryl-home-dir")
            }
            None => user_home()
                .context("cannot determine the user home directory; supply --beryl-home-dir")?
                .join(".beryl"),
        };
        let home = std::path::absolute(selected).context("cannot resolve the Beryl home path")?;
        Ok(Configuration {
            home,
            diagnostic_target_stdio: self.diagnostic_target_stdio,
        })
    }
}
