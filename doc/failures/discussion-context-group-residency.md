# Discussion Context Group Residency

Treating synthetic context chunks as independently evictable ordinary records loses the group's
scroll extent and return path. Existing transcript adjacent demand is driven by resident history
rows, while context chunks have explicit local provenance and no canonical projection row.
Releasing a subset during page extension can therefore leave context without enough durable
boundary residency to request its missing chunks.

The production release call is in `ResidentTranscriptCore::prepare_adjacent_extension`; ordinary
scrolling itself does not evict these records. This narrows the correction to extension retention
and whole-group release rather than requiring a second context loader or persisted descriptor.
The context envelope already has a 65,536-byte source bound, and its derived chunk set is bounded.
Retaining that complete charged group while any chunk is protected preserves ordinary virtual
extent while the renderer and measurement remain limited to the realized frame.

Coordinate group retention with the exact adjacent history row needed by existing paging. Release
the whole group when it becomes obsolete, and qualify bounded source remount on return. A
boundary-backed projection fallback is still a paging row. When draft context is the transcript
tail, retaining its protected boundary row also retains the group: there is no forward history
entry from which to reload a context item that was separately released. Tests must
cover extension on both sides of the boundary, a viewport containing only context chunks, complete
charge release and selection protection. Eviction assertions alone do not prove that scrolling
back can recover the content. The owning contracts remain the transcript presentation system and
the transcript-view widget; this correction does not add an independent scroll surface.
