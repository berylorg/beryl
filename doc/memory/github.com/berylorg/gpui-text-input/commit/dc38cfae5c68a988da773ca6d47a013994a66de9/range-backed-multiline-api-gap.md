# Reason For Investigation

Beryl's large-draft composer contract requires the external `gpui-text-input` range-backed
multiline variant before Beryl mounts bounded draft editing. This investigation checked whether
the dependency's live public API and implementation at Beryl's pinned revision implement the
range source, bounded page residency, revision coherence, and staged edit sink promised by the
widget spec.

# Outcome

The widget spec promises an optional range-backed multiline surface. In
`doc/gui/widgets/text-input/spec.md`, the Purpose, Anatomy, States, Interaction, Layout, and
Variants sections describe revision-bound logical length, a bounded-page source, fixed resident
and pending-request windows, stale-page rejection, coherent rendering while pages load, host-owned
staged edit authority, and layout that never concatenates the complete source.

The live implementation does not expose that variant. `src/options.rs::TextInputMode` has only
`SingleLine` and `Multiline`. `src/state.rs::TextInputState` owns `text: String`, constructs it from
`impl Into<String>`, returns it through `text() -> &str`, replaces it through `reset_text`, and
clones it into whole-buffer `EditSnapshot` undo and redo state. `src/widget/mod.rs::TextInput` owns
one `TextInputState`; `new`, `multiline`, `new_with_options`, `text`, and `set_text` are whole-value
entry points. `src/change.rs::TextInputChange` carries an owned replacement `String`, but this is an
event payload after the widget has applied an edit, not a revision-bound host edit sink.

Layout is also whole-buffer. `src/widget/layout.rs::build_input_layout` clones
`state.text().to_string()`, then passes the complete value to
`src/widget/layout/shape.rs::shape_logical_lines`, which enumerates and shapes every logical line.
The public re-exports in `src/lib.rs` contain no range-backed model, revision, document source,
page/request, resident-window, or staged edit-sink type. Focused searches found no such API in
`src/` or `tests/`; `src/widget/render.rs::PendingScrollbarRequest::Page` is only a scrollbar page
motion request and does not fetch text.

Therefore dependency implementation work must precede Beryl's large-draft editor mounting. Beryl
must not bridge this gap with a whole-draft `String`, whole-value compatibility buffer, mirrored
complete value, or flattening adapter: that would violate the authoritative bounded draft contract
and hide the missing dependency boundary. `doc/rework/beryl-home/REWORK.md` makes the order explicit:
implement the external range-backed multiline text-input without whole-value compatibility buffers,
then mount bounded draft editing.

# Sources

- Source authority: GitHub repository `berylorg/gpui-text-input`.
- Canonical remote URL reported by Git: `git@github.com:berylorg/gpui-text-input.git`.
- Requested ref: none; the live sibling checkout's `HEAD` was inspected.
- Full resolved commit: `dc38cfae5c68a988da773ca6d47a013994a66de9`.
- Commit date and subject: `2026-05-16T12:45:02+02:00`, `Update gpui-scrollbar revision`.
- Access date: 2026-08-10.
- Spec source: `doc/gui/widgets/text-input/spec.md` in the sibling checkout. It was untracked at
  inspection time (`? doc/gui/widgets/text-input/spec.md`) and therefore is not contained in the
  resolved commit. Its exact Git object hash from `git hash-object --no-filters` was
  `753e425ce9585cdde3d85561b445368c5dce32ff`.
- Implementation files: `src/options.rs`, `src/state.rs`, `src/editing.rs`, `src/change.rs`,
  `src/lib.rs`, `src/widget/mod.rs`, `src/widget/events.rs`, `src/widget/layout.rs`,
  `src/widget/layout/shape.rs`, and `src/widget/render.rs`.
- Beryl integration authority consulted: `doc/features/composer/design.md`,
  `doc/features/composer/gui.md`, `doc/gui/widgets/conversation-composer/spec.md`, and
  `doc/rework/beryl-home/REWORK.md`.
- Beryl dependency identity: root `Cargo.toml` pins `gpui-text-input` to the same full commit SHA.

# Commands

Commands were run read-only in the sibling `gpui-text-input` checkout unless a Beryl path is named:

- `git remote -v`
- `git remote get-url origin`
- `git rev-parse HEAD`
- `git rev-parse --verify "HEAD^{commit}"`
- `git show -s --format="%H%n%cI%n%s" HEAD`
- `git status --short`
- `git status --porcelain=v2 -- doc/gui/widgets/text-input/spec.md`
- `git ls-tree -r --name-only HEAD -- doc/gui/widgets/text-input/spec.md`
- `git hash-object --no-filters doc/gui/widgets/text-input/spec.md`
- `rg --files`
- `rg -n -i "range.?backed|resident|page source|page request|pending request|edit sink|edit authority|staged edit|revision-bound|logical length|bounded.page" src tests`
- `rg -n "pub enum TextInputMode|enum TextInputMode|RangeBacked|range_backed|pub fn .*range|pub trait|trait .*Source|trait .*Sink|Page" src tests`
- `rg -n -i "range-backed|resident page|page source|edit sink|staged edit|large draft|draft editor" doc/plan.md doc/features doc/gui doc/rework -g "*.md"` in Beryl.

# Refresh Triggers

- The range-backed spec is committed, removed, or its blob hash changes.
- Beryl updates the pinned `gpui-text-input` revision or uses a different build variant.
- `TextInputMode`, `TextInputState`, `TextInput`, or the public exports gain a range-backed variant,
  revision identity, document/page source, bounded resident window, or staged edit sink.
- Source, tests, or Beryl integration authority contradicts this negative API finding.
