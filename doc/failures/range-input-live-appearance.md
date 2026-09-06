# Range-Input Live Appearance

## Invalidated Assumption

The main-window shell phase assumed the accepted range-input API could consume coherent appearance
updates in a mounted main-window shell. Initial themed construction works, but later theme changes cannot reach
all of the existing editor's visual properties.

## Evidence

- `../gpui-text-input/src/range_widget.rs` keeps `RangeTextInputConfig` private. `set_layout`
  updates layout and geometry style; `set_presentation_generation` updates presentation identity.
  Neither replaces the retained theme or scrollbar style, and no public live-style setter exists.
- `../gpui-text-input/src/range_widget/render.rs` reads `config.theme` directly for placeholder,
  selection, marked underline, and caret colors, and `config.scrollbar_style` for its scrollbar.
  These values cannot all inherit a shell element's text style.
- `doc/features/theming/design.md` and `doc/systems/theme-runtime/design.md` require complete,
  coherent appearance generations across live windows. Updating shell chrome alone leaves mixed
  old and new role values.

GPUI streaming fragments also retain color-bearing shaped lines behind private decoration state.
The existing range-input layout-update path can reshape, but requires a replacement geometry
transition and realization work. It cannot supply a synchronous color-only update. The renderer
therefore needs an immutable paint-color override before the widget can apply a complete theme.

## Follow-Up

The Operator authorized the prerequisite on 2026-09-05. The ordered work is an owned-GPUI immutable
paint-color boundary, a small owned-widget live-style update preserving the mounted editor's state,
and dependency propagation before Beryl's coherent appearance integration. The GPUI contract lives
in `../zed-fork/doc/design.md`. Its immutable paint primitive and the range-input live-appearance
integration are implemented and verified against their owning contracts. Canonical dependency
publication and pins remain pending before Beryl shell integration.
Do not substitute editor reconstruction or weaken the complete-generation requirement silently.
The original shell attempt stopped before source edits, builds, or test execution; this is
unrelated to constructor fallibility and does not justify a fallible GPUI entity-construction API.
