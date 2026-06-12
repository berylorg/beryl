# Transcript responsiveness

## Failed approach

- Subphase 9.1 removed transcript-size scans from render-time profiling and stopped deep-copying visible turn records during transcript row rendering.
- That reduced one class of transcript hot-path work, but live testing still showed whole-window hover lag in opened workspaces with large transcripts.
- The invalid assumption was that fixing render-time transcript summaries alone would remove the dominant user-visible lag.
- In practice, ordinary hover-state changes in the shared workspace shell still invalidate the same `ShellView` that owns toolbar, composer, and transcript rendering together, so unrelated hover repaints still rebuild visible transcript rows.

## Course adjustment

- Treat hover-driven shell invalidation and transcript rendering ownership as the next responsiveness boundary problem.
- Move the transcript surface into its own child view/entity so toolbar and other shell hover repaints do not force transcript row reconstruction when transcript data is unchanged.
- Keep transcript-owned list state and transcript snapshots synchronized explicitly across history loads, streamed updates, and blocked-surface transitions after the view split.

## Follow-up failure

- Live testing after the child-view split showed no material improvement in lag.
- The invalid assumption in that follow-up was that embedding the transcript as a plain `gpui` child `Entity<V>` would itself create a cached subtree boundary.
- In the local `gpui` fork, `Entity<V>` as an element still calls `render()` during layout, while `AnyView::cached(...)` is the actual cached-view path.

## Follow-up course adjustment

- Keep the transcript as a dedicated child view, but embed it through `gpui`'s cached-view mechanism instead of plain `Entity<V>` element rendering.
- Continue driving transcript invalidation through explicit transcript-owned notifications so the cached subtree is reused only when transcript data and layout bounds are unchanged.

## Submit-anchor spacer failure

- Phase 26 initially used bottom padding equal to the transcript viewport height to create enough trailing scroll space for a newly submitted prompt's last rendered line to anchor at the top.
- Live testing showed the transcript viewport went blank after submission until manual scrolling released the submit anchor.
- The invalid assumption was that full-viewport bottom padding is harmless spacer content in a `gpui` `List`.
- In the local `gpui` fork, list prepaint clears visible items when vertical padding consumes the whole viewport, so the anchored list had content above the scroll position but painted no rows.

## Submit-anchor course adjustment

- Represent submit-anchor trailing slack as a synthetic list row after the real transcript turns instead of as list bottom padding.
- Keep the spacer row strictly below the visible transcript height and let the active submit anchor scroll to the real turn row, so `gpui` has concrete list children to paint while still leaving enough trailing scroll room for the prompt line to sit at the top.

## Submit-anchor scroll-handler failure

- Phase 26 initially released the submit anchor synchronously from the transcript `ListState` scroll handler when the user manually scrolled an overflowing response.
- Live testing showed a `RefCell already borrowed` panic inside `gpui` list splicing.
- The invalid assumption was that list item-count changes are safe inside a `ListState` scroll callback.
- In the local `gpui` fork, the list invokes its scroll callback while `StateInner::scroll` still owns the mutable list-state borrow, so releasing the submit anchor there reentered `ListState::splice` during that active borrow.

## Submit-anchor scroll-handler course adjustment

- Keep immediate scrollbar-activity recording in the scroll handler, but defer submit-anchor release until the current `gpui` effect cycle unwinds.
- Manual scroll must release only forced submit-time positioning, not the scroll allowance itself.
- Keep any synthetic spacer row while response content below the submitted prompt's last rendered line is too short to let that line scroll to the top naturally, and shrink or remove that spacer only as real response content makes the allowance unnecessary.

## Loaded-history spacer gap

- Phase 26 first applied the synthetic spacer only to a prompt accepted during the current GUI session.
- Live inspection showed startup-loaded thread history could not scroll beyond the last rendered transcript line, so the latest historical prompt could not be brought to the top when its response tail was shorter than the viewport.
- The course adjustment is to load existing thread history at the real transcript end first, then install a passive latest-prompt anchor after the initial paint so the spacer can exist without making startup open into blank trailing space.

