#![allow(dead_code)]

#[path = "../../src/shell/syndic_transcript/provider.rs"]
pub(crate) mod provider;

pub(crate) use provider::*;

#[path = "../../src/shell/syndic_transcript/fixture_provider.rs"]
pub(crate) mod fixture_provider;
