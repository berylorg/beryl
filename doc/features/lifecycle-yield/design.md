# Goals

Let the model request a semantic lifecycle handoff while Beryl retains ownership of stopping, notifying, compaction, clean phase-thread creation, continuation text, and resume mechanics.

## Non-goals

- Letting the model control notification sounds, compaction strategy, resume prompts, or runtime mechanics.
- Letting the model choose continuation thread ids, fork roots, rollback scope, or thread lineage.
- Treating a yield tool result as proof that plan state or phase state is correct.
- Allowing multiple tool calls in one turn to race lifecycle decisions.

# Decisions

## Yield Tool Contract

- Beryl exposes a Beryl-owned app-server dynamic tool named `yield` on Beryl-created conversation threads.
- The tool accepts one required `outcome` value.
- Supported outcomes are `phase_needs_review`, `blocked_needs_operator`, `phase_continue`, `phase_continue_new_thread`, and `plan_complete`.
- The model chooses only the semantic outcome. Beryl owns the mapping to stopping, notification, context compaction, automatic resume, and exact continuation text.
- A successful tool response acknowledges that Beryl accepted the lifecycle request. It is not turn completion, compaction completion, resumed-turn start, or validation of the reported phase/plan status.
- At most one lifecycle yield outcome may control one backend turn. If multiple yield calls occur in one turn, Beryl applies a deterministic host-owned policy.

## Outcomes

- `phase_needs_review` stops after the current turn reaches terminal state so the operator can review or live-test the completed phase.
- `blocked_needs_operator` stops after the current turn reaches terminal state and requests an operator-attention notification.
- `phase_continue` records that the current turn should continue after terminal completion, then Beryl runs selected-thread context compaction and starts the next turn with Beryl's fixed continuation message.
- `phase_continue_new_thread` records that a successfully completed source turn should continue in a clean backend child thread. Beryl does not compact the source thread and sends the same fixed continuation message as `phase_continue`.
- `plan_complete` stops after the current turn reaches terminal state and requests a completion notification.
- Automatic lifecycle continuation does not play ordinary end-turn sound for the turn that requested continuation.

## Continuation Behavior

- Beryl, not the model, chooses whether and how to compact before continuation.
- The fixed continuation message is Beryl-owned and is not supplied by the model.
- Automatic continuation sends the latest applied non-empty global developer-instructions setting as hidden developer-instructions context, subject to the composer feature's developer-instructions rules.
- Context compaction timeout behavior is governed by the settings/status-line contracts.

## Clean Phase Thread Orchestration

- The first `phase_continue_new_thread` request in one phase sequence establishes its live source thread as the original orchestration root.
- Each successful continuation creates a persistent Beryl conversation thread by calling backend `thread/fork` against that original root, never against the phase child that most recently yielded.
- Beryl fully rolls back all inherited user turns in the fork before the child becomes selectable. The prepared child begins its first turn with no inherited effective conversation history and with only Beryl's fixed continuation message as new user input.
- The prepared child's backend `forkedFromId` must identify the original orchestration root. The thread selector derives the visible root-to-child relationship only from backend lineage, never from GUI-local substitute parent metadata.
- Beryl retains the exact orchestration-root identity as lifecycle provenance for generated phase children. That provenance controls later continuation preparation but does not replace, synthesize, or override backend thread lineage.
- Filesystem state and root `doc/plan.md` provide work continuity across clean phase contexts. Beryl does not copy source transcript content into the new context.
- The child keeps the root's exact execution target and workspace-member binding. Its continuation turn uses ordinary Beryl top-level turn request assembly, including the latest applied hidden developer instructions and other current turn settings.
- A lifecycle-created child follows normal Beryl-created persistent-thread registration, inventory, and title behavior. Beryl does not copy the root's display title as child lineage metadata.

## Phase Sequence

- When the operator chooses an orchestration-only root, the bootstrap request asks the root to call `yield(phase_continue_new_thread)` immediately instead of performing the first planned phase.
- The resulting first phase child performs the first planned phase. After completing a non-final phase, that phase child calls `yield(phase_continue_new_thread)` only when root `doc/plan.md` contains another phase to execute.
- The child that completes the final planned phase calls `yield(plan_complete)` and does not request another phase thread.
- The yield feature does not infer, count, or validate plan phases or prescribe a number of phase children. The lifecycle outcome chosen by the model from root `doc/plan.md` determines whether another child is requested.

