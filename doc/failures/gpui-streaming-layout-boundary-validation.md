# Scope

Owned GPUI composite streaming-layout completion review for `gpui-text-input` Phase 132.

# Invalidated Assumptions

The accepted composite-layout work assumed that fragment caret lookup enforced the same trailing-
boundary ownership as hit testing, that positive inline width was a sufficient proxy for a
nonempty line, and that matching style-run byte totals implied valid text slice boundaries.

# Decisive Evidence

A fragment whose trailing boundary belongs downstream could still return that boundary's caret
position. A following wrapped fragment at the same composite gap was therefore shadowed by the
upstream old-line endpoint.

A valid width-zero inline object left the inline offset at zero, so later overflowing content could
treat the line as empty and skip its required wrap. The continuation carried no independent line-
content fact.

Style-run admission checked only the total byte count. A cumulative run boundary inside a
multibyte UTF-8 scalar could reach platform shaping and be used as a string slice boundary.

The first explicit line-content implementation still accepted an impossible resumed phase with a
nonzero ordinal, an unfinalized line, and `line_has_content=false`. Erasing the fact from an honest
zero-width-object continuation could therefore suppress the same required wrap after resume.

The first scalar-boundary correction scanned the caller's complete run vector before enforcing the
configured run-count cap. An over-limit vector of zero-length runs could therefore force unbounded
validation work before rejection even though the accepted run list was bounded.

# Why The Approach Fails

Geometry ownership, line content, and UTF-8 boundary validity are distinct facts. Inferring one
from another produces platform-dependent failure or incorrect caret and wrap geometry even though
all aggregate counts appear valid.

# Accepted Correction

Enforce trailing-boundary ownership in fragment position lookup. Carry one explicit compact
nonempty-line fact across streaming continuation so width-zero objects participate in wrap
decisions. During existing bounded style-run admission, reject any cumulative run boundary that is
not a UTF-8 scalar boundary before shaping or publication.

Resume validation must reject a noninitial, unfinalized continuation whose line-content fact is
false; only the initial empty phase or a finalized line may be empty.

Enforce the run-count cap before scanning cumulative run boundaries on ordinary text, oversize
atoms, and inline objects, so rejected work is bounded by the same admission limit as accepted work.

These checks add no registry, lock, whole-source scan, or render-time allocation. Boundary and line-
content checks are constant time; style validation remains linear in the already supplied bounded
run list.

# Affected Authority

- `doc/plan.md`, Phase 132 completion gate.
- `../zed-fork/doc/design.md`, composite boundary ownership, wrapping, and styled-input validation.
- `../zed-fork/crates/gpui/README.md`, revision-scoped object identity.
