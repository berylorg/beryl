# Reason For Investigation

The CAS 0.137 transcript-history migration needed a version-specific history-loading contract that avoided older item-list assumptions and kept generated-image payload retention bounded in Beryl.

# Outcome

Useful as historical CAS 0.137 protocol evidence. At investigation time, Beryl used `thread/turns/list` with `itemsView`. That live transcript-history use is superseded by the CAS-live Syndic transcript rework: selected transcript history now targets CAS live-event capture into Syndic and storage-backed Syndic projection reads. `itemsView = "notLoaded"` was the cheapest non-rendered turn-index view and returned turn identity, status, and page metadata with empty `items`. `itemsView = "full"` returned full turn items and was the bounded resident-detail path before the rework. `itemsView = "summary"` was not sufficient for generated-image transcript rendering because it omitted native `imageGeneration.savedPath`.

Generated stable 0.137 schemas did not expose `thread/turns/list` or `thread/turns/items/list`; generated experimental schemas exposed both methods. Live 0.137 probing showed `thread/turns/items/list` can still return JSON-RPC `-32601` with message `thread/turns/items/list is not supported yet`, so Beryl must not use that method in this target contract.

If a `notLoaded` turn-index page returned item vectors despite the request, Beryl discarded those item vectors before retaining the index page so whole-page detail data could not bypass transcript residency policy. The bounded full-page detail path preserved Beryl's app-side retention bounds by sanitizing the response stream, retaining only detail admitted by transcript residency policy or explicit pins, and releasing detail outside the resident full-detail window. It could not prevent app-server itself from assembling and sending generated-image result bytes for the requested full page. This mechanism is archived reference material, not live target architecture.

# Sources

- Legacy note: `doc/app-server-contract.md`, migrated on 2026-06-11.
- Local codex-cli 0.137.0 generated stable app-server schema from `codex app-server generate-json-schema --out <temp-dir>`, accessed 2026-06-08.
- Local codex-cli 0.137.0 generated experimental app-server schema from `codex app-server generate-json-schema --experimental --out <temp-dir>`, accessed 2026-06-08.
- Live codex-cli 0.137.0 stdio probes with `capabilities.experimentalApi = true` on the existing `City Image Generation` thread, performed 2026-06-08.

# Historical Local Integration Impact

- Transcript index and cursor planning used `itemsView = "notLoaded"`.
- Resident full-detail transcript windows used bounded `itemsView = "full"` page requests.
- Composer image-label cache validation preferred `itemsView = "notLoaded"` so generated-image bytes were not loaded just to validate history frontier metadata.
- Beryl's archived history sanitizer stripped generated-image `result` fields when a `savedPath` was present, allowing source-backed rendering without retaining base64 image bytes in typed app state.
