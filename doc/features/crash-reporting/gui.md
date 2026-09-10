# Crash Reporting GUI

This is the normative composition supplement to [design.md](design.md).

## Crash Report

Mount-into: crash-report-window.body

This is a feature-local arrangement of text and two canonical `command button` controls. It has no
reusable control identity, collection, editor, or scrolling behavior beyond its children and the
fixed integration window.

The vertical body contains the fatal-error heading, a short explanation that Beryl has stopped,
the bounded report preview, one reserved inline copy-result line, and a horizontal command group.
`Copy to clipboard` is secondary. `Exit Beryl` is the default command. Tab moves between those two
buttons; Enter and Space invoke the focused command. Initial focus is on Copy to avoid accidental
exit while inspecting the report. There are no menus, links, text-edit commands, or hidden normal
application shortcuts.

The preview uses a fixed bounded text region. Excess lines or text are visibly marked as omitted
from the preview; Copy includes the complete bounded report, including any capture-truncation
marker. Report text is inert and cannot create controls or alter command eligibility. Fixed
built-in styling and fonts require no settings, theme repository, Beryl-home read, or network work.
