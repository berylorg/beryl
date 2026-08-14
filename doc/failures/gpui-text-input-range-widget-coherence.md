# Scope

The superseded range-backed GPUI widget-integration phase in the owned `gpui-text-input`
dependency.

# Invalidated Approach

The first integration kept the accepted range, mutation, clipboard, segmentation, and geometry
coordinators publicly reachable through mutable widget state while GPUI keyboard and platform-input
handlers emitted generic host events. It also admitted a resident page before widget-produced exact
geometry and painted the two through separate publication paths.

# Evidence

The superseded phase's completion review found that baseline editing actions did not drive the accepted
coordinators, typed operation result delivery was incomplete, and mutation settlement events were
unreachable. It also showed that page replacement could become visible before exact keyed geometry
publication, so geometry-capacity failure could pair new text with the prior surface's geometry.

The same review found absent required pointer, painting, read-only, focus-loss, atom, and nonresident
platform-query behavior; unadmitted retained IME/page clones; incomplete restoration reconstruction;
and a public mutable geometry path that allowed caller-asserted viewport facts. Mechanical checks
and 146 passing tests did not detect these architectural omissions.

The first replacement closed the generic facade, public mutable geometry escape, several mounted
interactions, file sizing, and clone-free page ownership, but its re-review showed that component
coherence was still asserted too narrowly. Live selection mutated outside `CoherentSurface`,
estimated scrollbar ratios inferred source offsets without a complete exact index, admitted commits
were cancelled during detach, nonresident platform replacement fell back to the current selection,
shaped line allocations were undercharged, and restoration could report ready without all seeded
ranges or all quiescence conditions. These are the same root failure: separately current logical,
visual, and lifecycle facts were treated as one coherent widget without one atomic owner.

# Why It Failed

Exact identities around separate components do not make their independently published results one
coherent widget surface. A generic event facade also transfers widget-owned baseline editor policy
back to the host and makes the accepted coordinators optional rather than constitutive.

# Course Correction

Replace the facade with one app-neutral widget-owned lifecycle. GPUI actions and platform input must
open and advance the exact coordinator operations; the public host boundary carries typed requests
and keyed results without exposing mutable exact geometry construction. Pages, shaping, atoms,
caret, selection, hit regions, reveal, scroll facts, and restoration facts stage under one key and
publish atomically only after actual-value capacity admission. Every failure retains the complete
prior coherent surface.

The correction must also freeze logical selection and caret inside the published surface, use only a
complete exact geometry index to convert scrolling into source anchors, detach admitted commits
without cancellation, route nonresident UTF-16 replacement through exact bounded replay, account
every retained shaped allocation, and require every pending operation to be quiescent before seed
export. It must rebuild every seeded range with a bounded intra-anchor offset, advance layout epochs
from actual GPUI shaping inputs, and prove the complete mounted interaction and teardown contract
rather than event receipt alone.

Further source analysis established that the accepted geometry contract cannot currently supply the
required exact scroll target. Its capped sparse index has no exact block-position-to-source query or
streaming continuation. Reconstructing between retained entries would require shaping through an
arbitrarily long logical line, but the available shaping primitive requires that complete line as
one string. Widget integration therefore stopped until authority defined a bounded exact streaming
layout/index continuation and exact target-resolution owner; estimated mapping, page-fragment
shaping, caller-asserted entries, and whole-line assembly remain invalid.

The first owner built over the accepted streaming primitive still failed completion review. It left
the caller-staged geometry coordinator and superseded mounted widget publicly live, retained every
fragment between a sparse predecessor and distant target until output capacity failed, and could
pair a checkpoint's earlier source offset with an already advanced grapheme cursor. Its capacity
model omitted retained owner, style, job, and candidate state, did not report release of a desired
absolute target, and could not complete an empty source. Green focused and package suites therefore
proved only the chosen examples, not the bounded exact owner required by authority.

The first correction removed the caller-staged geometry and mounted widget path, bounded sparse-gap
output, keyed desired targets, and completed empty sources, but review still found semantic
incoherence. Target resolution chose the last caret map on a visual line instead of its leading
source anchor; newline checkpoints paired post-newline source/cursor facts with the prior logical
line; terminal errors and successful replacements silently dropped named state; and accounting
excluded peak coexistence while a borrowed page, GPUI admission, active/candidate collections,
converted publication, and prior publication were simultaneously live.

The exact-owner correction therefore retains only target-window output while exact continuation
crosses arbitrary sparse gaps, resolves the leading source anchor of the target visual line,
creates checkpoints only from wholly matching source, cursor, logical-line, segment, and
continuation state, and reports every terminal or replacement release. Admission covers peak
transient coexistence and every persistent input, job, continuation, checkpoint, and publication
exactly once. Desired targets and empty-source completion retain explicit lifecycle proofs.

