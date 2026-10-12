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
