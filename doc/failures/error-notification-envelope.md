# Error Notification Envelope

## CAS Omits the JSON-RPC Version Field

Beryl's special `method == "error"` branch in `crates/beryl-backend/src/session.rs::parse_incoming_value` required a `jsonrpc` string equal to `"2.0"`. The scoped CAS fork's `app-server-protocol/src/rpc.rs::JSONRPCNotification` contains only `method` and optional `params`; its module documentation and `app-server/README.md` explicitly state that CAS omits the version field on the wire. Requiring it only on error notifications converts valid backend turn errors into fatal stream failures.

The Operator reported repeated active-turn failures with `backend returned malformed error notification envelope: jsonrpc must be exactly 2.0`. Both WebSocket and stdio regressions reproduced this exact rejection before the fix using CAS-shaped omitted-field notifications. Each fixture covers retryable and non-retryable errors and a later matching terminal event. Existing fixtures generally supplied the version field and therefore missed this incompatibility.

The correction accepts an absent version field or an explicitly supplied string `"2.0"`, while retaining rejection of invalid supplied versions, any notification id, missing params, and invalid typed payloads. Exact thread/turn correlation and `willRetry` semantics remain unchanged. The backend runtime recovery feature and backend package design own this contract; plan Phases 7 and 8 own diagnosis and repair.

Pre-fix reproduction commands (both fail with the reported envelope error):

```text
cargo nextest run -p beryl-backend --features lifecycle-test-support --test managed_websocket managed_websocket_routes_cas_shaped_errors_and_later_terminal_events --jobs 2
cargo nextest run -p beryl-backend --features lifecycle-test-support --test launch_and_protocol stdio_routes_cas_shaped_errors_and_later_terminal_events --jobs 2
```

This establishes a source-level cause and deterministic reproduction, not provenance of every installed-binary incident. The parser is also used by compaction observation, but no captured incident establishes whether this caused any reported compaction timeout. Installed binaries are unchanged.

Post-fix verification passed 103 tests with `cargo nextest run -p beryl-backend --features lifecycle-test-support --test managed_websocket --test launch_and_protocol --jobs 2`. The existing app `turn_worker` tests also passed: `turn_error_events_are_nonterminal_and_streams_continue_to_completion` and both `error_before_completion` cases (three tests total). They preserve the distinction between a normalized turn error and actual transport/protocol failure. Formatting of changed Rust files, scoped diff checks, worker self-review, and main-thread review of the parser and regression fixtures passed.
