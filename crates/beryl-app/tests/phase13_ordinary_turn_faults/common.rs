use beryl_app::cas_projection::OrdinaryTurnExecutionError;
use beryl_home_store::{HomeHealthState, HomeStore};

use crate::backend::{FakeAppServer, ProjectionStep, TurnStartReply};

pub const INPUT: &str = "phase13 ordinary persistence cut";
pub const CAS_THREAD: &str = "phase13-fault-thread";
pub const CAS_TURN: &str = "phase13-fault-turn";
pub const CAS_ITEM: &str = "phase13-fault-item";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExpectedCutState {
    Prior,
    New,
    PriorOrNew,
}

impl ExpectedCutState {
    pub fn assert_allows(self, actual: Self) {
        assert!(
            actual != Self::PriorOrNew && (self == Self::PriorOrNew || self == actual),
            "expected {self:?} persistence state, recovered {actual:?}"
        );
    }
}

pub fn turn_server() -> FakeAppServer {
    FakeAppServer::spawn(vec![
        ProjectionStep::Fresh { target: CAS_THREAD },
        ProjectionStep::TurnStart {
            target: CAS_THREAD,
            expected_input: INPUT,
            before_reply: vec![],
            reply: TurnStartReply::Exact { turn: CAS_TURN },
            after_reply: vec![],
        },
    ])
}

pub fn assert_publication_failure(error: OrdinaryTurnExecutionError) {
    assert!(
        matches!(error, OrdinaryTurnExecutionError::Publication(_)),
        "expected a typed publication failure, got {error:?}"
    );
}

pub fn recover_after_writer_cut(store: &HomeStore) {
    assert_eq!(store.health().state(), HomeHealthState::Verifying);
    store.verify_health().unwrap();
    assert_eq!(store.health().state(), HomeHealthState::Healthy);
}
