//! CLI support for the Beryl executable.
//!
//! ```no_run
//! use beryl::cli::{BootstrapCli, RuntimeTarget};
//!
//! let cli = BootstrapCli::try_parse_from([
//!     "beryl",
//!     "-H",
//!     "state-root",
//!     "--probe-timeout-ms",
//!     "500",
//! ])
//! .unwrap();
//!
//! assert!(matches!(cli.target(), RuntimeTarget::Picker));
//! assert_eq!(cli.probe_timeout_ms(), 500);
//! assert_eq!(cli.beryl_home_dir().unwrap(), std::path::Path::new("state-root"));
//! assert!(!cli.diagnostic_target_stdio());
//! ```
//!
//! The dedicated acceptance executable uses the same typed parsing boundary:
//!
//! ```no_run
//! use beryl::acceptance_cli::AcceptanceCli;
//!
//! let cli = AcceptanceCli::try_parse_from([
//!     "beryl-acceptance",
//!     "--executable", r"C:\fixture\beryl.exe",
//!     "--isolated-home", r"C:\fixture\home",
//!     "--execution-workspace", r"C:\fixture\workspace",
//!     "--evidence", r"C:\fixture\evidence\run.json",
//!     "--run-identity", "run-001",
//!     "--request-plan", r"C:\fixture\requests.json",
//! ])?;
//! cli.run()?;
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

pub mod acceptance_cli;
pub mod cli;
pub mod diagnostic_startup_gate;
