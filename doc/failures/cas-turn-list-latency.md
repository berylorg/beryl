# CAS Turn List Latency

## Superseded Context

This note records measurements from the older CAS 0.128.0 transcript-loading path. The current Beryl target contract is CAS 0.137 with `thread/turns/list itemsView`; see `doc/design.md` and `doc/app-server-contract.md` for current target authority.

## 2026-05-10: Large Generated-Image Thread First-Byte Stall

Live measurements on the Happy Sunny Images thread showed that opening a thread with seven generated images remains slow in a Cargo release build even after Beryl removes inline image-result payloads from the final parsed response.

The invalid assumption was that the dominant cost was reading, parsing, or discarding roughly 24 MB of response data in Beryl. The release/debug comparison showed otherwise.

Evidence:

- `release.log:810-813` measured a `thread/turns/list` response of `24,737,979` bytes, sanitized to `8,163` bytes with seven image results removed. Total typed request time was `2,266.8 ms`.
- `debug.log:812-815` measured the same response size and sanitized size. Total typed request time was `4,590.1 ms`.
- The first backend payload wait was effectively unchanged between builds: `2,095.2 ms` in release and `2,104.3 ms` in debug.
- The post-first-payload reader wait was small: `10.6 ms` in release and `12.5 ms` in debug.
- The sanitizer path was build-sensitive: `171.3 ms` in release and `2,484.9 ms` in debug.
- Typed deserialization after sanitization was negligible: `0.071 ms` in release.

The failure mode is therefore not pipe throughput. Beryl spends about two seconds waiting for CAS/app-server to begin returning `thread/turns/list` for this large generated-image transcript. That delay occurs before Beryl's streaming sanitizer can reduce the response.

Secondary measurements showed seven later `fs/readFile` responses for generated image media totaling roughly `24.7 MB`, each arriving in about `292-403 ms` in `release.log:836-842`. Those media reads can make the UI continue to feel unfinished after the text response lands, but they do not explain the initial `thread/turns/list` first-byte stall.

## Course Adjustment

Treat large generated-image thread activation on CAS 0.128.0 as a backend/CAS first-byte latency problem unless later backend-side timing disproves it.

Future work should measure:

- backend-side timing inside app-server/CAS before the first `thread/turns/list` response byte is written
- UI activation timing from click/selection through pending state, sanitized response application, first transcript paint, media request start, image decode, and final image paint

Likely product fixes involve a payload-light or paginated turn-history API that can list turns without inline generated-image payloads, or an app-server change that avoids assembling those payloads on activation. In the measured CAS 0.128.0 path, Beryl could mitigate perceived delay with immediate pending UI and lazy media behavior, but it could not remove the measured backend first-byte stall from that request path.

## CAS 0.137: Retention Windows Must Not Become Request Priority

During the CAS 0.137 transcript-detail scheduler migration, live Phase 6 validation on the long `Codex Schema Migration` thread invalidated a plausible scheduler shape: using the visible row as request priority while also appending the retained overscan window to the request-priority list.

Evidence:

- A release diagnostic child opened the long thread at the tail after `City Image Generation`.
- The tail became a single full latest row, but retained-state diagnostics showed the detail request ring climbing from 3 to more than 120 requests without user scrolling further.
- Each request retained only a tiny window and released prior detail, so app memory stayed bounded, but CAS still did repeated `thread/turns/list itemsView = "full"` work for rows that were not useful to the current viewport.

The invalid assumption was that bounded retention implies bounded request work. Retention is a memory policy; request priority is a latency policy. Promoting retained overscan into request priority can create a detail-load train even when retained full rows remain bounded.

Course adjustment:

- The old priority-only detail scheduler is superseded by transcript residency policy.
- Keep request priority, retention, scroll-boundary extension, byte budgets, turn budgets, and in-flight limits as explicit policy inputs.
- Do not request retained margin rows merely because they are within the desired resident runway; policy budgets and current user intent still control admission.
- When the operator scrolls beyond the resident boundary, clamp at the resident edge and let the transcript residency controller create the next policy-allowed request before extending scrollable content.

The remaining CAS 0.137 cost is prefix-page detail loading. A jump to the oldest row in the same thread requested `limit = 63` and CAS returned 63 full turns, while Beryl applied and retained only one visible row. That preserves Beryl memory bounds but cannot avoid CAS response work until the app-server exposes a true per-turn detail API.
