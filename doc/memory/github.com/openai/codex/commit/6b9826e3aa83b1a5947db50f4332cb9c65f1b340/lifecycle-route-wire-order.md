# Reason For Investigation

Determine whether current CAS permits route-first, one-pass lifecycle capture without retaining
potentially large content before its exact thread and turn are known.

# Outcome

The installed executable reports `codex-cli 0.154.0`, SHA256
`BE96B992178B1E467C225800DA0D65F2C86D5EBA1EF0B14632F65DB381CBDFDE`.
The matching official release source still serializes `ItemStartedNotification` and
`ItemCompletedNotification` with `item`, then `threadId`, then `turnId`, then the timestamp.
`ThreadItem::AgentMessage` contains a `String` text field; other items include aggregate command
output. These are potentially large payloads, not bounded routing headers.

`ServerNotification` uses the Serde method/params tagged representation. The flattened
`ServerNotificationEnvelope` adds an emission timestamp, not earlier routing. The transport's
`OutgoingMessage` directly contains this typed envelope, and `serialize_outgoing_message` calls
`serde_json::to_string` on it. No intervening route-first transformation appears in this path.

In contrast, `AgentMessageDeltaNotification` declares thread, turn and item IDs before its delta.
That permits earlier routing of that message kind but does not repair lifecycle ordering.

For lossless lifecycle capture that requires exact routing before delivering content, a one-pass
consumer must retain the preceding relevant payload somewhere until those trailing IDs arrive.
Reordering decoder callbacks cannot remove that storage requirement. Streaming unpublished
content to durable staging can bound RAM during healthy storage; it is still retained content and
cannot supply the same solution during store outage. Bounded discard can limit memory but loses
content. Thus this is not a claim that every CAS consumer inevitably needs unbounded RAM.

These are exact release-source findings plus installed binary identity, not a live 0.154.0 turn
probe or proof that the installed artifact is an unmodified build of that source. Implementation
was stopped under the Operator's instruction; no buffering or producer change was made.

# Sources

- Canonical repository: `https://github.com/openai/codex.git`; requested tag `rust-v0.154.0`;
  resolved commit `6b9826e3aa83b1a5947db50f4332cb9c65f1b340` using `git ls-remote`.
  Accessed 2026-09-17. Exact raw source inspected without checkout, build or installation.
- [Lifecycle, item and delta definitions](https://github.com/openai/codex/blob/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/codex-rs/app-server-protocol/src/protocol/v2/item.rs).
- [Notification tagging and envelope](https://github.com/openai/codex/blob/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/codex-rs/app-server-protocol/src/protocol/common.rs).
- [Typed outgoing message](https://github.com/openai/codex/blob/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/codex-rs/app-server-transport/src/outgoing_message.rs).
- [Direct transport serialization](https://github.com/openai/codex/blob/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/codex-rs/app-server-transport/src/transport/mod.rs).
