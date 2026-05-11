# CAS Turn List Latency

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

Likely product fixes involve a payload-light or paginated turn-history API that can list turns without inline generated-image payloads, or an app-server change that avoids assembling those payloads on activation. While Beryl remains on CAS 0.128.0 and must request the full turn list, Beryl can mitigate perceived delay with immediate pending UI and lazy media behavior, but it cannot remove the measured backend first-byte stall from that request path.
