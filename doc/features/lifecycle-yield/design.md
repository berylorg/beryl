# Goals

Let the model request a semantic lifecycle handoff while Beryl retains ownership of stopping, notifying, compaction, continuation text, and resume mechanics.

## Non-goals

- Letting the model control notification sounds, compaction strategy, resume prompts, or runtime mechanics.
- Treating a yield tool result as proof that plan state or phase state is correct.
- Allowing multiple tool calls in one turn to race lifecycle decisions.

# Decisions

## Yield Tool Contract

- Beryl exposes a Beryl-owned app-server dynamic tool named `yield` on Beryl-created conversation threads.
- The tool accepts one required `outcome` value.
- Supported outcomes are `phase_needs_review`, `blocked_needs_operator`, `phase_continue`, and `plan_complete`.
- The model chooses only the semantic outcome. Beryl owns the mapping to stopping, notification, context compaction, automatic resume, and exact continuation text.
- A successful tool response acknowledges that Beryl accepted the lifecycle request. It is not turn completion, compaction completion, resumed-turn start, or validation of the reported phase/plan status.
- At most one lifecycle yield outcome may control one backend turn. If multiple yield calls occur in one turn, Beryl applies a deterministic host-owned policy.

## Outcomes

- `phase_needs_review` stops after the current turn reaches terminal state and requests one review-
  ready notification so the operator can review or live-test the completed phase.
- `blocked_needs_operator` stops after the current turn reaches terminal state and requests an operator-attention notification.
- `phase_continue` records process-local intent to continue after terminal completion. If separately
  accepted user input does not take precedence, Beryl runs selected-thread context compaction and
  starts the next turn with Beryl's fixed continuation message.
- `plan_complete` stops after the current turn reaches terminal state and requests a completion notification.
- Automatic lifecycle continuation does not play ordinary end-turn sound for the turn that requested continuation.
- Any exact soft stop admitted for the turn before terminal completion cancels that turn's pending
  automatic continuation. A user, diagnostic controller, window-close barrier, or Beryl-owned
  interrupting approval must not appear to stop a turn and then silently restart it; the stop does
  not discard separately accepted queued user input.
- When CAS-live supplies an eligible pre-admission soft-stop fallback for the exact yielding turn,
  pending automatic continuation is canceled before interruption dispatch. Separately accepted
  user input remains visible, preserved, and ordered.

## Continuation Behavior

- Beryl, not the model, chooses whether and how to compact before continuation.
- The fixed continuation message is exactly `Continue from the root doc/plan.md.`. It is
  Beryl-owned, is not supplied by the model, and appears as Beryl-authored continuation input rather
  than input authored by the operator.
- A pending automatic continuation does not survive Beryl process loss. Restart never recreates it
  from the yield tool item, transcript text, compaction history, or plan state.
- Once Beryl reports the fixed continuation as accepted and it appears as a conversation turn, it
  is no longer pending lifecycle intent. That visible turn survives and recovers like any other
  ordinary conversation turn; restart still does not create an additional one.
- Beginning the owning window's close barrier cancels the pending continuation before Beryl decides
  whether there is still an interruptible turn. This remains true after the yielding turn has
  finished or compaction has begun, and does not depend on the whole process exiting.
- After the yielding turn finishes, already accepted next-turn input wins. Beryl cancels the
  automatic continuation, does not start its compaction, and preserves the user's accepted order.
- Input submitted while automatic compaction is running is visibly accepted and queued. When
  compaction succeeds, already accepted input wins and consumes the automatic continuation;
  otherwise Beryl starts the fixed continuation while leaving the current composer draft
  unchanged. Input accepted after that continuation has started remains ordered behind it.
- The Beryl-authored continuation follows ordinary conversation execution behavior after it starts,
  including normal transcript capture and stop controls. Its distinct origin prevents Beryl from
  presenting it as operator-authored input.
- Compaction failure, interruption, stop, or lost backend authority cancels automatic continuation
  without discarding accepted user input. Completion-wait timeout alone does not cancel it; exact
  success observed later in the same process still follows the same user-input precedence.
- One lifecycle request can start at most one automatic continuation.
- Automatic continuation uses the ordinary new-turn admission safeguards. If the required home
  eligibility cannot be established, no backend turn starts, separately accepted input remains
  preserved, and Beryl reports one bounded continuation failure.
- An uncertain continuation-admission outcome never causes a compensating duplicate. A
  continuation proven admitted remains visible and recovers as ordinary admitted work; otherwise
  pending automatic continuation ends with bounded failure feedback.
- If Beryl cannot prepare the fixed continuation after compaction succeeds, it reports the
  continuation failure, does not substitute different text, and preserves accepted user input.
- Automatic continuation sends the latest applied non-empty global developer-instructions setting as hidden developer-instructions context, subject to the composer feature's developer-instructions rules.
- Context compaction timeout behavior is governed by the settings/status-line contracts.

## Notification Requests

- Review-ready, operator-attention, completion, and automatic-continuation-failure notices use the
  bounded destination, deduplication, priority, acknowledgement, content, and enqueue-failure
  behavior defined by the [Notifications feature](../notifications/design.md#lifecycle-notifications).
- Accepting a yield outcome does not claim that its requested notice was displayed or acknowledged.
  Notice admission failure never changes the selected lifecycle outcome, validates its report, or
  causes Beryl to repeat the yield request.

## Safety And Isolation

- Lifecycle yield outcomes do not mutate Syndic conversation history, Beryl-home durable state, settings, or backend-owned Codex configuration by themselves.
- Lifecycle notifications are GUI-local side effects and are governed by the notifications feature.
- Yield handling must not depend on ordinary end-turn sound eligibility.

# Engineering Rigor

Profile: `production-application/v1`

Modifiers:

- `external-side-effects/v1`
