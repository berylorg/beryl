# Reason For Investigation

Determine how Beryl could own model context after removing CAS without confusing immutable
history, visible transcript and the selected input sent to inference. This is a candidate design
assessment under the production hold, supported by pinned client-source inspection.

# Outcome

Direct requests established small explicit-context tool continuations, reuse of actual opaque
reasoning and streaming compaction in the catalog-selected Lite dialect. They did not establish
long-task quality, repeated compaction, opaque portability across models or restart, or
summary-only retention of facts. Those limits must survive architecture synthesis.

The strongest simplification is that Beryl can select context directly from its own exact
lineage rather than synchronize a CAS-native thread and then inject, fork or roll it back.
The corresponding new work is a typed context store, deterministic request construction,
budgeting, compaction selection and valid handling of incomplete turns and tool results.

## Candidate Context Representation

Keep immutable history and a versioned context selection separately. A context revision names
its exact history frontier, retained input/output resources, completed call/result relationships,
opaque reasoning/compaction resources, model/dialect and instruction/tool-schema snapshot.
Visible narrative is a projection of some of those records; it cannot reconstruct all of them.
Operational reasoning need not become parent transcript text just because it is model input.

Retain the actual completed output items. The directly inspected terminal output arrays were
empty, and added reasoning/compaction opaque content differed from completed content. An
added item is not a substitute for its final opaque value. Preserve message role and phase,
content kinds, call identity and protocol metadata required by the chosen dialect. Opaque
content stays opaque; summaries rendered for humans cannot recreate it.

Use stable local references to immutable resources rather than keeping all historical JSON in
memory. Request serialization must iterate the selected resources with bounded escaping/base64
state and transport backpressure. This is an implementation requirement, not something proved
by the bounded full-body research helper. Existing final-resource appendability still needs the
explicit storage decision identified in the streaming assessment.

## Instructions, Tools And Media

Freeze instruction provenance and tool catalog at request dispatch. Separate base instructions,
project/user instruction material, Beryl's global developer setting and tool results. The
[integration assessment](configuration-and-integrations.md) records source role distinctions and
reload policy. Do not attach global developer settings to title, subagent, steering or compaction
purposes merely because they are available in the UI. Historical inclusion and fresh injection
are separate questions that the eventual context contract must settle.

An outstanding call binds its original schema and handler revision. A tool removed or changed
before dispatch cannot be redirected by name to another handler. A completed result pairs with
its exact call and is reused without repeating the effect. For failed or interrupted calls,
choose an explicit protocol-valid representation or disallow continuation; never invent a
successful result simply to repair input shape. Missing tool results and malformed input are
local context failures, not invitations to automatic effect replay.

Images require stable asset references plus the correct request representation; generated media
and external resource links require durable availability before later context depends on them.
A thumbnail, filesystem display path or web citation is not necessarily usable model input.
Changing image detail or truncating tool output changes the model's evidence, even when the
visible transcript stays the same. Preserve the distinction between complete stored evidence
and the bounded selection actually sent to inference.

## Compaction Ownership

The direct successful protocol appended `compaction_trigger` to ordinary Responses input and
received a completed compaction item. Lite formatting and V2 compaction selection are separate
decisions. The two unary 404 results do not make streaming compaction unavailable.

Candidate compaction captures one exact context revision, produces an unselected replacement,
and publishes it only after complete validated response and an atomic eligibility check. Keep
the old revision usable until publication; interrupted, failed or stale results cannot partially
replace it. Compaction must not delete immutable history or its original asset provenance.

The successful continuation retained the original user fact alongside compaction. It proves
acceptance of that request, not that encrypted compaction alone remembered the fact. Repeated
compaction must identify which earlier summary and retained messages it replaces; blind append
of every previous summary would grow context rather than manage it. Model/dialect switching
needs a compatibility policy: no observation yet proves arbitrary opaque context portability.
Possible choices include supported same-dialect reuse, re-compaction from retained original
history, or explicit unavailability. These are options, not fallback behavior already approved.

Budgeting must account for instructions, schemas, messages, tool results, images and reserved
output. Local estimates may guide an internal scheduler if approved, but current status-line
authority explicitly forbids presenting them as exact context usage. Preserve exact reported
usage and unknown values. The rejected `max_output_tokens` probe means a Platform-style output
cap cannot be assumed available on this subscription dialect.

Automatic compaction should be a bounded operation, not an unbounded retry loop after context
overflow. Manual compaction remains a provider operation, not an ordinary conversation message
or draft submission. Its UI timeout reports still-in-progress; it does not cancel, repeat or
publish success. Later exact completion still follows ordinary eligibility rules.

## Branches, Edits And Incomplete Turns

Replacement editing chooses the context immediately before the exact historical input being
replaced and creates a new immutable path. Reusing a later compaction could leak discarded-tail
information into the new response. Therefore any reused compaction needs a proven frontier at
or before the branch point. If that proof or source context is unavailable, disable the exact
operation rather than approximate it from similar transcript text.

Branch discussion retains its exact finalized assistant selection and source envelope. Its
parent may later select another path without rewriting that envelope. Parent handoff selects
eligible current parent context through the durable job gate; it does not rerun old discussion
effects or replace the parent's history with the child's context.

