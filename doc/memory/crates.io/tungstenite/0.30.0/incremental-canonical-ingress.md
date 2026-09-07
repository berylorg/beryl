# Reason For Investigation

Audit BR-009 evaluated replacing Beryl's custom incremental WebSocket payload reader with a mature
crates.io framing implementation. Beryl already uses tungstenite 0.29.0 for test servers.

# Outcome

Retain the production soketto-based fragment reader. In tungstenite 0.30.0, even the low-level public
FrameSocket::read path enters FrameCodec::read_frame, reserves the declared full payload length,
and waits until all frame bytes are buffered. This does not satisfy canonical ingress that must
make progress through bounded pages without a whole-frame or whole-message input cap. A mature
whole-frame API is not an equivalent streaming boundary. No new canonical cap is proposed.

The inspected latest stable release was published 2026-07-11. The project dates to March 2017 and
had roughly 294 million crate downloads at the 2026-09-06 registry check; Beryl's test-server use
is direct adoption evidence. License is MIT OR Apache-2.0 and MSRV 1.85. Default handshake support
is relevant; loopback use requires no TLS feature. The synchronous Read+Write API is portable
across supported Rust platforms. This investigation did not install or upgrade the dependency,
run a prototype, or qualify its transitive advisory status. No net production deletion is claimed.

# Sources

- [Registry metadata](https://crates.io/api/v1/crates/tungstenite), checked 2026-09-06.
- [Tagged framing source](https://raw.githubusercontent.com/snapview/tungstenite-rs/v0.30.0/src/protocol/frame/mod.rs).
- [Public FrameSocket API](https://docs.rs/tungstenite/latest/tungstenite/protocol/frame/struct.FrameSocket.html),
  reviewed with the 0.30.0 source.
- Beryl baseline e6172f7c49bebef78d51831e621e70c8ac0d6a07: backend websocket_transport.rs,
  websocket_transport/reader.rs and message.rs plus all backend ingress tests. See
  [BR-009](../../../../audits/code-simplification/reviews/backend-rest-findings.json).
