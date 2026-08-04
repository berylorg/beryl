# Scope

Phase 13 backend decoded provider-fragment page admission, UTF-8 boundaries, terminal parser
commitment, and observation abandonment.

# Invalidated Approach

Let `bounded-json` write into every remaining byte of a partially filled page, copy fixed decoded
text into pages by raw byte count, and rely on dropping a partial current lease when parsing failed.

# Evidence

- `bounded-json` requires at least four writable bytes so one decoded Unicode scalar remains
  indivisible. A page with one to three bytes remaining can therefore report `NeedOutput` even
  though the page is not full.
- Raw byte-count copies can split a valid decoded multibyte scalar across provider fragments,
  contradicting the fragment contract even when the reusable parser emitted UTF-8-safe slices.
- A terminal parser call may commit decoded output before reporting its error. Merely advancing the
  current lease length and then dropping that partial page omits committed prefix bytes from the
  sink handoff.
- Calling abandonment while the capture still owns a page makes release ordering ambiguous to the
  destination and its resource diagnostics.

# Why It Failed

Parser output capacity, page capacity, and semantic fragment boundaries are different contracts.
Treating the remaining physical bytes as automatically writable ignored the parser's scalar
atomicity, while treating a committed but partial lease as disposable weakened the exact progress
contract at terminal failure.

# Course Correction

Exchange a nonempty page before the parser call whenever its remaining suffix is smaller than
`bounded_json::MIN_OUTPUT_CAPACITY`. Copy fixed decoded text only at UTF-8 character boundaries and
retain at most constant scalar-boundary slack. Preserve each valid parser output slice as a unit in
MCP discriminator probing instead of replaying it byte by byte.

After committing output from a failing parser call, hand off every nonempty current page through
the same ownership-preserving exchange before surfacing the parse failure. Every terminal path
drops its returned or current lease before invoking the infallible abandonment callback.

# Authority And Verification

`doc/plan.md` Phase 13 item 5, `crates/beryl-backend/doc/design.md`, and
`doc/systems/bounded-resource-dataflow/design.md` own this correction. Tests cover adversarial page
suffixes, escaped and direct multibyte UTF-8, non-ASCII MCP keys and values, same-call produced
bytes on parse failure, terminal fragment return, and zero leased pages at abandonment.
