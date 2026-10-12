# Native Copy Fixture Attribution

Early shared-Copy native runs used retained temporary Beryl homes before the retained-fixture
ledger was enabled. Filtered failure output and suppressed successful output discarded some exact
paths. The installed nextest CLI provides no retained-run replay from which to recover them.
Timestamp or name-pattern inspection of shared Temp does not establish task ownership.

Subsequent runs registered exact homes and Save-writer directories through
`BERYL_NATIVE_RETAINED_FIXTURE_LEDGER`; full task logs also identify failed fixture roots. Those
positively attributed directories can be reclaimed after review, using verified exact paths.
Unidentified pre-ledger homes may remain under `C:\Users\user\AppData\Local\Temp`; their exact
names and sizes are unavailable. No shared Temp sweep is authorized by this evidence.

Enable the fixture ledger before the first native run, retain complete failure output through
review, and use the ledger for cleanup after the last consumer. Do not claim complete cleanup
when successful or aborted fixtures have lost their ownership records.

Indexed Table qualification exposed a second failure mode: the retained ordinary-command fixture
appended its home to the ledger only after the native callback returned. A ready-timeout panic
aborted the process before registration. Enabling the environment variable alone did not protect
that exit. The fixture now records the exact home immediately after creation, before preparation
and native startup, and requires an absolute ledger path. Table test readiness failures also retire
their exact fixture before reporting a failed result outside the native callback.

All five homes from the failing Table run remained attributable through its retained full output's
`mounted ordinary command fixture:` lines. They were reclaimed using verified exact paths after
their last consumer, and all five were confirmed absent. This attribution does not recover or
authorize a sweep of the older unidentified homes.
