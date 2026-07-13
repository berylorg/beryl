#![allow(dead_code)]

#[path = "../../src/shell/syndic_transcript/provider.rs"]
pub(crate) mod provider;

pub(crate) use provider::*;

pub(crate) mod syndic_transcript {
    pub(crate) use super::provider::*;
}

#[path = "syndic_transcript_fixture_provider.rs"]
pub(crate) mod fixture_provider;
