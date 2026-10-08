#![cfg(feature = "test-faults")]

#[path = "frozen_catalog_queries/evaluation.rs"]
mod evaluation;
#[path = "frozen_catalog_queries/integrity.rs"]
mod integrity;
#[path = "frozen_catalog_queries/lifecycle.rs"]
mod lifecycle;
#[path = "frozen_catalog_queries/support.rs"]
mod query_support;
mod support;

use beryl_home_store::{
    CommandCancellation, CommandOutcome, FrozenReadAccessError, HomeStore, ReadError,
};
use beryl_model::{ProjectionRevision, RootId, RuntimeId, SyndicThreadId};
use beryl_state::*;
use query_support::*;
