# Phase 71 Context-Compaction Direct Mount Invalidated

## Scope

Checkpoint 3 context compaction, interruption, restart recovery, and automatic lifecycle
continuation.

## Invalidated Approach

The prior plan treated context compaction as ready to mount by connecting the existing
`ContextCompaction` provider item, `ProviderOperation` turn kind, `Compacting` input-gate
placeholder, backend request stub, stop target kind, and lifecycle-yield intent.

That assumed the placeholders already implied one coherent durable operation lifecycle and that
ordinary active-turn, terminal, recovery, and accepted-input paths could supply the missing details
during implementation.

## Evidence

- The compacting gate named only a Syndic turn while no durable compaction record owned admission,
  a dispatch attempt, request disposition, loaded authority, or CAS-turn publication.
- Ordinary binding activation requires a pending conversation turn and increments the native CAS
  model-turn count, which a provider-operation turn must not do.
- Generic terminal handling could return a nonterminal compaction turn to pending ordinary work.
- Startup explicitly deferred compacting gates and the app counted them without convergence.
- The backend stub exposed neither the non-idempotent request taxonomy nor the distinction between
  the empty compact-start acknowledgement and streamed completion evidence.
- Feature authority did not define timeout expiry, fixed continuation text, restart survival, or
  precedence against accepted user input.
- Stop required an exact CAS turn id, while compaction authority before provider turn publication
  had no defined cancellation or ineligible result.
- The lifecycle-continuation identity contract included `BerylHomeId`, but the accepted V1
  compaction record did not retain that admission fact. Settlement instead accepted a caller-
  supplied home id and could validate wrong-home derived identities against the same untrusted
  value.

## Why It Failed

The placeholders described vocabulary, not a state machine. Reusing the ordinary turn path would
have advanced conversation and native lineage incorrectly; implementing directly would instead
have forced source code to invent durable identity, retry, terminal, and restart policy outside
authoritative documentation.

That is an architectural gap. A local shell worker, inferred CAS identity, generic pending-turn
fallback, restart retry, or fabricated user-authored continuation would hide it rather than close
it.

## Course Correction

Phase 71 now completes one explicit target contract before source work resumes:

- a parentless provider-operation turn and dedicated durable compaction record;
- one claimed non-idempotent request attempt through the authenticated foreground driver;
- ordered CAS-turn, marker, and terminal correlation independent from request acknowledgement;
- fail-closed outcome, stop, timeout, and restart convergence;
- queue-only accepted input with one atomic user-work-versus-lifecycle-continuation settlement;
- exact Beryl-owned continuation text and origin without changing the composer draft.
- durable retention of the admission home identity so storage, not the settlement caller, owns the
  domain input for lifecycle-continuation turn and item derivation.

Phase 72 owns the clean source mount in focused backend, storage, and app modules. No compatibility
adapter or archived shell path is authorized.

## Resolution

Phase 71 closed the initially known gaps before source implementation resumed. The contract
included the dedicated `compaction-operations` family and record versions, exact
provider-operation turn and snapshot derivation, ordered thread-status and turn/item ingress,
terminal-before-response reconciliation, provider-specific stop handoff and safe reopen, every
restart disposition, and fixed lifecycle-continuation turn, item, content, empty-asset, close, and
user-precedence authority.

Phase 72's required end-to-end storage fixture then proved that the durable record could not verify
the home-domain part of those derived identities. The accepted clean correction makes the V1
record retain the admission `BerylHomeId` and removes the settlement caller's home-id choice. The
target authority was reconciled before implementation resumed; no compatibility decoder or
alternative identity is permitted.

The pinned CAS 0.144.1 investigation is retained under `doc/memory/github.com/openai/codex/commit/`
and proves the empty acknowledgement, subscription split, active-task replacement hazard, exact
turn/item identity order, successful idle boundary, interrupted forced-abort limitation, and failed
`systemError` boundary. Scoped Markdown and diff checks passed. A fresh independent completion
review found the initial omissions above and returned no remaining findings at that time. The
later storage-fixture finding supersedes the claim that Phase 72 can finish without one additional
authority correction.

## Affected Authority

- `doc/features/status-line/design.md`
- `doc/features/lifecycle-yield/design.md`
- `doc/features/composer/design.md`
- `doc/systems/syndic-conversation-history/design.md`
- `doc/systems/syndic-conversation-history/concepts.md`
- `doc/systems/cas-live-syndic-transcript/design.md`
- `crates/beryl-backend/doc/design.md`
- `crates/syndic-storage/doc/design.md`
- `crates/beryl-app/doc/design.md`
- `crates/beryl-state/doc/design.md`
- `doc/rework/beryl-home/REWORK.md`
- `doc/plan.md`