## Synthetic spacer row failure

- Subsequent live testing showed that manually scrolling to the live bottom while response content was streaming could briefly blank the transcript viewport until the response completed.
- The likely invalid assumption was that a synthetic list row could safely represent empty trailing scroll allowance while live content rows are repeatedly remeasured.
- A synthetic spacer row leaks empty scroll geometry into the list content model, so scroll preservation and bottom-range calculations can treat blank space as a durable item position.

## Synthetic spacer row course adjustment

- Replace transcript-owned synthetic spacer rows with reusable virtual trailing scroll geometry owned outside transcript content.
- Keep content rows, visible item ranges, and durable content anchors real-content-only while scrollbar range and max scroll position include bounded virtual trailing allowance.
- Preserve live scroll position by explicit intent, so bottom-following, content-anchored, and virtual-tail positions remain distinguishable during streaming remeasurement.

## Third-party list ownership failure

- Phase 2 first attempted to add virtual trailing scroll allowance by modifying the local `gpui` fork's list primitive.
- The invalid assumption was that the local fork was the right ownership boundary merely because Beryl depends on it.
- In practice, this pushed Beryl-specific scroll behavior into third-party code and made verification depend on `gpui` fork test wiring, including a stale `reqwest_client` dev-dependency path that still pulls TLS/native crypto dependencies.

## Third-party list ownership course adjustment

- Keep the `gpui` fork as a dependency boundary unless a separate operator-approved reason requires changing it.
- Own the virtual trailing list primitive inside `beryl-app`, using `gpui` public element APIs and copied list mechanics where necessary.
- Verify virtual trailing scroll behavior through Beryl-owned primitive and transcript integration tests rather than fork-level `gpui` tests.

## Virtual trailing remeasurement failure

- Live testing of the Beryl-owned virtual list showed the transcript could flicker, jerk, or briefly paint blank while Markdown streamed and the user manually scrolled.
- The invalid assumption was that same-row streaming remeasurement could reuse `ListState::splice` safely.
- In practice, replacing the live row with an unmeasured zero-height row let scroll math observe collapsed content height between the stream update and the next prepaint.
- A second invalid assumption was that the full virtual trailing allowance could be counted as layout-fill height for every non-bottom scroll position.
- That let near-viewport allowance make the list consider the viewport filled by empty virtual space before enough real rows were backfilled.

## Virtual trailing remeasurement course adjustment

- Do not splice same-count live transcript updates; keep the previous measured row height until the visible row is naturally remeasured by prepaint.
- Count only the currently visible portion of virtual trailing allowance during list layout fill calculations.
- Preserve manual virtual-tail intent as a non-following content position when the allowance shrinks to zero.
- Release forced submit anchoring synchronously on the first transcript scroll now that release no longer mutates list item counts.

## Code-selection hit scan regression

- Phase 6 extended selectable transcript text from prose into code panels, which increased the number of visible selectable geometries when code blocks or command output were on screen.
- Live testing showed drag selection could feel sluggish again.
- The invalid assumption was that the Phase 5 batched registration fix was sufficient after the selectable geometry count increased.
- In practice, pointer hit testing still scanned the visible geometry list linearly on every drag mouse-move, so code-panel line registration reintroduced avoidable hot-path work.

## Code-selection hit scan course adjustment

- Keep code-panel selection enabled, but sort hit geometries by vertical position after each registration frame.
- On pointer movement, narrow hit testing to the vertical candidate range before consulting text layout offsets.
- Preserve selection ordering separately in `VisibleTranscriptTextFrame`, so copied text and quote harvesting still follow transcript order rather than geometry index order.

## Generated-image diagnostic child freeze

