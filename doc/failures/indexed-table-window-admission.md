# Indexed Table Window Admission

Initial Table mounting coupled generic indexed Range admission to the grid's header-before-body
staging sequence. An existing narrow body-only provider query then failed residency admission,
even though its exact binding, root, rectangle and payload satisfied the provider contract. Native
Table readiness also failed and required separate diagnostics rather than an assumption that all
timeouts shared this cause.

Generic provider response admission and coherent grid publication are different boundaries.
A standalone bounded body response may be admitted without publishing a headerless grid. The
mounted grid stages its bounded header/body window and publishes only after complete exact-root,
binding and rectangle agreement. Source and anchor actions retain the same generic query path.
Tests must preserve the existing narrow body-only provider case alongside mounted window staging.

The same correction found shared response Arcs charged twice when retained by both an indexed
entry and its coherent/staged window. Count each actual shared payload once within that residency
owner, while separately charging copied widget state and distinct old/replacement payloads. Do
not infer allocation duplication solely from multiple owning references.

Mounted qualification exposed another boundary: after Header completion, replacing the indexed
entry with a Body request clears its response while that request is in flight. Treating this
response-absent frame as an initial window request restarted Header, discarding the unread Body
and staged Header indefinitely. Exact-root metadata and advancing presentation revisions proved
that construction had completed; revision movement alone did not prove cell realization.

Keep the current Header/Body query and continuation cursor while its requested rectangle matches
the measured desired window. A regression must span Header completion, Body admission, intervening
response-absent frames and Body continuation/completion. Changing fixture size or extending a
readiness timeout cannot repair this query cycle.

Independent review also found that final replacement promotion removed the old Table window from
candidate byte accounting before measured publication released the current host. Earlier partial
staging charged both windows, but the last page could pass a candidate-only budget check despite
their physical overlap exceeding the limit. Admission must account for the current host and its
measured replacement through publication, deduplicating shared allocations while charging distinct
old and replacement pages. Qualify the final-page transition near the budget limit and its release.

Native diagnostic qualification then isolated a supported-limit conflict. The admitted viewport
requested six Header cells and twelve body rows across six columns. Owner-projected responses
carried one item per page: six Header pages and ten Body pages exhausted the sixteen-page staging
limit, leaving the next cursor at body row 1, column 4. No complete window was published; measured
and published presentation revision settled at 76. Query, budget and fallback regression cases
passed, so neither increasing a timeout nor correcting those already-qualified paths fixes this
limit.

Run `9b070246-fa25-42f1-a2a8-ee7c7f936d13` passed 23 nonnative cases and failed the one native
readiness case. Its content-free diagnostics identify the exact retained-page frontier. The root
plan records the Operator-approved correction to permit up to 128 Table response pages while keeping
the 128-descriptor window and existing byte budget; Code's sixteen-page limit remains separate.
Implementation and native qualification remain pending; the changed limit is authorized.

Storage Range projection iterates toward the descriptor allowance but permits an intermediate
Limit after successful progress to close a bounded response. Cell, alignment and source-fragment
lookups share the per-query ledger. There is no contractual minimum number of descriptors per
response, so the observed one-item pages do not establish a storage defect. The resident window
must accommodate valid bounded continuation rather than assume a batching factor.

The native complete-source clipboard-cap fixture later stalled at a cell crossing the 65,536-byte
source-chunk boundary. The query reserved a 65,536-byte response alongside two source chunks and
index/item allocations, exceeding the fixed 524,288-byte query ledger before returning an item.
Read-only qualification of the identical captured root and cursor returned Limit with that
allowance and one exact item with a 16,384-byte allowance (8,082 encoded bytes), leaving Home
revision 64 unchanged and frozen reads at zero. Run `f8804030-b12d-4392-af33-5eb001ccc54a` passed.

Table Header/Body queries now request the smaller response allowance under the existing smaller-
allowance contract. Keep the ledger, resource budget, Source/anchor/Code limits and exact cursor
fences unchanged; this correction narrows the request rather than increasing retained capacity.
The temporary observer is removed before final qualification, which must cover the real native
crossing and clipboard refusal/Save behavior.
