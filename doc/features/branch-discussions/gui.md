# Branch Discussions GUI

This is the normative supplemental GUI composition file for `design.md`. It owns discussion-context record and discussion-status mounts and feature-specific configuration. Branch creation, resolution admission, queueing, archive behavior, and recovery semantics remain in `design.md`.

## Discussion Context Record

Mount-into: transcript.context-records

The feature contributes one synthetic-context presentation item at the exact branch boundary of a branch-discussion transcript. It configures the project-local `transcript view` with the stable discussion identity, context revision, exact selected text, source provenance, and unavailable-context meaning.

This is a feature configuration of the transcript view's canonical synthetic-context group anatomy rather than a separate widget. The item introduces no independent viewport, scrolling, focus model, disclosure state, popup, or layout algorithm.

The record uses `DISCUSSING` as its heading, renders the selected passage as readonly selectable content, and shows compact source provenance without exposing backend ids. It supplies no Quote, Edit, Discuss in new branch, Resolve, Archive, or ordinary turn-menu target.

## Discussion Status Strip

Mount-into: main-window.discussion-status

The feature configures one bundled `segmented status bar` as a fixed-height strip immediately above the composer and below any visible activity panel. This is a feature-local configuration rather than a project-local widget because the built-in status bar already owns strip geometry, segment layout, focus, direct-action, disabled, and truncation mechanics.

The leading passive segment uses the key `DISCUSSION` and the value `Open`, `Resolution pending`, `Handing off`, `Handoff failed`, or `Archived`. The feature supplies the exact state from the selected discussion revision and keeps the same strip instance mounted throughout that thread's lifecycle.

For retryable handoff failure with a live job, a trailing direct-action segment exposes `Retry handoff` for that already admitted immutable job and the composer is inert. While retry is pending the action remains visible and disabled through `disabled-command-tooltip`.

For terminal handoff failure, the leading value remains `Handoff failed`, the trailing retry action is absent, and the composer returns to its ordinary writable presentation. `Archived` keeps the composer readonly. No state exposes Resolve or Archive.

The status strip value, retry-action availability, and composer writable or inert presentation update atomically. Long error detail uses the established per-window notice and never expands the strip.
