//! Bounded CLI runner for Beryl diagnostic acceptance sessions.
//!
//! The executable consumes an explicit frozen Beryl path, isolated home, validated launch
//! configuration, evidence destination, run identity, limits, and a bounded JSON request plan.

use anyhow::Result;
use beryl::acceptance_cli::AcceptanceCli;

fn main() -> Result<()> {
    AcceptanceCli::parse_from_env().run()
}