An incomplete response can contain valid partial observations without being a complete context
step. The execution assessment proposes conservative local-tool dispatch after response closure.
For future user continuation, explicitly define which completed observations can enter context
and how missing call/results are represented. Keep the original incomplete turn and unknown
effects; a later user retry is a new durable submission, not repair by replay.

Accepted input arriving during compaction stays ordered. Lifecycle continuation is process-local
intent until admitted, and accepted input takes precedence. Stop or shutdown cancels that intent;
successful compaction must not resurrect it. A compaction revision and permission to start a
new turn are separate facts.

## Pinned Client Policies And Their Limits

Codex `6b9826e3aa83b1a5947db50f4332cb9c65f1b340` normalizes a consumed history snapshot before
request construction. Missing function/custom/local-shell results receive synthetic `aborted`
outputs; missing tool-search results receive an empty completed tool list. Orphan paired outputs
are removed with exceptions for standalone/server events. Pairing uses call-ID membership, not
an exhaustive uniqueness/order proof. Root checked the normalization source: this is model-input
repair, never evidence that an external effect was actually aborted or absent.

The same pin checks model `comp_hash` metadata. Two present, differing values trigger compaction
with the previous model; a missing hash does not establish compatibility. A smaller-window model
can also trigger previous-model compaction when its relevant limit is reached. Selected failures
allow a current-model fallback on the Codex/OpenAI path. Root checked the switching predicates.
These client rules do not prove arbitrary opaque portability, and a slug change alone does not
always trigger compaction.

V2 rebuilds active history from selected messages under a 64,000-token retention budget and then
appends the new compaction item. Previous opaque summaries participate in the request but are
not retained as ordinary messages. Old assistant messages, reasoning and tool calls/results also
fail the retained-message filter. Genuine user/hook messages survive only subject to budget;
developer and selected agent messages have additional metadata/feature filters. Newest groups
are favored and boundary text can be truncated. This is deliberately lossy active context, not
a promise to retain all user text indefinitely.

Usage accounting combines reported totals with local estimates for appended items and, in some
cases, older encrypted reasoning. Instructions and tool-output truncation policy contribute.
Auto-compaction can budget total context or body after a prefix while also respecting an effective
model-window cap. These source heuristics do not become exact Beryl status-line values.

Media has independent hazards. Unsupported input modalities can become placeholders. Image
retention can charge estimated tokens and retain labels with the image, but the inspected routine
does not charge audio; text-only truncation can retain media outside its text budget. Image
preparation can resize/re-encode data URLs and rejects remote HTTP(S) URLs in that client path.
None of this proves a universal media-memory bound or a service prohibition on remote URLs.
Retain original assets independently and bound decoded pixels/bytes, not just estimated tokens.

## Remaining Verification

Explicit context is the demonstrated baseline. The
[inference surface](subscription-inference-surface.md) and
[execution assessment](durable-execution-and-recovery.md) describe source-supported WebSocket
previous-response reuse: it requires compatible cached request prefixes and response identity.
It is not demonstrated restart recovery or guaranteed durable server context; losing the cache
must not lose Beryl's only usable conversation representation.

Completion review accepted the research synthesis with these limits. Specify deterministic checks for
orphaned calls/results, schema changes, stale compaction, repeated summaries, branch-before-summary,
partial turns, asset loss and bounded outbound serialization. A small quality experiment may
later test summary-only fact retention, but no such result is claimed here. Expensive long-history
stress is not required merely to rediscover already-known uncertainty.

# Sources

- Source investigation at the pin above, inspected 2026-09-17:
  [normalization](https://github.com/openai/codex/blob/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/codex-rs/core/src/context_manager/normalize.rs),
  [history and usage](https://github.com/openai/codex/blob/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/codex-rs/core/src/context_manager/history.rs),
  [model switching](https://github.com/openai/codex/blob/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/codex-rs/core/src/session/turn.rs),
  [context thresholds](https://github.com/openai/codex/blob/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/codex-rs/core/src/session/context_window.rs),
  [retention](https://github.com/openai/codex/blob/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/codex-rs/core/src/compact_remote_v2.rs),
  [media budgeting](https://github.com/openai/codex/blob/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/codex-rs/core/src/compact_remote_v2_images.rs)
  and [image preparation](https://github.com/openai/codex/blob/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/codex-rs/core/src/image_preparation.rs).

- Direct [tool/reasoning continuations](direct-stream-continuation.md),
  [streaming compaction](subscription-streaming-compaction.md),
  [inference surface](subscription-inference-surface.md) and
  [ordering assessment](ordering-and-bounded-streaming.md), observed 2026-09-17.
- Local `doc/features/conversation-threads/design.md` Replacement Editing,
  `doc/systems/branch-discussion-handoff/design.md` Durable Discussion Binding and Parent Input,
  `doc/features/lifecycle-yield/design.md` Continuation Behavior,
  `doc/features/status-line/design.md` Context And Rate-Limit Cell,
  `doc/systems/syndic-conversation-history/design.md`; inspected 2026-09-17.
- Proposed context responsibilities and failure cases are architecture reasoning from these
  requirements. They are not approved schema or evidence of untested service behavior.
