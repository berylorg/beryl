# Reason For Investigation

Context-status mounting needs release-pinned usage provenance and quota identity, rather than
assuming that Beryl's older metadata type constitutes a live observation route.

# Outcome

At Codex App Server 0.146.0, `ThreadTokenUsageUpdatedNotification` serializes `threadId`, `turnId`,
then `tokenUsage`. The usage object serializes `total`, `last`, then nullable `modelContextWindow`.
Each breakdown uses signed 64-bit counters and includes `cacheWriteInputTokens` in addition to
total, input, cached input, output and reasoning output. The cache-write field defaults when
deserializing; it is emitted by the current serializer. The context window is optional, so even
an authentic notification need not support a context percentage.

`handle_token_count_event` forwards supplied core usage without estimating it. When one event
contains both usage and rate limits, it awaits the thread-usage send before the quota send. The
quota notification itself carries only `rateLimits`, with no thread or model identity.

`RateLimitSnapshot` carries nullable `limitId`, `limitName`, `primary` and `secondary` and other
account fields. Its windows carry `usedPercent` as an `i32`, nullable `windowDurationMins` and
nullable `resetsAt` as `i64`. Core floating-point usage is rounded during conversion to the public
integer percentage. Field types alone do not prove that a value is within Beryl's display domain.

The account read response also offers a nullable `rateLimitsByLimitId` map. The protocol describes
its keys as metered limit IDs, with `codex` as an example. Neither that description nor a display
name proves equality with a selected model's identity. No model-to-metering-bucket mapping was
established by this investigation. A consumer requiring exact model identity must preserve
unavailability for unmatched buckets rather than infer a mapping.

The transport envelope also matters: `send_server_notification_to_connections` constructs a
timestamped `ServerNotificationEnvelope` for targeted and broadcast notifications. Its serialized
top-level `emittedAtMs` follows `params`; the producer sets it to an integer millisecond timestamp.
The envelope's serialization assertion confirms this wire shape. Consumers must consume the
optional field incrementally rather than reject authentic notifications after their params object.
Inspected the pinned outgoing sender on 2026-10-10 during implementation review.

This inspection establishes a stream producer, serialized field order and public numeric shapes.
It does not qualify a read-only latest-usage response, quota-window classification, a reconnect
cache, or source changes beyond this pinned release.

# Sources

- Repository: [OpenAI Codex](https://github.com/openai/codex), requested release `rust-v0.146.0`,
  resolved commit `e363b08c9175ac1cbe5893615dd2cb9ddf95043b`, previously qualified by the sibling
  [release investigation](native-spawn-and-thread-response-0.146.0.md). Accessed 2026-10-10.
- [Outgoing envelope](https://github.com/openai/codex/blob/e363b08c9175ac1cbe5893615dd2cb9ddf95043b/codex-rs/app-server/src/outgoing_message.rs#L543-L581):
  timestamped notification construction at lines 677–681 and serialization assertion at 715–739.
- [Thread protocol](https://github.com/openai/codex/blob/e363b08c9175ac1cbe5893615dd2cb9ddf95043b/codex-rs/app-server-protocol/src/protocol/v2/thread.rs):
  usage notification, usage/breakdown declarations and core conversion.
- [Account protocol](https://github.com/openai/codex/blob/e363b08c9175ac1cbe5893615dd2cb9ddf95043b/codex-rs/app-server-protocol/src/protocol/v2/account.rs):
  rate-limit response, notification, snapshot and window declarations/conversions.
- [Event handling](https://github.com/openai/codex/blob/e363b08c9175ac1cbe5893615dd2cb9ddf95043b/codex-rs/app-server/src/bespoke_event_handling.rs):
  `handle_token_count_event`. Inspected exact raw files over HTTPS, without an upstream checkout
  or package installation.
- [Wire methods](https://github.com/openai/codex/blob/e363b08c9175ac1cbe5893615dd2cb9ddf95043b/codex-rs/app-server-protocol/src/protocol/common.rs):
  exact notification method mapping for usage and account rate limits.
