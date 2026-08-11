# Branch Discussions GUI

This is the normative supplemental GUI composition file for `design.md`. It owns discussion-context and discussion-status mounts, labels, and mapping of design-owned visible states. Product behavior remains in `design.md`; internal provenance, queueing, handoff, and recovery mechanics remain in the system authorities linked there.

## Discussion Context Record

Mount-into: transcript.context-records

The feature contributes one synthetic-context presentation item at the exact branch boundary of a branch-discussion transcript. It configures the project-local `transcript view` with the exact selected text, compact source provenance, and the unavailable-context meaning defined in `design.md`.

This is a feature configuration of the transcript view's canonical synthetic-context group anatomy rather than a separate widget. The item introduces no independent viewport, scrolling, focus model, disclosure state, popup, or layout algorithm.

The record uses `DISCUSSING` as its heading, renders the selected passage as readonly selectable content, and shows compact source provenance without exposing backend ids. It supplies no Quote, Edit, Discuss in new branch, Resolve, Archive, or ordinary turn-menu target.

## Discussion Status Strip

Mount-into: main-window.discussion-status

The feature configures one bundled `segmented status bar` as a fixed-height strip immediately above the composer and below any visible activity panel. This is a feature-local configuration rather than a project-local widget because the built-in status bar already owns strip geometry, segment layout, focus, direct-action, disabled, and truncation mechanics.

The leading passive segment uses the key `DISCUSSION`. Its value maps the selected discussion state
defined in `design.md` to `Open`, `Resolution pending`, `Handing off`, `Handoff failed`,
`Unavailable`, or `Archived`. `Unavailable` is the terminal exact-mutation-collision state from
`design.md`; it exposes no retry action.

When `design.md` makes retry available, a trailing direct-action segment is labeled `Retry handoff`. Its pending presentation remains visible and disabled through `disabled-command-tooltip`.

When retry is unavailable, the trailing action is absent. No mapped state exposes Resolve or Archive.

Composer writable, inert, or readonly presentation follows the state mapping in `design.md`. Long error detail maps to the established per-window notice and never expands the strip.
