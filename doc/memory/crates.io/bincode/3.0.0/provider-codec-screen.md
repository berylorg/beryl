# Reason For Investigation

Audit SP-006 screened bincode as a possible third-party replacement for provider value codecs.

# Outcome

Reject 3.0.0 at maintenance screening. The official package states that development ended and
this final release intentionally contains a compiler error to communicate that status. It is not
a usable serializer implementation. Earlier versions were not proposed or qualified by this audit.
No feature, target, MSRV, advisory or adoption clearance is claimed for new use.

Independently, Beryl's PIV1/V7 codecs retain exact tags, ordered duplicate object entries, distinct
numeric representations, typed assets, reused ranges and bounded streamed validation. Existence
of a generic binary serializer does not establish a smaller equivalent implementation.
No installation, prototype, dependency edit or savings estimate resulted from this screen.

# Sources

- [Official bincode 3.0.0 package](https://docs.rs/crate/bincode/3.0.0), inspected by the provider
  auditor and root on 2026-09-06; published 2025-12-16.
- [SP-006 audit evidence](../../../../audits/code-simplification/reviews/syndic-provider-findings.json).
