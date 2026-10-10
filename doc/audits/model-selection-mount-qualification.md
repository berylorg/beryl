# Model Selection Mount Qualification

## Boundary

Accepted following runtime/build qualification and independent completion review. The
selected-thread status cell and anchored selection menu implement the
[status-line contract](../features/status-line/design.md#model-and-reasoning-cell),
[status menus](../features/status-line/gui.md#status-operation-menus),
[runtime ownership](../systems/backend-runtime/design.md) and
[bounded backend operations](../../crates/beryl-backend/doc/design-thread-operations.md).

The process graph publishes a weak model reader. Queries authenticate the selected window claim,
Home/service generation, execution binding, admitted Ready runtime attempt and exact idle gate.
Configuration reads use that execution root's cwd; model reads use separate bounded request
connections and pages of at most 64 records. Reading Unknown never launches CAS.

The reusable bundled anchored-menu widget owns placement, virtualization, logical focus,
realized-row activation, dismissal, disabled explanations and focus return. The feature retains
two presented source pages and one bounded replacement, with fixed selected/focused record pins.
It does not aggregate the inventory. A 320px viewport uses 30px rows, zero overscan and at most
12 retained visible index/identity pairs including fractional boundary rows. Popup invocation,
exact model/effort identities, original preparation fences and current publication elections
reject stale callbacks or results. Manual Retry preserves the original query and sealed cursor;
closing or retiring the scope drains retained results and capacity.

Explicit choices stay in bounded thread/binding/revision-specific execution state. The ordinary
scheduler carries the choice into the next real turn and resolves current developer instructions
at dispatch. Exact rejection or uncertain start retains the choice; only an accepted matching
turn-start response consumes it. Selection creates no synthetic turn or global configuration
write. A stale acknowledgment cannot consume a newer choice, and a pending-choice absence ABA
cannot publish an obsolete status snapshot. Backend menu defaults never fill missing effective
reasoning values.

## Verification

All 176 distinct selected cases pass:

- App boundary run `78209c17-99ce-4c42-98b2-400b005f60eb`: 45/45 in 55.766 seconds.
  This includes fifteen genuine source cases, bounded reader/runtime cases, seven menu cases,
  three reusable-widget cases, the stale reasoning-row callback case, six actual choice/dispatch
  cases and eight native scenarios.
- Current App regression run `dab77f05-141b-400f-b58f-0105e957397d`: 88/88 in 70.557 seconds,
  covering status, mounted exact status controls, runtime interest, managed runtime interest,
  accepted-next scheduling and attempt policy.
- Current Syndic regression run `1ffa40b3-c89e-463d-9344-b712bb53e9df`: 35/35 in 134.510 seconds,
  covering live history and native projection after the explicit-profile fixture extension.
- Backend parser run `9b0034e8-0906-4cea-a1a5-ad337dcbc8c8`: 8/8 on unchanged current inputs,
  covering configuration proof/defaults, effort shapes, retained text/cursor bounds,
  the 64-record limit and malformed facts.

Native scenarios positively exercise progressive pointer/keyboard/reasoning selection,
initial/later exact Retry, bounded eviction/replay, readiness retirement, authentic completed
metadata, passive Unknown and complete owned Exit teardown. A real chosen ordinary turn carries
the selected model/effort and latest instructions; subsequent resume of that same backend thread
on the same admitted connection presents metadata derived from its actual accepted request.
The native completed-history case independently mounts authentic loaded metadata without a
pending choice or synthetic turn.

Canonical all-target checking passes for App, executable, Backend, State, Home and Syndic with
`beryl-app/test-faults`; production-default App/executable checking passes too. Both checks use
locked offline resolution. All 53 intended Rust source inputs match the canonical copies and
remain unchanged since the passing boundary run. Scoped rustfmt and whitespace checks pass.
Root/canonical manifests and lockfile match; the canonical graph contains exactly one qualified
`gpui-text-input` at `85223650b92f2b4dc65dd158a4ad0d94228dc7f8`, with the qualified GPUI
and settings pins preserved.

## Fixture Corrections And Resource Custody

The [provider fixture lesson](../failures/model-selection-provider-fixtures.md) records inherited
nonblocking sockets, ordered protocol payloads, nullable configuration proof, owned runtime
retirement, heap-backed fixed model-page slots, canonical history-profile admission and normal
Exit confirmation. No parser weakening, timeout increase or alternate recovery route was used.

All 221 distinct Homes logged in retained phase evidence are absent. Early failed native runs
required exact owned-path cleanup; the successful final run removed all 35 of its Homes normally.
One early cleanup inspection failed; its receipt records that limitation, and subsequent
inspections used the corrected terminating reparse guard. Final absence is independently checked.
Every launched verification process has retired.

Native programs use Operator's Windows session with minimized startup requested as best effort;
original pointer, keyboard, activation and focus assertions remain intact. Evidence under
`.tmp/model-selection-evidence` remains below its 96 MiB budget and is retained for completion
review. The shared build target and canonical checkout are preserved.
