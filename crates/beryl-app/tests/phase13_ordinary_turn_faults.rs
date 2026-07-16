#![cfg(feature = "test-faults")]

#[path = "phase10_projection/backend.rs"]
mod backend;
#[path = "phase13_ordinary_turn/support.rs"]
mod support;
#[path = "phase10_projection/syndic.rs"]
mod syndic;

#[path = "phase13_ordinary_turn_faults/activation.rs"]
mod activation;
#[path = "phase13_ordinary_turn_faults/common.rs"]
mod common;
#[path = "phase13_ordinary_turn_faults/identity.rs"]
mod identity;
#[path = "phase13_ordinary_turn_faults/live_event.rs"]
mod live_event;
#[path = "phase13_ordinary_turn_faults/terminal.rs"]
mod terminal;
