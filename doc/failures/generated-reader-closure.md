# Generated Reader Closure

Native reader cancellation initially treated a private socket reset as a global init failure.
The stopped-reader regression in nextest run `d10c75f4-e45e-4bc4-a256-b54ff73c721d` received
namespace closure instead of its exact reader-close receipt. Killing a Unix sequenced-packet peer
with an unread request can produce `ECONNRESET` before a subsequent physical EOF. Closing now
continues draining after that reset; actual EOF and exact child reaping remain separate required
facts. The corrected stopped-reader case passed in run `8fd226ad-978e-40ae-9ec5-83bb018bc898`.

Self-review also invalidated eviction of an arbitrary terminal receipt when another worker slot
opens a reader. An automatic failed-open/closed receipt can precede the backend's final cancel;
cycling a different slot must not erase that reader's positive proof. Both owners retain its
existing reservation until an explicit next-sequence close/cancel is acknowledged. This uses the
same fixed reader cardinality and protocol, with no unbounded identifier history. A multi-slot
regression exercises repeated reuse of the other slot before the delayed acknowledgement.

Adding the independent reader channel also exposed a fast-exit ordering error: normal channel
EOF requested disposal before a queued positive workload-start proof was consumed. The complete
native run `a4fdcba1-e70e-45e7-b4c8-0ab54c2559d4` lost startup evidence for ordinary and abnormal
server exit and fixed home observation. Normal EOF is now a separate fact; setup proof is polled
after observing it and again after positive init reaping, before closure publication. EOF does not
supply namespace proof, and malformed/setup failures retain their separate refusal behavior.
The unchanged cases and the full forty-case supervisor suite passed on the final correction in
run `4eaff739-1d61-4f7c-813e-992fba468625`.

The [supervisor contract](../../crates/beryl-wsl-supervisor/doc/design.md#generated-file-readers)
owns closure and final acknowledgement. Complete source and native qualification remains in the
[active plan](../plan.md); the first corrected native run alone is not composed acceptance.
