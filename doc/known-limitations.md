# Known Limitations

This follow-up register records accepted limitations and links their owning contracts and evidence.
It does not replace feature/system/package authority or declare missing behavior implemented.
Revisit these items after the Beryl-home rework is otherwise finished; accepted deferrals do not
block its remaining implementation and qualification.

## Automatic Thread Titles

Automatic model-generated titles are unavailable. Threads retain the existing Syndic
history-derived title and localized untitled fallback; any already accepted generated attribute
retains its normal display precedence. Submission, activation and transcript reading remain
available. The [thread feature](features/conversation-threads/design.md#automatic-title-generation-availability)
owns this supported behavior.

Official CAS 0.163.0 has an internal tool ceiling, but no qualified public inference-only operation
that excludes tools, hooks, MCP/app startup and subagents before effects. See the
[exact artifact/schema/source evidence](memory/github.com/openai/codex/commit/83086188ae0614bd882818ff1aa23f3194962012/inference-only-maintenance.md).
Prompt restrictions, read-only sandboxing and rejecting activity afterward cannot substitute.

Operator decided on 2026-10-22 that CAS will not be modified for this work. Background title
generation and its additional preparation/service wiring are deferred. The prior
[extension proposal](audits/title-maintenance-inference-only-proposal.md) is historical and is not
an active implementation or approval request. Revisit available upstream capabilities and the
desired feature scope after the rework is otherwise complete; no extension, pin change or unsafe
ordinary-session fallback is authorized.

## Historical Terminal-Turn Repair

The supported CAS history source does not establish complete exact terminal-turn content for the
current thread population. Recovery records explicit incomplete history and releases successor
gates only after bounded finalization and coherent publication. It does not infer missing content
from a partial full-view response. The
[historical repair contract](systems/cas-live-syndic-transcript/design.md#exact-terminal-turn-historical-repair)
owns eligibility and any future separately authorized repair. The
[source-completeness evidence](failures/cas-terminal-repair-full-view-is-not-completeness.md)
records the accepted limitation. Revisit source capabilities after the remaining rework closes.
