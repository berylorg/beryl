#![allow(dead_code, unused_imports)]

#[path = "../../src/shell/syndic_transcript/provider.rs"]
pub(crate) mod provider;

pub(crate) use provider::*;

#[path = "../../src/shell/syndic_transcript/snapshot.rs"]
pub(crate) mod snapshot;

pub(crate) use snapshot::*;

#[path = "../../src/shell/syndic_transcript/status_facts.rs"]
pub(crate) mod status_facts;

pub(crate) use status_facts::*;

#[path = "../../src/shell/syndic_transcript/demand.rs"]
pub(crate) mod demand;

pub(crate) use demand::*;

#[path = "../../src/shell/syndic_transcript/activation.rs"]
pub(crate) mod activation;

pub(crate) use activation::*;

#[path = "../../src/shell/syndic_transcript/frame/mod.rs"]
pub(crate) mod frame;

pub(crate) use frame::*;

#[path = "../../src/shell/syndic_transcript/selection.rs"]
pub(crate) mod selection;

pub(crate) use selection::*;

#[path = "../../src/shell/syndic_transcript/context_menu.rs"]
pub(crate) mod context_menu;

pub(crate) use context_menu::*;

#[path = "../../src/shell/syndic_transcript/media_action.rs"]
pub(crate) mod media_action;

pub(crate) use media_action::*;

#[path = "../../src/shell/syndic_transcript/renderer_context_menu.rs"]
pub(crate) mod renderer_context_menu;

pub(crate) use renderer_context_menu::*;

#[path = "../../src/shell/syndic_transcript/renderer_media_action.rs"]
pub(crate) mod renderer_media_action;

pub(crate) use renderer_media_action::*;

#[path = "../../src/shell/syndic_transcript/renderer_selection.rs"]
pub(crate) mod renderer_selection;

pub(crate) use renderer_selection::*;

#[path = "../../src/shell/syndic_transcript/renderer_quote.rs"]
pub(crate) mod renderer_quote;

pub(crate) use renderer_quote::*;

#[path = "../../src/shell/syndic_transcript/command.rs"]
pub(crate) mod command;

pub(crate) use command::*;

#[path = "../../src/shell/syndic_transcript/core.rs"]
pub(crate) mod core;

pub(crate) use core::*;

#[path = "syndic_transcript_fixture_provider.rs"]
pub(crate) mod fixture_provider;
