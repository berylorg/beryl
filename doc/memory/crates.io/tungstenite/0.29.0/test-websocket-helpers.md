# Reason For Investigation

Beryl needed a reusable exploration-memory note for the legacy dependency investigation migrated from doc/deps/tungstenite/0.29.0.md. The migration preserves source-entrypoint, feature, lifecycle, gotcha, command, and unresolved-question findings that future dependency work may reuse.

# Outcome

The legacy note is preserved below as a dependency exploration memory note for crates.io package tungstenite 0.29.0. It is supporting research only; design decisions remain in design docs and implementation sequencing remains in doc/plan.md.

# Sources

- Legacy note: doc/deps/tungstenite/0.29.0.md.
- Source identity: crates.io package tungstenite 0.29.0.
- Workspace dependency context: Cargo.toml and Cargo.lock in this repository at migration time.
- Additional upstream files, commands, feature flags, local use sites, and follow-up sources are listed in the migrated legacy details below.

# Migrated Legacy Details

## tungstenite 0.29.0

Verified on 2026-05-10.

### Workspace Use

- `beryl-backend` uses `tungstenite` as a test-only WebSocket server/client helper for managed WebSocket integration coverage.
- Production managed app-server client sessions now use the `soketto` transport path instead of tungstenite.
- Beryl enables `default-features = false` with the `handshake` feature for plain `ws://127.0.0.1:<port>` test handshakes and text-frame JSON-RPC fixtures.
- TLS and URL parsing features are not needed for the loopback `ws://` test boundary.

### Symbols Needed By This Workspace

- `tungstenite::accept_hdr`
- `tungstenite::connect`
- `tungstenite::handshake::server::{Request, Response, ErrorResponse}`
- `tungstenite::http::StatusCode`
- `tungstenite::protocol::Message`
- `tungstenite::protocol::WebSocket`

### Lifecycle And I/O Notes

- Test fake app-servers accept loopback `TcpStream` values with `accept_hdr` so tests can assert the `Authorization` header.
- Test fake app-servers use `WebSocket::read()` to inspect JSON-RPC requests and `WebSocket::send(Message::Text(...))` to send ordinary JSON-RPC text responses.
- Transport-boundary tests may write raw WebSocket bytes to `WebSocket::get_mut()` after the handshake when they need invalid frames that tungstenite would not produce through its safe send API.
- The blocking test API runs in test helper threads, not on the `gpui` thread.

### Integration Gotchas

- Keep tungstenite in `beryl-backend` dev-dependencies unless all fake app-server helpers are rewritten.
- Tungstenite safe APIs are useful for ordinary protocol tests, but invalid masking, reserved-bit, and fragmentation edge cases need raw frame bytes.
- The `connect` convenience remains useful for negative auth tests that verify unauthenticated clients are rejected by the fake server.

### Minimal Upstream Entrypoints

- `tungstenite-0.29.0/src/lib.rs`
- `tungstenite-0.29.0/src/server.rs`
- `tungstenite-0.29.0/src/handshake/server.rs`
- `tungstenite-0.29.0/src/protocol/mod.rs`
- `tungstenite-0.29.0/src/protocol/message.rs`
- `tungstenite-0.29.0/src/error.rs`

### Commands And Files Consulted

- `cargo info tungstenite@0.29.0`
- `rg -n "tungstenite|getrandom|WebSocket|websocket|token|capability" -S .`
- `Select-String -Path Cargo.lock -Pattern 'name = "tungstenite"' -Context 0,30`
- `Get-Content -Raw Cargo.toml`
- `Get-Content -Raw crates/beryl-backend/Cargo.toml`
- `Get-Content -Raw doc/design.md`
- `Get-Content -Raw crates/beryl-backend/doc/design.md`
- `Get-Content` and `rg` over the upstream source entrypoints listed above.

### Unresolved Questions

- None for the current workspace use.