The next review found that a nominal borrowed-page charge still counted atom fallback strings but
not the borrowed page and `AtomFact` records, while spanning-atom scanning cloned an already retained
atom without observing the duplicate peak. Exact admission therefore treats the complete borrowed
page record graph as live alongside owner state and moves a spanning atom or explicitly charges the
clone. Byte and item capacities receive separate direct boundary proofs, and target tests use known
soft-wrap line starts and independently expected viewport/overscan output rather than only comparing
two executions of the same resolver.

Implementing that correction exposed a dependency-boundary gap: GPUI reports exact retained bytes
for a streaming admission but not exact retained semantic item counts. Atom fragments hide their
retained shaped line, and the public charge combines shaped-run and glyph bytes while exposing no
run, glyph, or decoration record counts. `gpui-text-input` therefore cannot prove exact item-cap
admission without inventing an approximation. The owned GPUI fork must first expose its exact
retained item charge in Phase 113; Phase 114 then consumes that dependency-owned fact.

Phase 113 closed that dependency gap with one GPUI-computed retained-item charge covering text
payloads, caller style runs, shaped runs, glyphs, decorations, wrap facts, maps, fragments, and
continuations. Oversize-atom caller runs are excluded after GPUI discards them, while the shaped
records retained by the atom remain charged. The consumer can now admit the exact returned fact
without inspecting GPUI-private shaped payload.

Phase 118 exposure then invalidated the accepted page-demand assumption itself. A nonresident
widget knows the logical UTF-8 byte extent but cannot know whether an arithmetic bounded page edge
falls inside a multibyte scalar. `PageRequestKey` nevertheless fixes one exact byte range, and
`RangePage::new` requires the response to match that range exactly while carrying a `String` whose
byte length equals it. The host may neither adjust the edge nor return a typed boundary-resolution
continuation. This makes arbitrary-Unicode geometry, segmentation, clipboard collection, platform
replay, reveal, and restoration impossible without assuming caller-known boundaries, requesting an
unbounded remainder, or violating the exact response key.

# Affected Work

The bounded GPUI streaming-layout, retained-item-charge, and exact text-input target-resolution
phases are accepted. Phase 118 may integrate the corrected widget lifecycle over those
boundaries. The owned GPUI design, text-input package design, and text-input widget specification
define the target state; no Beryl compatibility path is permitted.

Phase 118 completed and independently accepted the source-selected page envelope before live widget
membership. Adjacent demands carry one proven boundary, direction, and byte ceiling; validation
demands cover untrusted offsets without rounding them. The corrected boundary preserves request
identity, binding, revision, purpose, byte caps, stale-result rejection, and exact release without
making the host or widget retain a whole value or guess a character boundary. Phase 120 resumes
widget integration over that accepted API after the keyed-scrollbar prerequisite is accepted.

The next live-widget attempt again compiled and passed its canonical and local 153-test suites, but
independent review invalidated its claim of one coherent owner. Rebind retained an old surface while
new edits and clipboard work used the replacement binding; active geometry could publish against a
newer desired selection or scroll intent; layout replacement could repaint the old surface with new
inputs; and the consumed scrollbar exposed no owner identity or mount-generation key at all.
Mounted staging could strand a rejected fragment, history bypassed the staged transaction owner,
non-origin UTF-16 mapping was incorrect, required pointer and atom behavior was absent, queued
clipboard writes and late pages lacked exact cancellation or release, quiescence omitted unpublished
platform payload, and byte/item accounting both double-counted inline records and omitted transient
allocations. Self-calibrated cap tests and broad green suites did not prove the declared lifecycle.

The correction starts by making the owned `gpui-scrollbar` implementation satisfy its already
authoritative keyed-owner and same-snapshot interaction contract; `gpui-text-input` must not hide
that dependency gap behind an unkeyed callback adapter. The widget replacement must then remove
independently current configuration, desired, platform, clipboard, history, pointer, and capacity
state: every interaction uses the published surface key or starts a separately keyed atomic
replacement, and every cancellation, rejection, detach, late result, and prepaint allocation has an
exact owner and terminal release. The current live widget source is unaccepted working material
until those boundaries pass a fresh review.

The first keyed-scrollbar implementation still treated GPUI's pending and active drag machinery as
incidental framework state. It registered drag initiation across the whole lane, returned a drag
value even for lane and horizontal no-op presses, and cleared only retained scrollbar state when a
drag key became stale. GPUI installs its active drag after the listener returns, so an ignored or
obsolete threshold crossing could still retain capture; a rerender could also hit-test an old press
against a replacement owner's snapshot. Exact keys inside scrollbar state therefore did not govern
the complete mounted lifecycle.

