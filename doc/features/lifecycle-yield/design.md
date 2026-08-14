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

- `phase_needs_review` stops after the current turn reaches terminal state so the operator can review or live-test the completed phase.
- `blocked_needs_operator` stops after the current turn reaches terminal state and requests an operator-attention notification.
- `phase_continue` records that the current turn should continue after terminal completion, then Beryl runs selected-thread context compaction and starts the next turn with Beryl's fixed continuation message.
- `plan_complete` stops after the current turn reaches terminal state and requests a completion notification.
- Automatic lifecycle continuation does not play ordinary end-turn sound for the turn that requested continuation.

## Continuation Behavior

- Beryl, not the model, chooses whether and how to compact before continuation.
- The fixed continuation message is Beryl-owned and is not supplied by the model.
- Automatic continuation sends the latest applied non-empty global developer-instructions setting as hidden developer-instructions context, subject to the composer feature's developer-instructions rules.
- Context compaction timeout behavior is governed by the settings/status-line contracts.

## Safety And Isolation

- Lifecycle yield outcomes do not mutate transcript history, semantic graph state, workspace persistence, settings, or backend-owned Codex configuration by themselves.
- Lifecycle notifications are GUI-local side effects and are governed by the notifications feature.
- Yield handling must not depend on ordinary end-turn sound eligibility.
