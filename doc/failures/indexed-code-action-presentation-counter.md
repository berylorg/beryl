# Page Publication Counters Are Not Source Action Identity

Code Copy initially compared the complete resource owner snapshot, including
`presentation_revision`. Real native Copy then terminated as source changed before reaching its
checked writer, although its source, resource, activation and provider were unchanged.

Ordinary resource demand uses the current global presentation counter. Indexed response admission
stores that demand and then increments the counter for the new page. Thus a later Source page can
change this derived counter as a consequence of the same Copy. Mock page assembly with an
unchanged owner did not expose the mismatch.

Stable action identity excludes only that presentation counter. It retains activation, view,
provider revision, record, exact provenance, resource kind/id and metadata, together with current
host ownership, exact binding/root, action sequence and final admitted-query equality. This follows
the existing indexed-owner currentness boundary and does not relax replacement or retirement
qualification. Rendering freshness and source-effect custody remain separate responsibilities.

Qualification must advance the presentation counter across admitted pages and separately reject
real provider/provenance replacement. The real native multi-page Copy is required integration
evidence. Results and acceptance remain in the active [plan](../plan.md); the
[presentation boundary](../systems/transcript-presentation/shell-boundary.md) remains authoritative.
