# Selected Process Startup

## Overlapping Preparation Frames

Same-home interrupted-Exit qualification exposed an overflow on the ordinary Windows GPUI worker
while restoring a real selected window. The fixture persists a selected session through ordinary
acquisition, closes the seed store, and starts the same configured home through the process
startup controller. No substituted graph, window registration or retirement result is involved.

The original focused run `ddd949aa-8a1e-45cd-8ad4-4b6c254f27d0` overflowed before startup completion;
the later 15-second fixture watchdog was secondary. Diagnostic run
`b29e36e7-1b3b-4333-aaae-ef4e9f6f8c23` localized this to editor activation beneath restore-set
preparation. Unoptimized library assembly reserved 436,944 bytes in `restored_step`, 302,352 bytes
in `prepare_selection`, and 108,160 bytes in its consuming `RestoredWindowComposer::prepare`
caller. Large temporaries for later work remained live across nested validation and construction.

Separating only restoration admission from activation was insufficient. Activation and claim
settlement then completed, but selected-service preparation still overflowed. Isolating service
construction removed its temporaries from source validation; the remaining shell-construction
frame also had to be separated from composer preparation. The correction retains existing values
and boxed custody while giving admission, activation, service construction and shell construction
separate noninlined call frames. Validation, command order, pending/retry outcomes, revision
advancement, failure custody and allocations remain unchanged. Executor stacks and storage limits
are unchanged; no workers or lifecycle state were added.

Clean focused run `26f9d4a9-befe-4963-bd18-6ff20a5fd165` passes ordinary selected startup, failed
Exit, actual same-home graph retirement and private preparation, then unpublished-graph disposal
and native/home cleanup. Temporary probes were removed. A fixture assertion was corrected to
respect the existing read-only shutdown gate before retirement; disabled input is checked after
retirement. This evidence covers one selected window and proven noncommit, not selected rebinding,
publication, or committed/indeterminate Exit recovery.

The comparable corrected assembly reserves 536 bytes in `restored_step` and 10,608 bytes in
`prepare_selection`. Admission, composer preparation and shell preparation use separate frames
of 107,936, 212,112 and 144,144 bytes; service construction uses 291,952 bytes outside source
validation. These measurements establish reduced overlap for this build, not a universal stack bound.

All 117 lifecycle, initial-composer and restore-set regressions passed
(`7e92e693-1252-4830-86df-1dda1fb6cd92`). Production compilation, scoped formatting and independent
semantic review passed. The two assembly artifacts and all six printed failed selected homes were
removed and verified absent; successful selected fixtures cleaned themselves up. The separate
retry-timer fixture correction and its cleanup limit are recorded in
[recovery fixture evidence](recovery-mount-fixture.md#reopening-retry-deadline).

### Acquired Replacement Composition

Later progressive-startup qualification exposed the same overlap mechanism on the empty-session
acquired path. Run 006's acquired case overflowed on an unnamed WinRT pool worker before its
watchdog expired. First-chance debugger probes of binary SHA256
`24E532A9BB253A1DD0A2E94F2F5CA4942435079B1DDEFED83FA47FA9327EE238`
captured `c00000fd` under eligible-empty inspection, window acquisition, creation and restore-set
preparation. Exact PE unwind ranges identify a 178,544-byte private inspection frame and further
overlapping acquisition/creation frames; nearest public-symbol names are not exact private frame
identities. The restored-path receipt above did not cover this acquired composition.

`MainWindowRestoreSet::replacement_step` passed its already boxed creation owner through by-value
`advance`, retaining a large result frame during nested acquisition. The bounded correction calls
the existing `advance_owned` and handles its boxed outcome in a separate noninlined settlement
frame after acquisition returns. It preserves original Pending/Prepared/Settled custody, revision
checks, member order and commands. No executor stack increase or storage rewrite is involved.
This alone did not remove the acquired-path overlap. Run 011 still overflowed, and its first-chance
probe reached initial-composer claim validation, acquired-source inspection and pristine-thread audit.
A small 6,280-byte crossbeam collection reserve exhausted the remaining stack beneath the overlapping
creation, composer, validation and inspection frames. Moving the failure deeper is not successful
qualification. The acquired variant now follows the existing initial-variant consumption pattern:
extract its original acquisition and reservation in a separate noninlined frame, then invoke unchanged
initial-composer preparation after extraction returns. This removes the measured 231,552-byte generic
state-movement frame from nested claim inspection without new allocations or lifecycle state.
The acquired case passes in `007e1088-b33b-4839-850f-c81b71a19df7`, including genuine producer
release, native input, durable save and ordinary Exit with unchanged production stacks. Combined
qualification and whole-boundary acceptance remain pending; the separate restored cases still
require ordinary Exit qualification.
