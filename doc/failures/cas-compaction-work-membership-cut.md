# Compaction Work Membership Replacement

The compaction work source initially registered a replacement local operation before disposing
the previous operation's independent membership guard. Independent review found that these two
source mutations could expose both operations as registered for one thread, even though the
coordinator replaces its single map entry under one lock. Revision validation alone cannot reject
a stable intermediate cut.

Replacement now validates both observation identities and atomically clears the old membership,
installs the new membership and advances the source revision under one source lock while the
coordinator holds its operations lock. Disposing the replaced membership guard is then a no-op.
Result-only waiters retain no observation guard. Command ownership remains independent so target
cleanup stays observable after local removal.

The deterministic membership test checks 64 replacements before each old guard is dropped,
requiring exactly one registered local and one revision advance. The final acceptance run passed
100 lifecycle, compaction, stop, persistent-failure and terminal regressions; production compilation,
formatting, diff checks and independent semantic review passed. Seven guarded verification runs
left no owned child processes; their temporary directories were reclaimed. This correction changes
no execution admission or authoritative lifecycle contract. No unresolved membership risk remains.
