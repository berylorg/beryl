# Scope

Phase 13 item 5 provider-message classification and structured-number conversion.

# Invalidated Approach

Reuse one fixed 256-byte accumulator for undecided JSON-RPC names and strings and for the complete
raw spelling of a structured JSON number. After accumulation, compare the classifier value or call
Rust's ordinary `f64` parser.

# Why It Failed

A fixed complete-spelling buffer turns an implementation allocation bound into a protocol limit.
Valid ordinary JSON may have a first key, `jsonrpc` value, or non-target method longer than that
buffer. The classifier can prove a mismatch through constant prefix state and route the message to
the existing explicitly budgeted ordinary path; rejecting it as an invalid provider string is a
schema error.

JSON also permits arbitrarily long lexical spellings for one finite numeric value. For example,
`1.0` followed by any number of zeroes is still exactly finite. Rejecting it only because its token
exceeds 256 bytes contradicts the provider cutover's no-arbitrary-cap rule. Retaining only an
unchecked leading significand is also invalid because silent truncation can select the wrong
IEEE-754 value.

These lexical cases differ from a real semantic value contract. `CasThreadId`, `CasTurnId`, and
`CasItemId` remain explicitly bounded identities and must reject an invalid or oversized decoded
value even when the surrounding provider field streams.

# Course Correction

Classifier probes retain only the constant prefix needed for exact comparison and decide ordinary
as soon as a mismatch is proven. The separately accepted 1,024-byte undecided-prefix rule remains a
bounded validator for pinned-order wire drift and may never emit a target method as an ordinary
value.

Structured finite-number conversion consumes arbitrary validated number fragments with fixed
state, produces the correctly rounded finite `f64`, and fails only for semantic invalidity such as a
non-finite result. It may summarize an unbounded suffix, but it may not impose a complete-token
length ceiling or silently approximate. Bounded identity probes validate their exact semantic
types while the original field fragments continue through the staging path.

The fixed significant-digit proof is specific rather than heuristic. Every binary64 rounding
midpoint has the form `(2k + 1) * 2^-1075`; its terminating decimal numerator is
`(2k + 1) * 5^1075`, with `2k + 1 < 2^53`. That numerator has at most 768 significant decimal
digits. Retaining those digits plus a nonzero-suffix fact is therefore sufficient to distinguish
every rounding cell without retaining or rejecting the remaining lexical suffix.

# Verification Consequence

Provider ingress coverage must include ordinary classifier strings and finite number spellings well
beyond every fixed scratch size, adversarial rounding boundaries, every input split, semantic
identity overflow and malformed cases, and proof that the provider sink is abandoned without a
live page lease on every rejected path.
