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
