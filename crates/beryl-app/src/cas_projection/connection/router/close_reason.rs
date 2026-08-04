/// Why one target or its owning connection stopped accepting live work.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LiveEventTargetCloseReason {
    /// Explicit local connection retirement revoked all targets.
    ConnectionRetired,
    /// Ordered backend ingestion failed.
    StreamFailure,
    /// Another operation or pending close already owned the exact source-publication route.
    SourcePublicationRouteUnavailable,
    /// An ordered source publisher failed while owning the exact publication permit.
    SourcePublicationFailed,
    /// Durable active-turn or activation publication failed before start exposure.
    TurnActivationPublicationFailed,
    /// CAS emitted an explicit protocol-error event.
    ProtocolError,
    /// A required CAS thread, turn, item, or tool-call identity was invalid.
    InvalidEventIdentity,
    /// One target observed a CAS turn other than its one-way bound identity.
    ConflictingTurnIdentity,
    /// A response did not match an exact dynamic-tool request routed to this target.
    ConflictingDynamicToolIdentity,
    /// The separately required exact permission-approval interruption failed.
    ApprovalInterruptionFailed,
    /// A feature operation arrived before a provisional target observed turn start.
    EventBeforeTurnStart,
    /// An operation arrived after the exact target turn completed.
    EventAfterTurnCompletion,
    /// The same provisional target attempted more than one `turn/start` dispatch.
    DuplicateTurnStart,
    /// The target's bounded operation queue was full.
    QueueOverflow,
    /// The sole target receiver disappeared before operation delivery.
    ReceiverAbandoned,
    /// CAS explicitly closed the target thread.
    ThreadClosed,
    /// The sole connection worker stopped.
    WorkerStopped,
    /// Remembering another retired remote thread lane would exceed the bounded fence.
    RetiredThreadLaneCapacity,
}