- Phase 6 attempted a live generated-image verification with a debug diagnostic child launched from `target\debug\beryl.exe` against an isolated copied Beryl home.
- The child selected the `City Image Generation` thread and diagnostics showed ten visible source-backed generated images.
- A same-setup reproduction on June 11, 2026 confirmed Windows became barely responsive until the diagnostic child was closed. Task Manager data stopped updating, desktop responsiveness degraded, and audio playback struggled.
- The sampled frame metrics included repeated generated-image frames with very large media-preload timing, including hundreds of milliseconds and an approximately 1.4 second single-frame sample, while renderer diagnostics showed active source-backed image decode/upload/live state.
- The invalid assumption was that the existing generated-image fixture could be used as an ordinary bounded Phase 6 live diagnostic workload in a debug child.

## Generated-image diagnostic child course adjustment

- Do not repeat generated-image live verification through an unconstrained debug diagnostic child.
- Before retrying, add or use stricter guardrails: prefer a release child, avoid parallel diagnostic reads while image preload is active, stop after a small bounded sample, and abort on the first sustained media-preload spike or shell-response timeout.
- Treat generated-image Phase 6 verification as unresolved until the media-preload/render loop is investigated without risking operator-machine responsiveness.

## Staged Activation Admission Retry Deadlock

- Scope: selected-thread activation publication after bounded transcript prepublication and media admission were introduced.
- Invalid assumption: notifying the shell after one staged admission drain was enough to keep activation publication moving.
- Evidence: live testing on June 12, 2026 showed switching from a blank New Thread screen fetched 36 transport turns, then left the GUI on the blank New Thread screen indefinitely with no CPU activity.
- Why it failed: a bounded drain can make progress but still leave staged activation unpublished. Retrying from shell notification alone does not reliably re-render the cached transcript panel child. Media admission also treated a row-budget flag as unpublishable even when the unprocessed rows had no required media, so long plain-text histories could remain staged forever.
- Course correction: staged admission now returns an explicit retry signal that notifies the `TranscriptPanel` itself. Media admission settlement depends on unresolved required media, while budget flags only request another pass when required media remains pending.
- Affected tests: keep `media_admission_budget_exhaustion_requests_retry`, `prepublication_preparation_accumulates_bounded_rows_for_layout`, and the staged-admission source guards in `conversation_shell_source`.

## Media-admission retry cursor failure

- Scope: staged selected-thread activation and staged residency pages that require multiple bounded completed-media admission passes.
- Invalid assumption: once a bounded media-admission pass reported `requires_retry`, rerunning the same request would eventually reach later rows.
- Evidence: completion review on June 12, 2026 found that each retry rebuilt the request from the full row list and the drain restarted at row zero. If required media began after the row budget, the same prefix could be rescanned forever.
- Why it failed: retry state was modeled as a boolean, not as scan progress. The summary also needed to distinguish media that was deferred because it had not been scanned from pending media already seen in earlier rows.
- Course correction: media-admission requests now carry row and item scan starts plus an explicit prefix-recheck flag. Summaries report scanned rows, current-row scanned media items, deferred completed-media items, and whether a prefix recheck is required. The staged window advances after row/time exhaustion, resumes inside a heavy row after media-budget exhaustion, forces a full pass from row zero after suffix scanning when earlier pending media may have changed readiness, and waits for media readiness instead of spinning if that prefix is still pending.
- Affected tests: keep `media_admission_retry_request_advances_after_row_budget_exhaustion`, `media_admission_retry_rechecks_pending_prefix_after_suffix_scan`, and `media_admission_retry_resumes_inside_row_after_media_budget_exhaustion`.

## Staged-admission post-summary retry failure