## Serialized Handoff

- Beryl acknowledges and records a valid `phase_continue_new_thread` call against the exact active source thread and turn while leaving that turn live and selected.
- Child preparation must not begin until app-server reports that exact source thread and turn successfully completed and the source `TurnWorker` has finished and released its turn and dynamic-tool receivers. A terminal event whose thread or turn identity differs from the active source cannot enable worker completion, even if the other identity matches.
- Failed, interrupted, disconnected, or otherwise non-successful source turns do not create or select a child.
- If input accepted during the source turn is already queued for a later source turn, Beryl fails that queued input visibly through normal turn-delivery failure presentation before preparation begins. It does not run an extra source turn or copy that input into the clean child.
- While preparation is pending, Beryl disables conflicting user-facing thread selection, thread creation, history mutation, compaction, turn-start, steering, and stop controls. At no point may two user-visible conversation turns be active concurrently.
- Fork, rollback, history verification, and lineage verification run as background work. Beryl keeps the source selected until every preparation step succeeds.
- Only a fully prepared child may be registered and selected. Beryl then starts exactly one continuation turn in that child through the normal turn-start path.
- Every asynchronous request and result is guarded by exact workspace, source-thread, orchestration-root, execution-target, selection, and request-generation identities. A stale result must not switch or start work in an unrelated GUI state.

## Preparation Failure

- Definitive fork rejection, rollback, history-verification, lineage-verification, registration, or selection failure leaves the source thread idle and does not select a partially prepared or history-bearing child.
- If the ordinary continuation `turn/start` fails after the fully prepared child was registered and selected, Beryl leaves that empty child selected and idle, applies normal turn-delivery failure presentation, and does not retry automatically.
- Beryl reports preparation failures through a bounded surface notice.
- If fork succeeds but a later preparation step fails and app-server cannot delete the backend child, Beryl reports the child accurately as an orphan and refreshes thread inventory so it remains discoverable rather than hiding it.
- If a fork request may have committed but timeout or transport loss prevents Beryl from receiving the child id, Beryl reports an indeterminate fork outcome and warns that an unidentified backend child may exist. It keeps the source selected and idle, refreshes inventory, and does not guess which inventory row came from the request or select any candidate automatically.
- A successfully prepared but stale child result must not alter current selection or start a turn. Beryl registers or refreshes that child only within the still-valid original workspace and reports why automatic activation did not occur.
- When the original workspace is not selected as a late preparation result arrives, Beryl retains a bounded workspace-scoped notice and required inventory refresh for presentation when that exact workspace is selected again. It does not surface the result in or mutate the replacement workspace.
- Accepting deletion of the active workspace invalidates child activation immediately, then waits for that workspace's cancelling phase-thread preparation to reach a definitive result or bounded indeterminate drain before capturing the deletion persistence barrier and starting deletion. Late results are applied without activation while the workspace still exists. Before deletion begins, Beryl releases any deferred lifecycle outcomes scoped to the workspace being deleted and reports any known remaining backend child accurately; deleted-workspace outcomes must never recreate persistence or consume deferred-outcome capacity permanently.
- Beryl must not emulate clean context with `excludeTurns`, a fresh unrelated `thread/start`, transcript copying, or GUI-local fake parent metadata.
- Implementation must first prove that the targeted app-server preserves Beryl's registered dynamic tools through fork, full rollback, and the child's continuation turn. If the prepared child cannot call `yield` again, this outcome is unsupported and implementation stops without a workaround.

## Safety And Isolation

- Accepting a lifecycle yield tool call does not mutate transcript history, semantic graph state, workspace persistence, settings, or backend-owned Codex configuration by itself. After successful source-turn and worker completion, `phase_continue_new_thread` may mutate only the new backend child's inherited history through the defined fork-and-rollback preparation flow.
- Lifecycle notifications are GUI-local side effects and are governed by the notifications feature.
- Yield handling must not depend on ordinary end-turn sound eligibility.
