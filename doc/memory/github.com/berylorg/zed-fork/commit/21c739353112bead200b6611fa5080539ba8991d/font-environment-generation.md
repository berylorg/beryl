# Reason For Investigation

The indexed no-wrap Code layout review needed to establish whether GPUI exposes a font-environment
token covering resolved primary fonts and mutable fallback geometry. Beryl's mount currently compares
the requested `Font`, size, line height and window scale; its renderer authority also requires fallback
generation continuity.

# Outcome

No font-environment or fallback-generation API was found in the inspected GPUI text system. `Font`
equality includes family, shaping features, explicit fallbacks, weight and style. The existing public
`TextSystem::resolve_font(&Font) -> FontId` provides a resolved-primary identity, but does not identify
the collection used for subsequent fallback shaping.

Windows has a collection replacement path. `DirectWriteState::find_font_id` tries custom and system
collections, then calls `update_system_font_collection` if the family remains unresolved. That method
calls `GetSystemFontCollection(false, ..., true)` and replaces `system_font_collection` on success.
Beryl's theme editor accepts any nonempty font-family string and renders font previews, so resolving
a previously unseen missing family can reach this path without changing the Code font descriptor.
`DirectWriteState::layout_line` uses the current system collection when constructing a text format
and setting subsequent run collections, including when a run retains its earlier `FontId`.

The source therefore permits the collection used by later shaping to be replaced while Code's
current environment tuple and primary ID stay equal. The investigation did not install fonts,
exercise a live Windows font change, or establish how quickly DirectWrite observes OS changes.
An actual geometry change requires the effective font/fallback environment to differ. Beryl has no
`add_fonts`/`load_fonts` calls or explicit font-refresh control in the searched crates. Whether live
OS font changes belong to the supported envelope remains unresolved; source refresh capability and
observed OS font mutation must not be treated as the same evidence.

Both resolution and shape caches matter. `TextSystem::font_ids_by_font` and Windows
`font_selections` cache requested `Font` to `FontId`; neither key contains a generation. Ordinary
`LineLayoutCache` keys contain text, font size, `(run length, FontId)` runs, wrap width and forced
width, also without a generation. The inspected WIP canonical streaming text and atom paths call
`layout_line_uncached`, avoiding ordinary shape-cache retention, but they still resolve fonts through
the existing ID caches and use the platform collection.

If mutable environments are supported, a sufficient API would need an owner-scoped token for the
same shared text system used to shape, observation of effective collection/fallback changes, and
consistent invalidation or generation qualification of resolution and ordinary shape caches.
Checking only `FontId` or restarting the Code index cannot establish that qualification. A token must
remain valid across bounded layout quanta and publication. These are investigation constraints,
not a selected design or implementation sequence. Refresh this note when the platform observation
contract, font-loading integration, cache keys or generation API changes.

# Bounded Windows Observation Evidence

Microsoft's `IDWriteFactory::GetSystemFontCollection` contract says a true `checkForUpdates`
performs an immediate installed-font change check. A false value can still observe changes through
the font cache service with latency. The page does not establish that unchanged collections retain
COM identity, so pointer comparison alone is not qualified as semantic collection equality.

`IDWriteFontCollection3::GetExpirationEvent` provides a potentially bounded observation seam:
the system collection's event is signalled when installed, removed or updated fonts make that
collection obsolete. The event belongs to the collection, remains valid while its reference is
retained and must not be closed by the consumer. The documented minimum Windows client is build
17134. The inspected GPUI factory is `IDWriteFactory5`. Subsequent native qualification found that
its legacy collection supports the Collection3 cast but returns a null/invalid expiration handle.
Direct optional `IDWriteFactory7::GetSystemFontCollection` acquisition with the existing
weight/stretch/style family model returns an observable collection on the tested Windows process;
the retained event's zero-timeout wait returns `WAIT_TIMEOUT`. The
[collection-acquisition lesson](../../../../../../failures/indexed-code-font-collection.md) records the
failed approach and qualification boundary. A zero-timeout event check can gate
collection adoption without a complete inventory scan or unchanged missing-family refresh loop.
The owned fork's later working implementation and focused tests establish observed adoption,
cache/continuation invalidation and old glyph lifetime. Those results do not establish OS font-change
delivery timing. Unsupported event access must not be represented as a qualified generation.

