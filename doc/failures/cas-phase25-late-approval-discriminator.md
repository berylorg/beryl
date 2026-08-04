# Invalidated Approach

The incremental classifier selected only canonical method-first approvals. A root field such as
`id` selected the ordinary lane immediately. Completion then materialized the entire raw message as
`serde_json::Value`, inspected its final `method`, and converted a late approval method into a typed
envelope error.

# Why It Failed

That completion-time check was semantically fail-closed but architecturally too late. An incompatible
envelope such as `id`, multi-megabyte `params`, then an approval `method` grew the ordinary raw
`Vec` up to its proportional message budget and constructed a root DOM before rejection. It broke
Phase 25's compact-before-payload boundary and made a logical ordinary-message cap the approval
residency boundary.

Blindly treating every id-first message as incompatible was also invalid. The pinned producer emits
successful responses as `id,result`, while canonical error responses are `error,id` and may carry
arbitrary error data before the id. A classifier correction must preserve both response families
rather than narrowing unrelated valid protocol behavior.

The first quarantine correction was also too broad when an id-first envelope presented its method
before any request payload. Existing ordinary dynamic-tool compatibility envelopes use that order.
Once the bounded method scalar diverges from every approval and provider target, the retained fixed
prefix proves the generic ordinary lane and can be committed without DOM inspection. Quarantine
remains irreversible only after a request-like value or unknown field has already made replay
impossible.

# Architectural Correction

The root classifier now keeps an id-first prefix in fixed classification storage until it proves a
canonical ordinary response or encounters a request-like incompatible field. The latter transition
is irreversible: a fixed-state quarantine structurally discards values, watches only root method
discriminants, and returns a typed approval envelope error without activating raw capture or DOM.
Classification-prefix pressure while the family remains ambiguous also enters quarantine.

Canonical success and error response shapes still enter the ordinary lane. That committed lane owns
a fixed root sentry: any later root `method` is rejected before its value is consumed. An ordinary
non-target method proven while the fixed prefix is still retained selects the generic ordinary lane
before payload, while a late approval found during the pre-method generic probe fails before DOM
construction. A prefix that already entered quarantine cannot later reconstruct discarded bytes or
escape to ordinary. The completion-time approval inspection is removed, so it can no longer mask a
selector escape.

# Reusable Lesson

A post-materialization schema check cannot prove a compact ingress boundary. While a pinned
request/response ambiguity remains live, the selector must either prove the response family before
its first unbounded value or enter an irreversible fixed-residency rejection/quarantine path. A
separately selected generic ordinary family still needs an incremental root sentry so a later target
discriminator cannot rely on DOM inspection. Pinned order for every valid early-exit family is part
of that proof, not an optimization.