The correction must make the press-to-threshold transition itself owner- and mount-bound. A lane
page press or horizontal no-op never registers an active drag, an obsolete pending press cannot be
reinterpreted by a replacement render, and every stale move, owner replacement, unmount, teardown,
release, or capture cancellation terminates both GPUI's capture and the scrollbar's retained state
exactly once. Mounted tests must observe GPUI active-drag state directly; state-only tests cannot
prove this boundary.

Thumb-only registration and keyed pending state were still insufficient because GPUI retains the
mouse-down across rerenders but invokes the newest render's drag constructor. Comparing only owner
and geometry lets an old press survive render B and become valid again if later geometry equals
render A. Pending interaction identity must therefore include the exact originating render or
constructor snapshot, not merely values that can recur. Promotion accepts only the constructor that
belongs to that retained press; every later render rejects it and defers GPUI capture release even
when owner and geometry happen to match again.

The semantic render epoch then exposed two narrower identity erasures. A drag value's destructor
cancelled by owner alone, so GPUI's ordinary movement redraw could drop an unused newer-frame value
and terminate the genuine active drag created by an earlier frame. Separately, the scroll-handle
adapter treated every recreated handle interaction as the same instance, so replacing handle A with
handle B under recurring owner and geometry could preserve the press epoch. Destruction and adapter
recreation are lifecycle events, not anonymous cleanup: cancellation must name the exact pending or
active drag instance, and scroll-handle interaction identity must remain stable for one handle while
changing across distinct handles.

The owned GPUI `ScrollHandle` boundary then proved unable to supply that required distinction:
clones share one private `Rc<RefCell<ScrollHandleState>>`, but the public type exposes neither
equality, a stable opaque identity, pointer equality, nor access to its inner allocation. Equal
geometry or mutable scroll state cannot substitute because different handles may legitimately have
the same values, while a caller token would move actual-handle authority outside GPUI. The minimal
correction is an owned-fork `ScrollHandle::ptr_eq` query that reveals only whether two handles share
the same retained allocation; scrollbar integration remains blocked until that API is accepted.

Phase 119 implemented and independently accepted that exact query. Phase 120 published it as owned
fork commit `b83f38e38839ab1b917febfbbacfbed900e57e09` and repinned the canonical scrollbar graph, so GPUI
now distinguishes clones of one actual handle from separately created equal-state handles without
exposing the allocation or state. The keyed scrollbar lifecycle may consume that boundary while
correcting exact drag-instance destruction and handle-replacement recurrence.

Phase 121 then completed and independently accepted the keyed scrollbar prerequisite. Pending and
active interactions retain exact private instances across semantic redraw, provider reentrancy, and
completion; public custom renderers receive opaque non-retentive state-bound receipts; provider
returns are exact-revalidated before mutation or callbacks; and pending promotion remains retained
through its provider call so recurrent state cannot exploit an empty-slot ABA transition.

Phase 122 exposed one remaining exact-geometry prerequisite when non-origin traversal released a
forward page before `GraphemeCursor` requested pre-context at that page boundary. Retaining the old
page or feeding an entire returned context page raw was invalid: the former broke fixed residency,
while the latter could reinterpret bytes covered by an authoritative opaque atom and reconnect
Unicode state across the atom-imposed boundary. The accepted owner now requests bounded backward
adjacent context, replays from the exact unconsumed forward anchor, and gives each authoritative
atom a private local Unicode cursor origin carried coherently through sparse checkpoints. Context
feeds no atom-covered bytes, advances no source or visual facts, and releases exact keyed state;
ordinary oversized graphemes remain Unicode graphemes rather than acquiring atom semantics.

Phase 123 review found the same coherence failure in the mounted boundary: terminal-only history
settlement bypassed staged edits; stale same-purpose pages could remove newer work; nonresident IME
lost marked metadata; disposal could exceed detached-commit capacity; resident page reuse could
strand clipboard work; and placeholder state was outside publication. The accepted correction
replaced history with exact keyed proposals and bounded fragments through the ordinary edit owner,
validates every deterministically knowable successor and selection fact before commit admission,
proves a page key before taking current state, services resident and coalesced aliases under the
widget owner, carries marked metadata through replay, reserves detach capacity before admission,
and publishes admitted placeholder payload with the coherent surface. Malformed precommit work
rejects and quiesces without changing publication; admitted commits remain exact and detachable.
