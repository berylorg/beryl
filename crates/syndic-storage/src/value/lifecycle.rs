/// Provider-owned operation represented as a Syndic turn when it owns emitted work.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ProviderOperationKind {
    /// Exact CAS context-compaction operation.
    ContextCompaction,
}

/// Immutable kind of one submitted Syndic turn.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum TurnKind {
    /// A user-authored submission and its agent response.
    OrdinaryUser,
    /// A provider operation that owns turn-scoped items.
    ProviderOperation(ProviderOperationKind),
}

/// Durable lifecycle of one submitted turn.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum TurnLifecycle {
    /// Durably admitted but not yet accepted by its execution provider.
    Pending,
    /// Accepted by the provider and not yet proven terminal.
    Active,
    /// Provider completion was observed.
    Complete,
    /// Exact interruption was observed or durably converged.
    Interrupted,
    /// Execution or local ingestion failed terminally.
    Failed,
    /// Capture ended with an explicitly incomplete durable suffix.
    Incomplete,
    /// Execution may have ended, but no exact terminal fact is known.
    UnknownTerminal,
}

impl TurnLifecycle {
    /// Returns whether this state is an explicit locally settled outcome.
    #[must_use]
    pub const fn is_proven_terminal(self) -> bool {
        matches!(
            self,
            Self::Complete | Self::Interrupted | Self::Failed | Self::Incomplete
        )
    }

    /// Returns whether recovery must withhold a competing same-thread turn.
    #[must_use]
    pub const fn blocks_same_thread_start(self) -> bool {
        matches!(self, Self::Pending | Self::Active | Self::UnknownTerminal)
    }
}

/// Closed outcome carried by one normalized turn-ending source event.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum TurnTerminalOutcome {
    Complete,
    Interrupted,
    Failed,
    Incomplete,
    /// The stream ended without an exact provider terminal fact.
    UnknownTerminal,
}

impl TurnTerminalOutcome {
    #[must_use]
    pub const fn lifecycle(self) -> TurnLifecycle {
        match self {
            Self::Complete => TurnLifecycle::Complete,
            Self::Interrupted => TurnLifecycle::Interrupted,
            Self::Failed => TurnLifecycle::Failed,
            Self::Incomplete => TurnLifecycle::Incomplete,
            Self::UnknownTerminal => TurnLifecycle::UnknownTerminal,
        }
    }
}

/// Exact status-only turn-ending fact, including a reason for incomplete capture.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TurnEndStatus {
    outcome: TurnTerminalOutcome,
    incomplete_reason: Option<TurnIncompleteReason>,
}

impl TurnEndStatus {
    /// Constructs one exact execution outcome with its independent captured-history fact.
    pub const fn new(
        outcome: TurnTerminalOutcome,
        incomplete_reason: Option<TurnIncompleteReason>,
    ) -> Result<Self, super::SyndicValueError> {
        if matches!(outcome, TurnTerminalOutcome::Incomplete) && incomplete_reason.is_none() {
            return Err(super::SyndicValueError::IncompleteTurnRequiresReason);
        }
        Ok(Self {
            outcome,
            incomplete_reason,
        })
    }

    #[must_use]
    pub const fn complete() -> Self {
        Self {
            outcome: TurnTerminalOutcome::Complete,
            incomplete_reason: None,
        }
    }

    #[must_use]
    pub const fn incomplete(reason: TurnIncompleteReason) -> Self {
        Self {
            outcome: TurnTerminalOutcome::Incomplete,
            incomplete_reason: Some(reason),
        }
    }

    #[must_use]
    pub const fn outcome(self) -> TurnTerminalOutcome {
        self.outcome
    }

    #[must_use]
    pub const fn incomplete_reason(self) -> Option<TurnIncompleteReason> {
        self.incomplete_reason
    }

    #[must_use]
    pub const fn lifecycle(self) -> TurnLifecycle {
        self.outcome.lifecycle()
    }
}

/// Provider-supplied phase of one transcript-visible assistant message.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum AssistantMessagePhase {
    Commentary,
    FinalAnswer,
    /// The provider supplied no recognized phase; Beryl does not infer one.
    Unknown,
}

/// Stable normalized kind of one pinned public CAS turn item.
///
/// This kind never changes as provider lifecycle or assistant phase advances.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ProviderItemKind {
    UserMessage,
    HookPrompt,
    AgentMessage,
    Plan,
    Reasoning,
    CommandExecution,
    FileChange,
    McpToolCall,
    DynamicToolCall,
    CollabAgentToolCall,
    SubAgentActivity,
    WebSearch,
    ImageView,
    Sleep,
    /// Standalone `image_gen.imagegen`; hosted Responses image generation is unsupported.
    StandaloneImageGeneration,
    EnteredReviewMode,
    ExitedReviewMode,
    ContextCompaction,
}

impl ProviderItemKind {
    /// Returns whether the pinned lifecycle permits completion without a preceding start.
    #[must_use]
    pub const fn permits_completion_only(self) -> bool {
        matches!(self, Self::SubAgentActivity)
    }
}

/// Provider-observation lifecycle retained independently from canonical finalization.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ProviderItemLifecycle {
    /// A local submitted item exists but has not yet been correlated with provider events.
    AwaitingCorrelation,
    Started,
    Completed,
}

/// Why one observed item cannot support a complete-history claim.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum UnsupportedHistoryReason {
    UnknownPublicItem,
    MalformedRequiredField,
    UnsupportedRequiredPayload,
    HostedImageGeneration,
    ImpossibleLifecycle,
}

/// Why source capture converged as incomplete.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum TurnIncompleteReason {
    StreamLost,
    AuthorityLost,
    WorkerStopped,
    CompletionMismatch,
    ItemAuditFailed,
    UnsupportedHistory(UnsupportedHistoryReason),
}

/// Durable delivery lifecycle for one accepted input fragment.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum AcceptedInputLifecycle {
    /// Admitted locally and awaiting a delivery attempt.
    Admitted,
    /// One exact delivery attempt is in progress.
    Delivering,
    /// Provider acceptance was proven.
    Delivered,
    /// A bounded later attempt is allowed.
    Retryable,
    /// No further automatic delivery attempt is allowed.
    Failed,
    /// The request may have reached the provider, but no authoritative response is available.
    ///
    /// The admitted input remains permanent history and must never be replayed automatically.
    DeliveryUnknown,
}

impl AcceptedInputLifecycle {
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Delivered | Self::Failed | Self::DeliveryUnknown)
    }
}

/// Top-level lifecycle of one exclusive CAS projection binding.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum BindingLifecycle {
    Unbound,
    Valid,
    Active,
    Stale,
}

/// Rebuild status of one derived transcript or resource projection.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ProjectionLifecycle {
    Current,
    Stale,
}
