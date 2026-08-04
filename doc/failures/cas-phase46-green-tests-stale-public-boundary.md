# CAS Phase 46 Green Tests With A Stale Public Boundary

## Invalidated Approach

The first Phase 46 completion candidate passed the all-feature backend behavioral suite, Rustdoc,
formatting, metadata, dependency, and stale-symbol checks. That evidence was treated as sufficient
to show that the backend had reached its final local-bound API.

Fresh independent review found that default-feature integration targets were not all gated by
their required test-support feature. It also found compile-only public remnants that the exercised
behavior never used: collection-backed active flags, a complete rate-limit bucket map, an obsolete
whole-turn terminal model, a duplicate active-turn error projection, a redundant branch probe
report, and detached discovery and file-read helpers.

## Why It Failed

Behavioral tests prove only the paths they compile and execute. An all-feature build can conceal a
broken default feature boundary, while dead public types and methods can remain perfectly green
because no current test constructs them.

Text scans aimed only at previously removed resource-runtime names also cannot prove that every
surviving public shape satisfies current authority. A renamed `Vec`, `BTreeMap`, free-form
diagnostic, duplicate result model, or always-unavailable method can evade such a scan without
being architecturally valid.

## Course Correction

Backend phase completion now requires both locked default and all-feature all-target checks, the
behavioral suite, warnings-denied Rustdoc, and an authority-driven inventory of public exports,
public session methods, response variants, and production-reachable retained collections.

Unrestored families expose no stale compatibility model. Obsolete public surfaces are removed
directly, and final bounded shapes are introduced only where their semantics are already
authoritative. A fresh independent reviewer audits the frozen tree after remediation rather than
relying on the reviewer that found the first tranche.
