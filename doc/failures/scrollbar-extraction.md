# Scrollbar Extraction

## 2026-05-15: Local patch cannot validate an unpublished git revision

During Phase 5, Beryl was checked in a temporary worktree with `gpui-scrollbar` declared as the intended final git dependency:

`gpui-scrollbar = { git = "https://github.com/berylorg/gpui-scrollbar.git", rev = "ff660f59c1cf3bee8ca4cf462b23c92714f7d0de" }`

A local `.cargo/config.toml` `[patch."https://github.com/berylorg/gpui-scrollbar.git"]` override pointing at `../gpui-scrollbar` did not let Cargo resolve the unpublished revision. Cargo still attempted to fetch the remote git source and failed because the revision was not available remotely.

Cargo `paths = ["../gpui-scrollbar"]` also did not avoid that fetch for the unpublished git dependency.

The useful pre-push validation path is a temporary registry-style dependency patched through local config:

`gpui-scrollbar = "0.1.0"` plus `[patch.crates-io] gpui-scrollbar = { path = "../gpui-scrollbar" }`.

That check compiled `beryl-app` against the local reusable crate and validated GPUI compatibility, but it is not equivalent to validating the final immutable git pin. The final pinned git dependency can only be verified after the reusable crate commit is available from the remote.

## 2026-05-15: Optional caller-owned fade state made scrollbar reuse too easy to misuse

During local live testing, Beryl's settings window used the extracted scrollbar thumb and direct-manipulation behavior but did not fade the scrollbar in and out. The settings-window integration had passed a constant opacity and no activity callback, which produced a permanently visible scrollbar whenever the surface overflowed.

That result exposed an architectural problem in the first reusable API shape: fade/activity was treated as optional caller-owned wiring even though it is part of the shared scrollbar affordance expected across Beryl and reusable GPUI crates. The course correction is to move app-neutral managed visibility and fade behavior into `gpui-scrollbar`.

The reusable scrollbar crate still must not own scroll offsets, wheel routing, keyboard routing, focus, nested-scroll routing, or application-specific edge rules. Callers store per-scroll-region scrollbar state and report viewport activity into it, while `gpui-scrollbar` derives the scrollbar chrome visibility and opacity from an explicit visibility policy. Non-fading scrollbars should require an explicit API choice rather than falling out of omitted fade wiring.

## 2026-05-15: Synchronous visibility repaint callbacks re-entered GPUI owners

During the `gpui-text-input` managed-visibility migration, focused tests failed with GPUI's "cannot update while it is already being updated" guard. The text-input viewport reported scrollbar activity from inside its own pointer and wheel event handlers, and `gpui-scrollbar` synchronously invoked the managed-visibility repaint callback. That callback tried to update the same text-input entity before the current update had returned.

The invalid assumption was that repaint callbacks can be invoked synchronously from managed visibility state changes. In GPUI, viewport-originated activity is commonly reported while the viewport owner is already on the update stack.

The course correction is for `gpui-scrollbar` to defer managed visibility repaint callbacks through the GPUI effect cycle. Callers may report viewport activity from their own event handlers, while async fade timers and direct scrollbar interactions still use the same owner repaint callback without re-entering an active owner update.

## 2026-05-15: Timer-only managed fade produced choppy and instant-looking animation

During local live testing after the managed visibility extraction, all expected Beryl-reachable scroll surfaces used the shared scrollbar widget, but the fade quality regressed. Fade-in appeared coarse and choppy, and fade-out appeared instant compared with the earlier Beryl main-shell implementation.

The invalid assumption was that continuously computed opacity plus timers at lifecycle boundaries would be enough for smooth visual fade. The opacity math was continuous, but `gpui-scrollbar` only repainted on activity, the idle-delay wakeup, and the end-of-transition wakeup. Without a presentation-frame repaint loop, intermediate opacity values were not presented reliably.

The course correction is for `gpui-scrollbar` to own presentation-rate fade driving as part of reusable scrollbar chrome. Managed fade transitions request `gpui` animation frames from the scrollbar render path while opacity is changing. Timers may still wake idle-delay and cleanup boundaries, but they must not be the only mechanism that advances visible fade frames.