- Scope: staged selected-thread activation and staged residency page publication after completed-media admission updates the staged window.
- Invalid assumption: a raw drain summary could decide whether the transcript panel should self-notify for another admission pass before the staged admission window incorporated that summary.
- Evidence: completion review on June 12, 2026 found that full-prefix rechecks with still-pending prefix media would keep requesting immediate retries even after the staged window correctly transitioned into a wait-for-media state. The same review found that a source-backed image whose requested upload bytes exceeded the entire per-drain upload budget could remain the current retry item forever.
- Why it failed: retry intent is derived state owned by the staged media-admission window, not by the raw drain result. Source-backed admission also needed a terminal fallback path for items that can never fit in one drain budget.
- Course correction: the transcript panel now asks the staged window for retry intent after writing back the admission summary. Source-backed completed media that exceeds the whole upload budget is counted as terminal fallback for admission instead of being retried, while ordinary row-budget and item-budget exhaustion still request retry passes.
- Affected tests: keep `media_admission_retry_rechecks_pending_prefix_after_suffix_scan`, `media_admission_retry_current_item_after_media_budget_exhaustion_in_later_row`, and the completed-media admission source guards in `conversation_shell_source`.

## Prepublication pre-writeback retry failure

- Scope: staged selected-thread activation and staged residency page publication after prepublication preparation updates the staged window.
- Invalid assumption: fixing completed-media admission retry derivation was sufficient because prepublication preparation had a simpler boolean acceptance path.
- Evidence: completion review on June 12, 2026 found that `TranscriptPanel` still computed prepublication retry from the raw drain before the staged target accepted the summary. A stale drain for a replaced staged target could therefore schedule a transcript-panel self-notify even though no staged state changed.
- Why it failed: prepublication retry is derived from the staged preparation window in the same way media-admission retry is derived from the staged media window.
- Course correction: prepublication summary acceptance now returns the accepted stored summary, and the transcript panel computes `preparation_requires_retry` only after the matching staged activation or residency page writes that summary back.
- Affected tests: keep `prepublication_preparation_is_bounded_and_staged_before_publication` source guards for accepted-summary return and post-acceptance retry ordering.

## Activation Worker Historical Image Import Stall

- Scope: selected-thread activation after the initial transcript transport page has loaded.
- Invalid assumption: preparing a full `TranscriptImagePathResolver` inside the activation worker was cheap enough to do before handing the activation result back to the UI.
- Evidence: live testing on June 12, 2026 showed thread switching from a blank New Thread screen logged only `Fetched transcript transport page` for a 42-turn page and then left the GUI blank for minutes.
- Why it failed: `transcript_image_path_resolver_for_turns` may synchronously import unresolved historical local-image paths, including backend `fs/readFile` calls, before the worker sends `ThreadActivationOutcome::Activated`. A slow or missing historical image path can therefore block the staged activation pipeline before the transcript panel can run bounded admission.
- Course correction: selected-thread activation and selected-thread startup restore now use an assets-only resolver before UI publication. Already-retained workspace image assets still resolve, while unresolved historical image import must not block activation publication.
- Affected tests: keep `selected_thread_activation_worker_uses_assets_only_image_resolver_before_ui_result` and `workspace_asset_resolver_does_not_import_historical_paths`.

## Transcript list row reentrant state read

- Scope: transcript row rendering during selected-thread activation and residency release.
- Invalid assumption: a transcript row renderer could query `ListState` for `logical_scroll_top()` and `viewport_bounds()` while the virtual list was rendering rows.
- Evidence: live testing on June 12, 2026 showed activation of the recent `Implement plan.md` thread admitted 16 resident turns, planned a budget shrink to one desired turn, released 15 turns, and then panicked at `crates\beryl-app\src\shell\virtual_list\state.rs:177:16` with `RefCell already mutably borrowed`.
- Why it failed: the Beryl-owned virtual list calls the row closure while holding a mutable borrow of `StateInner` for layout and measurement. Reading `ListState` inside that closure nests a borrow of the same `RefCell`.
- Course correction: the Beryl-owned virtual list now passes a layout-owned row viewport context into each row renderer, including row offset and viewport height. Transcript rows consume that context instead of reading `ListState`, and list layout refreshes rows whose final top offset differs from a provisional render before prepaint.
- Affected tests: keep `transcript_row_renderer_uses_layout_context_without_reentrant_list_state_reads` and `list_row_render_context_is_layout_owned_and_refreshed_after_bottom_up_fill`.
