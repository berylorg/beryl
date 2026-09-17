# Reason For Investigation

Direct context and tool round trips require retaining actual returned items across requests.
The earlier one-shot probes were not a reusable protocol harness; the research now needs a
small Rust-owned boundary for request budgets, credential handling and sanitized observations.

# Outcome

A disposable helper under `localtest/subscription-probe-20260917` passed locked offline Cargo
metadata/check/build, four offline nextest checks and independent credential/diagnostic boundary
review on 2026-09-17. It is research tooling, not production implementation or proof of service
behavior. No live request was made by these qualification checks.

## Method And Scope

Standalone ignored Cargo workspace using cached `serde_json 1.0.149` with `preserve_order`,
`indexmap 2.14.0`, and the installed curl 8.18.0 transport. Build uses one job, LLVM linking,
no normal debug information and no incremental compilation. Invoke Cargo from a neutral directory
with the helper's explicit `probe-cargo.toml`; the task-local resolver lockfile and target
directory prevent touching the production graph. See the
[Cargo isolation correction](../../../failures/research-cargo-isolation.md).

Allowed modes are custom tool, function-tool round trip and compaction round trip. Endpoints are
fixed to `https://chatgpt.com/backend-api/codex/responses` and its `/compact` sibling.
There is no arbitrary URL/body interface. Each invocation has at most two sequential requests,
30 seconds per request, 10 seconds to connect, a 1-MiB response cap, 512-KiB request cap and
1,024-event parse cap. Curl's default configuration is disabled; redirects and retries are not
enabled. Every non-200 status or transport failure stops the invocation without another request.

The existing credential file is bounded to 64 KiB and read only. Access token and account ID
remain in process memory and curl stdin configuration, never command arguments or diagnostics.
Control characters in credential headers are rejected. No refresh, logout, revocation, credential
writes or external tools are implemented. Curl stderr is discarded; write/read/cap failures
kill and reap the exact child, and successful paths wait before parsing.

The helper buffers bounded responses for inspection and retains returned synthetic items for the
next request. It does not implement or prove production no-spill decoding, arbitrary-scale memory
bounds, server-side generation limits or remote cancellation. It preserves JSON member order,
reports known event/item/status labels and selected numeric usage counters, and reports lengths
rather than opaque content. Final synthetic answers are checked by equality, not printed wholesale.

## Verification

Four offline checks covered header-injection/request caps, preserved member order including
multiline SSE data, malformed/missing/oversized/incomplete response handling, and sentinel values
in unexpected metadata/usage. All passed with nextest. These checks validate the helper only;
they do not count as live API findings.

Independent review initially found raw metadata Value forwarding in summaries. It was replaced
with closed labels and selected numeric counters; the sentinel check passed and follow-up review
accepted the correction. Endpoint, credential, request-budget and normal child-disposal boundaries
were also reviewed.

Qualified SHA-256 identities:

- `src/lib.rs`: `2F2AC7247AFF091AD9EEA507A09869EEEFD2B3EDD055E4E48ED9F7E23B16C0E0`.
- `Cargo.lock`: `294603B571D7815C41C379A6241848D30BA05588BED541216A48AB4346E3B2FD`.
- Executable: `5739CBA4F3111D9857E662AE9F19F8127058675BDF86690E8DD41362370DB6B0`.

The helper is ephemeral and must be reclaimed after its last experiment. Durable live findings
record exact request conditions and outcomes separately; a successful qualification is not an
inference result.

# Sources

- Task-owned helper manifest, `doc/design.md`, `src/lib.rs`, `src/main.rs`,
  `tests/probe_boundary.rs` and `probe-cargo.toml`, qualified 2026-09-17 as identified above.
- Installed curl 8.18.0, libcurl 8.18.0/OpenSSL 3.6.0; no installation/download performed.
- Existing Beryl root Cargo.lock used to seed cached dependency versions; production tracked
  Cargo.toml and Cargo.lock remained unchanged.
- Root `doc/plan.md` research-helper boundary, with the production rework hold intact.
