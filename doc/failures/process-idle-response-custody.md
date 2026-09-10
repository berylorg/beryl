# Completed Response Observations Are Not Required Execution Work

## Scope

Process-session idle eligibility and autonomous response-custody release in beryl-app and
beryl-backend. The app's [live control](../../crates/beryl-app/doc/design-live-control.md) retains
unsettled request obligations; the backend's [response contract](../../crates/beryl-backend/doc/design-live-control.md)
separates successful write, retained response handles and response authority.

## Invalidated Approach

Use the accepted shared process-work classification unchanged, then retire eligible sessions from
coalesced scheduler wakes emitted by view and app control/connection releases. Readiness inspection
after the shared-reader phase found that request completion is not sufficient to remove its work
classification, and the response source provides no completion/release notification.

## Decisive Evidence

- `crates/beryl-app/src/cas_projection/service/process_work/live.rs` classifies every
  `ConnectionWorkRecord::Request` as `request_handling`, regardless of successful response write
  or retained-capability count. Both the public inventory and targeted session reader share it.
- `connection/router/work_facts.rs` retains completed request observations. Its only pruning
  caller is `observe_request`; read-only pages intentionally do not prune them.
- The passing `approval_response_completion_and_presentation_custody_are_independent` case in
  `crates/beryl-app/tests/connection_work.rs` explicitly observes a retained request row after
  successful response write and after its capability count reaches zero.
- `crates/beryl-backend/src/response_work.rs` records write completion and final capability
  release under its observation lock but exposes only snapshot/revision reads, with no bounded
  consumer notification. Adding app-only release wakes cannot cover this last-owner boundary.

This can retain an otherwise idle execution session indefinitely without another request. The
shared-reader phase's 24 focused and 120 broader passing tests verified extraction and source
integrity but did not assert process-idle eligibility after completed response disposal. Independent
review of that phase also did not identify this classification gap.

## Required Correction

Before idle integration, distinguish completed response observation from unsettled required work
using exact response and app custody facts. Add bounded notification at successful response-write
and final capability-release cuts, outside observation locks, without response capability, payload,
new polling, replay, or dispatch authority. The app must connect those notifications to its
maintenance-only scheduler wake and preserve exact service/connection fencing.

Split the independently missing response-lifecycle prerequisite before activating the integration
phase. Verify retained completed rows, unwritten final release, registered versus removed targets,
notification/observation races and eventual idle retirement without later requests. Keep ordinary
request handling and permission-stop cleanup required until their actual owners release them.

## Status

The Operator authorized the recommended correction. The owning app and backend live-control
contracts distinguish completed response observations and specify one bounded completion wake.
Response classification is accepted with independent review, real response fixtures, 72 broader
regressions and production compilation. Both shared query paths now exclude completed response
records while preserving independently owned stopping and cleanup work.

An old permission test expected request work after the denial was already written; its assertion
now checks the written response and independently retained stopping work. An attempted pre-install
inventory assertion hung at the approval-install barrier and was removed; the isolated paused
response-writer case covers the pre-write obligation. The corrected regression suite passes.

Backend completion notification and its scheduler integration remain unaccepted prerequisites to
autonomous idle retirement without later requests.
