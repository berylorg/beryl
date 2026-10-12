# Shared Command Style Must Belong To The Active Crate Graph

The mounted Code header attempted to reuse command geometry and secondary-button defaults from
`src/shell/layout.rs` and `src/shell/render_theme/button.rs`. Semantic navigation could locate
those definitions, but the current app library does not mount the legacy `shell` module. The
focused Code test build therefore rejected the imports before running any cases.

The clean prerequisite extracts the existing passive constants and palette values into the
compiled widgets module. Current Code consumes that definition; remaining legacy source may
reexport it without activating obsolete shell services. It preserves the
[shared command geometry](../gui/widgets/contracts/beryl-command-geometry.md) and does not create
a second theme policy or revive the former shell architecture.

Check the current crate entry-point module graph before choosing a reusable source symbol during
the architectural rework. A symbol's presence in source navigation does not prove that it belongs
to the compiled application. Verification and final review remain in the active [plan](../plan.md).
