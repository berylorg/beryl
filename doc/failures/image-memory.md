# Image Memory Investigation

## Invalid Preload Size: Outer Tile Instead Of Image Content

Phase 11 live testing showed that Beryl transcript preloads initially requested the outer media tile size, while GPUI visible `img()` rendering requested the inner image content box after the tile border had been applied by layout.

The symptom was duplicate source-backed resources in the City Image Generation thread: preload requests were about `629 x 419` device pixels, while visible final-scene requests for the same sources were about `625 x 416` or `626 x 416`.

Course adjustment: Beryl preload sizing must use the same image content box as the visible `img()` child, not the decorated media tile bounds. The implementation now subtracts the media tile border before converting the preload size to device pixels.

## Invalid Request Identity Assumption: Exact Pixels Are Too Fine

After correcting the media content size, City live testing still showed first-frame duplicate resources for some images. GPUI/Taffy snapped equivalent visible children to neighboring device-pixel widths, so a preload request at `626 x 416` and a visible request at `625 x 416` could be queued in the same frame before either resource existed.

A live-resource reuse check was not enough because the duplicate decode/upload work was already scheduled by the time any resource became live.

Course adjustment: GPUI source-backed image request identity must absorb subpixel layout jitter before decode/upload work is queued. The implementation now canonicalizes source-backed request dimensions to an even device-pixel bucket and can also reuse an already-live same-source resource within a small device-pixel tolerance.

## Invalid Preload Filter Placement: Filtering After Media Lookup Is Too Late

The Phase 11 preload implementation initially filtered to loaded source-backed file images inside `preload_media_item`, after `TranscriptMediaRenderContext::media_for` had already looked up each media item. That meant Markdown images or inline byte-backed generated-image fallbacks in the preload band could schedule backend file reads or retained-byte media-cache work even though overscan preload is meant to prepare only source-backed GPU resources.

Course adjustment: preload runs now return before media lookup unless every item in the run is a native generated image with a non-empty `savedPath`. Visible rendering still uses the ordinary media-cache path for Markdown and byte-backed media, but overscan preloading cannot wake those loads solely because they are near the viewport.

## Invalid Detail Scheduler Gate: Startup Capability Reports Are Not A Row State

During the CAS 0.137 transcript-history migration, live testing on the `City Image Generation` thread showed Beryl rendering a visible `Loading transcript details...` skeleton row while retained-state diagnostics reported `transcriptMissingDetailTurns = 1`, `transcriptDetailRetentionTurns = 0`, `transcriptDetailLastRequestedTurns = 0`, and `transcriptDetailPendingRequests = 0`.

The invalid assumption was that the old transcript detail scheduler could be gated only on the backend capability report stored in the ready shell state. That report was not the same authority as the selected transcript row state. Under the superseded visible-skeleton architecture, once Beryl had a visible non-full turn, the UI had to either request full details for that row or mark the row as failed; leaving the scheduler disabled produced a permanent loading placeholder.

Evidence:

- A diagnostic child using `target\debug\beryl.exe` and local `codex-cli 0.137.0` opened `City Image Generation` as thread `019e1e41-2c86-7e23-a28a-034bfa9032f2`.
- `beryl_diagnostic.read_retained_state` showed one skeleton turn, one missing detail turn, and zero detail retention or pending requests.
- Historical schema inspection with `codex app-server generate-json-schema --experimental --out <temp>` showed `thread/turns/list` defaults `itemsView` to `summary`, and `thread/turns/items/list` is the experimental per-turn detail method that Beryl later stopped using because local CAS 0.137 reports it as runtime unsupported.

Course adjustment at the time: drive detail scheduling from the visible transcript range and actual cached skeleton/full row state.

This was later superseded first by the CAS 0.137 single-contract invariant and then by the resident-window transcript design. That intermediate correction removed the schema-exposed per-turn item-list method and used bounded `thread/turns/list itemsView = "full"` requests admitted by transcript residency policy. The CAS-live Syndic transcript rework has since superseded that CAS-history path as live selected-transcript architecture.

## Schema-Exposed Item Detail Method Can Still Be Runtime-Unsupported

CAS 0.137 schema inspection showed `thread/turns/items/list` in the experimental generated schema, but a live stdio probe against local `codex-cli 0.137.0` returned JSON-RPC `-32601` with message `thread/turns/items/list is not supported yet` for the `City Image Generation` thread and visible turn.

The invalid assumption was that schema presence plus experimental initialization meant the per-turn item-list method was usable. In this build, `thread/turns/list` with `itemsView = "full"` succeeds for the same thread, but that loads full item payloads at page granularity rather than per visible turn.

Course adjustment at the time: do not silently replace per-turn detail loading with full turn-page loading without operator approval. The operator approved an explicit CAS 0.137 workaround that retried unsupported per-turn detail requests through `thread/turns/list itemsView = "full"` with the skeleton page cursor and the smallest page prefix that can include the visible turn.

This was later superseded by the root design invariant for the 0.137 migration and by the resident-window transcript design. That path is now superseded again by the CAS-live Syndic transcript rework: Beryl's live target no longer uses CAS historical transcript reads or the archived streaming sanitizer for selected transcript history.

## Visible Transcript Rows Are Not Always History Skeletons

During the superseded CAS 0.137 item-list attempt, submitting a new turn in the `Codex Schema Migration` thread opened a popup saying `thread/turns/items/list is not supported yet; the skeleton row did not record a full-page fallback cursor`.

The invalid assumption was that every visible row requiring detail scheduling had been inserted into `TranscriptTurnDetailCache` as a history skeleton. Live rows created by `begin_turn` are visible transcript rows, but they are not history skeletons; their items arrive through stream state. The unsupported per-turn item-list method was later removed from Beryl's current CAS 0.137 contract.

Evidence:

- The operator reproduced the popup in a release build immediately after submitting a new user input.
- A diagnostic child using the patched debug build submitted a copied-home smoke-test turn and retained-state diagnostics reported `transcriptDetailLastRequestedTurns = 0`, `transcriptDetailPendingRequests = 0`, and no popup.
- `turn_detail_scheduler_ignores_required_turns_without_history_skeleton` covers the missing-skeleton case.

Course adjustment at the time: the history-detail scheduler requested details only for known skeleton entries whose `itemsView` was not full. Unknown live row ids were ignored by the history-detail path rather than converted into cursorless history-page detail tickets.

Current correction: the history-detail scheduler model is superseded. Live rows continue to receive items through stream state, while historical rows become user-visible only after the transcript residency controller admits resident full-detail turn data.