# Sources

- Canonical remote: `https://github.com/berylorg/zed-fork.git`; inspected local HEAD:
  `21c739353112bead200b6611fa5080539ba8991d`, accessed 2026-10-11. Beryl's root `Cargo.toml`
  still requested GPUI revision `edd4928c5be424630da49f872e00dafbf94cf0b2` at inspection;
  this note identifies the local source snapshot examined, not a claim that the manifest resolved HEAD.
- [`crates/gpui/src/text_system.rs`](https://github.com/berylorg/zed-fork/blob/21c739353112bead200b6611fa5080539ba8991d/crates/gpui/src/text_system.rs):
  `Font`, `TextSystem::{font_id,resolve_font,add_fonts}` and
  `WindowTextSystem::layout_line_with_cache`; primary resolution identity and cache behavior.
- [`crates/gpui/src/platform/windows/direct_write.rs`](https://github.com/berylorg/zed-fork/blob/21c739353112bead200b6611fa5080539ba8991d/crates/gpui/src/platform/windows/direct_write.rs):
  `font_id`, `find_font_id`, `update_system_font_collection`, `add_fonts` and `layout_line`;
  collection replacement and collection selection during shaping.
- [`crates/gpui/src/text_system/line_layout.rs`](https://github.com/berylorg/zed-fork/blob/21c739353112bead200b6611fa5080539ba8991d/crates/gpui/src/text_system/line_layout.rs):
  `CacheKey`, `FontRun` and `LineLayoutCache::layout_line`; ordinary shape-cache keys.
- Local WIP files relative to the zed-fork root, SHA-256 at inspection:
  `crates/gpui/src/text_system/streaming_layout.rs`:
  `B8876D949A183ADA6D2B4127B98102EB6F3CBBDC53805E33AB5F19741DE9D390`;
  `crates/gpui/src/text_system/streaming_layout/session.rs`:
  `8A03518878F84C0E2208EBE327BF870552849FCB33569993779885F34ABABF47`;
  `crates/gpui/src/text_system/streaming_layout/inline.rs`:
  `C9234ADFC37E5155A0FE580507E88DF4CB53E4DADB884D89B05EC75ED8DA9C47`.
  The text and atom admission paths use uncached line layout.
- Beryl local use sites inspected: `crates/beryl-app/src/shell/settings/theme_editor/helpers.rs`
  (`FontFamily` validation and preview styling),
  `crates/beryl-app/src/shell/settings/theme_editor/navigator/row_render.rs` (font preview),
  `crates/beryl-app/src/shell/syndic_transcript/panel/code_appearance.rs`
  (`line_height` and prepared body font), and `panel/code_layout_mount.rs` in that directory
  (`Environment`). These are integration observations, not documentation authority.
- Reproduction commands: `git rev-parse HEAD`, `git remote get-url origin`, scoped
  `git status --short`, `Get-FileHash -Algorithm SHA256`, focused Serena symbol reads, and `rg`
  searches for font loading, collection mutation and generation/cache fields. Existing zed-fork
  memory siblings were listed before authoring. No runtime OS font experiment was conducted.
- Microsoft primary documentation, accessed 2026-10-12:
  [`IDWriteFactory::GetSystemFontCollection`](https://learn.microsoft.com/en-us/windows/win32/api/dwrite/nf-dwrite-idwritefactory-getsystemfontcollection)
  and [`IDWriteFontCollection3::GetExpirationEvent`](https://learn.microsoft.com/en-us/windows/win32/api/dwrite_3/nf-dwrite_3-idwritefontcollection3-getexpirationevent).
  These establish the documented refresh and event-lifetime contracts above, not actual event
  delivery timing. The direct acquisition contract is
  [`IDWriteFactory7::GetSystemFontCollection`](https://learn.microsoft.com/en-us/windows/win32/api/dwrite_3/nf-dwrite_3-idwritefactory7-getsystemfontcollection).
  Actual event access and forced-adoption verification are recorded in the collection-acquisition
  lesson and active root plan; no live OS font-install experiment was conducted.
