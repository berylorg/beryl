# Scope

Phase 13 streamed provider observation ingress.

# Invalidated Approach

Implement the strict incremental JSON lexer and parser as private modules inside
`beryl-backend`, coupled directly to the provider observation sink and page-lease exchange.

# Evidence

- The JSON correctness and fixed-residency contract is independent of CAS schemas and is useful to
  other projects with the same reliability requirements.
- The partial backend reader consumes an input byte before downstream fragment acceptance on one
  backpressure path. Retrying after `WouldBlock` can therefore omit the unaccepted byte.
- Keeping syntax, UTF-8, escape, number, provider schema, and page-lease state in one backend module
  obscures which bytes have committed and makes independent exhaustive testing harder.
- Existing parser libraries either retain complete scalar tokens, require retry with a growing
  input slice, materialize object keys, or leave Beryl responsible for the same hard state while
  adding an awkward dependency boundary.

# Why It Failed

The private implementation duplicates a reusable infrastructure concern inside a protocol package
and couples parser progress to one sink API. Its consumed-before-accepted edge also violates exact
resumption, so it cannot be repaired as a small integration fix.

# Course Correction

Create the independent sibling `../bounded-json` project. It owns only allocation-independent
strict JSON recognition over caller-provided fixed input, output, and structural storage. Its
progress result reports exact consumed and produced extents and explicit input/output pressure.
`beryl-backend` removes the private recognizer and retains only the adapter that maps generic JSON
events and fragments to CAS schema state and admitted `beryl-stream` page leases.

# Affected Authority

- `../bounded-json/doc/design.md` and `../bounded-json/doc/plan.md`.
- `doc/systems/bounded-resource-dataflow/design.md`.
- `crates/beryl-backend/doc/design.md`.
- `doc/rework/beryl-home/REWORK.md`.
- `doc/plan.md`, Phase 13 sequence items 4 and 5.
