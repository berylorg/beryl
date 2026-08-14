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

pub mod cli;
