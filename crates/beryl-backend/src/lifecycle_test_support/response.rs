/// Initial and terminal state of the non-cloneable response expectation test seam.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IncomingJsonExpectation {
    Idle,
    Initialize { id: u64 },
    ConfigRead { id: u64 },
    ModelList { id: u64 },
    ThreadStart { id: u64 },
    ThreadRead { id: u64 },
    ThreadResume { id: u64 },
    ThreadFork { id: u64 },
    ThreadCompactStart { id: u64 },
    ThreadInjectItems { id: u64 },
    ThreadUnsubscribe { id: u64 },
    TurnInterrupt { id: u64 },
    TurnStart { id: u64 },
    TurnSteer { id: u64 },
    Poisoned,
}

pub(super) fn test_expectation_slot(
    expectation: IncomingJsonExpectation,
) -> crate::incoming_json::ResponseExpectationSlot {
    let mut slot = crate::incoming_json::ResponseExpectationSlot::default();
    match expectation {
        IncomingJsonExpectation::Idle => {}
        IncomingJsonExpectation::Initialize { id } => slot
            .install_fixed(id, crate::incoming_json::ResponseFamily::Initialize)
            .expect("fresh response expectation slot accepts initialize"),
        IncomingJsonExpectation::ConfigRead { id } => slot
            .install_fixed(id, crate::incoming_json::ResponseFamily::ConfigRead)
            .expect("fresh response expectation slot accepts config/read"),
        IncomingJsonExpectation::ModelList { id } => slot
            .install_fixed(id, crate::incoming_json::ResponseFamily::ModelList)
            .expect("fresh response expectation slot accepts model/list"),
        IncomingJsonExpectation::ThreadStart { id } => slot
            .install_fixed(id, crate::incoming_json::ResponseFamily::ThreadStart)
            .expect("fresh response expectation slot accepts thread/start"),
        IncomingJsonExpectation::ThreadRead { id } => slot
            .install_fixed(id, crate::incoming_json::ResponseFamily::ThreadRead)
            .expect("fresh response expectation slot accepts thread/read"),
        IncomingJsonExpectation::ThreadResume { id } => slot
            .install_fixed(id, crate::incoming_json::ResponseFamily::ThreadResume)
            .expect("fresh response expectation slot accepts thread/resume"),
        IncomingJsonExpectation::ThreadFork { id } => slot
            .install_fixed(id, crate::incoming_json::ResponseFamily::ThreadFork)
            .expect("fresh response expectation slot accepts thread/fork"),
        IncomingJsonExpectation::ThreadCompactStart { id } => slot
            .install_fixed(id, crate::incoming_json::ResponseFamily::ThreadCompactStart)
            .expect("fresh response expectation slot accepts thread/compact/start"),
        IncomingJsonExpectation::ThreadInjectItems { id } => slot
            .install_fixed(id, crate::incoming_json::ResponseFamily::ThreadInjectItems)
            .expect("fresh response expectation slot accepts thread/inject_items"),
        IncomingJsonExpectation::ThreadUnsubscribe { id } => slot
            .install_fixed(id, crate::incoming_json::ResponseFamily::ThreadUnsubscribe)
            .expect("fresh response expectation slot accepts thread/unsubscribe"),
        IncomingJsonExpectation::TurnInterrupt { id } => slot
            .install_fixed(id, crate::incoming_json::ResponseFamily::TurnInterrupt)
            .expect("fresh response expectation slot accepts turn/interrupt"),
        IncomingJsonExpectation::TurnStart { id } => slot
            .install_fixed(id, crate::incoming_json::ResponseFamily::TurnStart)
            .expect("fresh response expectation slot accepts turn/start"),
        IncomingJsonExpectation::TurnSteer { id } => slot
            .install_fixed(id, crate::incoming_json::ResponseFamily::TurnSteer)
            .expect("fresh response expectation slot accepts turn/steer"),
        IncomingJsonExpectation::Poisoned => slot.poison_for_test(),
    }
    slot
}

pub(super) fn test_expectation_state(
    slot: &crate::incoming_json::ResponseExpectationSlot,
    installed: IncomingJsonExpectation,
) -> IncomingJsonExpectation {
    if slot.is_poisoned() {
        IncomingJsonExpectation::Poisoned
    } else if slot.is_idle() {
        IncomingJsonExpectation::Idle
    } else {
        installed
    }
}
