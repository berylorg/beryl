# CAS Phase 70 Hard-Stop Activity Delta Overcount

## Invalidated Approach

Treating every published `CommandExecution` delta as an active hard-stop activity transition was
initially considered harmless because tracked provider-item identities are deduplicated.

That fails at the bounded overflow edge. Once the fixed active-item capacity is full, omitted
entries retain checked counts without retaining their item identities. Repeated deltas for the same
omitted command would therefore increment the omitted-active count repeatedly and corrupt the
frozen limitation result.

## Correction

Only successfully published resolved command start and completion frames emit hard-stop activity
transitions. Deltas emit no activity effect. The coordinator still deduplicates retained start
identities and decrements the corresponding retained or omitted count on completion, while
arbitrary command-output fragments cannot change active membership.
