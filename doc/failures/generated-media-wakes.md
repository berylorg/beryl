# Generated Media Wakes

Two bounded-notification assumptions were invalidated during generated coordinator qualification.

Generic connection work revisions include quiet router polls. Waking generated discovery after
every quiet poll made an unfinished producer continually rediscover itself until its test runtime
expired. Source-owner notifications must distinguish real source changes from diagnostic quiet
observations. Quiet polls retain their work revision and can release an actually registered capacity
waiter; they do not independently request generated discovery.

Registering a stage waiter after an unsuccessful acquire can miss a release already consumed by a
peer. Registering and then retrying acquisition closes that boundary, but does not alone cover
active-flight coalescing: a notification can cause a peer sweep to skip still-active R and consume
the wake before R finishes Pending. The sequence `Resource(R)`, notify, peer discovery, skip R,
finish R, Wait strands eligible work without another external event.

Each existing bounded active reservation now keeps one recheck bit. A notification marks active
reservations; finishing a marked reservation preserves one fresh finite sweep. Discovery itself
does not mark active work, so an unchanged pending producer still sleeps. No history, per-resource
timer, extra observer or independent task is introduced. Deterministic scheduler coverage includes
the exact skipped-active ordering, coalescing, no-notification sleep and retirement; native
unfinished/replacement qualification must prove the composed behavior.

The owning [generated coordinator contract](../systems/image-assets/generated-finalization.md)
already requires registered notifications, finite continuations and sleeping without eligible
progress. These are implementation corrections, separate from the
[unresolved non-text projection architecture](generated-resource-projection.md).
