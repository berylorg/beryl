# Reason For Investigation

Assess operational responsibilities transferred from CAS: transport, authentication, packaging,
service drift, resource limits and diagnostics. Avoid mistaking successful research requests for
an already qualified production client.

# Outcome

Direct subscription ownership is technically plausible without an auth-only CAS helper. It
trades app-server process/protocol coordination for maintenance of a subscription dialect and a
local agent runtime. Supporting only personal Pro reduces account/product scope but does not
remove credential rotation, OS execution, MCP, TLS or failure-recovery responsibilities.

This assessment combines retained direct observations, pinned source research and local manifest
inspection on 2026-09-17. No new live request, install, network-failure experiment or license audit
was performed. Proposed handling below is not a claim about unobserved service behavior.

## Transport And Dependency Boundary

Existing backend dependencies include Tokio, bounded-json and WebSocket plumbing for CAS, but
the inspected workspace/backend manifests do not select a direct subscription HTTP/TLS/OAuth
stack. Choose a maintained Rust client with controllable streaming and cancellation, then qualify
its exact resolved dependency/features. Reusing Codex's whole client can import auth, retry and
buffering policies that conflict with the proposed ownership boundary. The disposable curl-based
research harness is evidence equipment, not the selected production transport.

Required verification includes TLS trust behavior, proxy configuration, bounded request deadlines,
quiet-stream handling, redirects, decompression expansion, HTTP body backpressure, SSE line/event
limits and cancellation while storage is blocked. Never forward bearer credentials to an
unapproved redirect destination. Distinguish locally proven nondispatch from failure after request
bytes may have been sent. Do not use a blanket retry middleware for effectful inference/tool calls.
Preserve root authority: quiet active streams are not failures. Qualification must not introduce
an inactivity timeout or whole-response deadline for ordinary active inference. Research-probe
deadlines and bounded control-request timeouts are different from that production stream policy.

Support needs an explicit network envelope, including host versus WSL proxy/trust differences.
No current probe establishes corporate proxy or custom-CA support; excluding enterprise accounts
does not establish that every Pro user's network is uncomplicated. Compatibility can be bounded
without implementing a general enterprise configuration system.

## Account And Credential Ownership

The [auth assessment](subscription-auth-assessment.md) records the direct Pro account path and
source-supported login/refresh mechanisms. Select one refresh owner per credential family across
processes/homes. Fence late persistence against logout/account switch, and treat failed saving
after remote rotation as a distinct recovery problem. A process-local mutex is insufficient when
multiple programs share the store.

An auth-only helper still needs a narrow secure handoff, liveness/error handling and single-owner
coordination. It can reduce login implementation work but retains packaging/version coupling and
does not solve context, tools or streaming. Reading credentials owned by the active research
session was authorized for these probes; that is not a production credential-sharing design.

Separate unsupported plan, expired access, login-required, transient network failure, model
capacity and quota exhaustion. Keep a request bound to its exact account and model. Do not
silently switch models or accounts to recover. Token claims alone do not establish current
personal-Pro entitlement in arbitrary mixed-account sessions; retain explicit unknown/unsupported
states and qualify that admission path before shipping.

## Service Drift And Diagnostics

The live model catalog varied with client version, Luna selected Lite/code mode, ordinary
function requests still worked, `max_output_tokens` was rejected, unary compaction returned 404,
and streaming compaction succeeded. These are concrete reasons to version dialect/capability
handling rather than treating the Platform API schema as the subscription contract.

Use an honest client identity and known source-backed endpoints. Model metadata and observed
schema changes should invalidate affected capabilities or fail the exact operation clearly;
do not scan alternate routes or retry indefinitely. A new unknown field can be skipped or retained
only under an explicit bounded schema policy; unsupported semantic events cannot be silently
declared complete. Pin reviewed upstream evidence and use small opt-in regression probes when a
material compatibility question changes, not periodic quota-consuming traffic merely for health.

Useful diagnostics include local request/item identities, model/dialect revision, byte/event
counts, buffer high-water marks, timings, closure/cancellation reason, storage generation and
classified status/error codes. Redact credentials, authorization headers, prompts, tool results,
opaque reasoning and image bytes. Provider request IDs may aid support but are not public content
or proof of idempotency. Default diagnostics should not become a second raw conversation archive.

## Packaging, Resources And Licensing

Native HTTP-only work is relatively small compared with tool-host packaging. V8 adds a large
native runtime; PTY/sandbox helpers differ across Windows, WSL/Linux and macOS. A subprocess
execution host reduces GUI fault coupling but requires exact binary admission, lifetime cleanup,
protocol compatibility and distribution. Removing CAS does not remove these process concerns.

Root's cross-platform requirements remain. A Windows research run qualifies neither WSL sandbox
behavior nor macOS packaging. Select supported deployment tiers and test each retained execution
environment; do not silently reduce the product to the development host.

Inspected Codex components inherit Apache-2.0, with NOTICE/attribution concerns already recorded.
No final reusable dependency set has been selected, so a transitive/native license and
redistribution audit cannot yet be complete. It is a pre-distribution gate, including V8 and any
bundled execution helpers, not evidence of a current legal blocker. Do not promise that every
useful upstream crate is independently reusable at negligible cost.

Bound active requests, tool processes, JS heaps/output, MCP catalogs/messages, media decode,
storage pages and shutdown work at their owners. Token budgets do not bound bytes/pixels; SSE
framing limits do not bound decompressed transport; an output truncation limit does not bound a
retained JavaScript map. These independent limits need local measurement and failure checks.

## Maintenance Tradeoff And Open Gates

The direct runtime removes dependence on CAS listener startup, route-late event wrapping,
native history synchronization and app-server release admission. It assumes responsibility for
model/dialect changes, login/runtime releases, execution security updates and integration
compatibility. Selective dependency reuse reduces some implementation but preserves upstream
upgrade coupling. An auth helper addresses only one subset of that cost.

Remaining gates are exact dependency selection, packaging/license review, supported transport
envelope, secure credential storage/admission, cross-platform tools and resource qualification.
No evidence supports a precise calendar estimate or guaranteed future subscription compatibility.
These unknowns can be assigned to bounded implementation validation after architecture selection;
disruptive login/revocation tests on the active account are not needed for this decision.

# Sources

- Local `Cargo.toml`, `crates/beryl-backend/Cargo.toml`, root platform/persistence/resource
  requirements, and [storage/product assessment](storage-and-product-changes.md), inspected 2026-09-17.
- [Auth assessment](subscription-auth-assessment.md), [inference surface](subscription-inference-surface.md),
  [streaming compaction](subscription-streaming-compaction.md),
  [tool-host options](local-tool-host-options.md), [integrations](configuration-and-integrations.md),
  [streaming assessment](ordering-and-bounded-streaming.md) and their pinned/direct sources.
