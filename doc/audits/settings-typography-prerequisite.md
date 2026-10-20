# Settings Typography Prerequisite

Operator authorized the missing host typography API required by the complete Settings mount.
Published widget revision `75b92518efc7013a5df83e8f83a5ce6dd55489af` provides validated base
font size and optional family, applied to initial and retained live controls. Heading sizes derive
from the base, popups inherit the family, and tooltips apply it on their separate root.

All 122 owning cases passed across the nine widget integration targets, including four new cases
for options refusal, retained editor/history/composition state, rendered family replacement and
clearing, popup state, interaction gates and event silence. The normal production check passed.
Independent semantic review and correction review found no blocking acceptance findings. The
widget repository retains its own detailed audit at `doc/audits/base-typography-qualification.md`.

The Beryl manifest and lockfile change only the settings-window revision. The complete Beryl mount
has its separate [acceptance](settings-workflow-mounting.md). Simulated GPUI
layout does not prove native paint, physical font availability or sustained scrolling performance.
