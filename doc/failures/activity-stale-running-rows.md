# Activity Stale Running Rows

## Reported symptom

An active parent turn showed several blue `running` Activity rows for one child agent even though the displayed PowerShell commands, including `Get-Content`, should have completed quickly. Later rows for the same agent were already green or red. Multiple rows for one agent are valid because Activity identity is `(thread, turn, item)`; multiple terminally stale blue rows are not.

## Instrumented reproduction

The projection-owned lifecycle diagnostics were exercised through a freshly built diagnostic child on August 9, 2026. A three-command smoke and a representative child turn with twenty separate sequential `Get-Content -TotalCount 1 'doc/plan.md'` calls both completed normally.

The representative evidence boundary is sequence 141. Raw cursor pages cover every sequence from 23 through 141 exactly once. The twenty command items occupy exact start/completion pairs `48/49` through `124/125`. Every start inserted one running row; every completion used the same thread, turn, and item identity, matched the existing row, and changed it from `running` to `finished_ok`. At most one command row was running at any point. Child and parent terminal fallbacks at sequences 128 and 130 found no running rows. No stream failure, eviction, blank or over-bound identity, or truncated protocol string affected the evidence.

The child reached completed and idle state with no background work, and Activity Auto hid the panel. The controlled run therefore did not reproduce the reported stale-row state.

## Ruled-out fix boundaries

For the observed run, do not change lifecycle normalization, exact-key correlation, completion fallback, row identity, or parent-turn cleanup. Source inspection confirms completion synchronously mutates the authoritative exact-key record, rebuilds derived rows with the terminal status, and reaches the Shell render path through the existing notification flow.

Do not add refresh timers, periodic cleanup, forced repaint loops, transcript-child notification, or unstable row ids. Those changes lack supporting evidence, would obscure subsequent classification, and contradict the Activity contract.

## Presentation classification and closure

A fresh instrumented release was exercised through the ordinary composer and one child agent while Activity remained On. The target child ran six separate sequential `Get-Content` calls. The same process retained lifecycle sequences 3 through 41 and presentation sequences 14 through 133 without gaps, duplicates, eviction, target-row omission, or event-byte truncation.

Every target command start inserted one running row. Every completion matched the same exact `(thread, turn, item)` identity and changed it from `running` to `finished_ok`. Each corresponding presentation revision was followed by `shell_notified` and a render sample acknowledging that exact revision. Running samples used `activity.indicator.running` from the theme role with RGBA `56,189,248,255`; terminal samples used `activity.indicator.ok` from the theme role with RGBA `34,197,94,255`. Child and parent terminal fallbacks found no running rows.

Before diagnostic extraction, an exact-window capture from that process visibly showed the target child path's `Get-Content` rows green and the turn `ok`. The screenshot is semantic visual evidence rather than a claim about unobserved pixels, but it closes the previously missing link between correct retained render input and the visible target rows.

The normal-workflow run therefore did not reproduce the reported stale blue terminal markers. Together with the earlier representative lifecycle-only run, this was the required second non-reproduction. No lifecycle/projection mismatch, skipped Shell notification, stale render input, theme-role/color error, or GPUI reconciliation/paint discrepancy was classified in those controlled runs. Historical intermittency remained possible, so a future correlated capture was required to reopen the incident.

## Operator reproduction and root-cause classification

An August 10, 2026 normal-workflow capture reopened the incident and reproduced the lifecycle signature twice. The frozen rolling journal contains 8,691 newline-delimited records across four headers, with no JSON parse, schema-version, required-field, capture-sequence, capture-gap, or in-process lifecycle/presentation epoch-reset defect.

In one retained child turn, 80 `commandExecution` items started and 78 received an exact terminal lifecycle event. Two exact starts remained `running` for approximately 19 minutes 57 seconds. Each was sampled as running 414 times, including the render immediately before the turn-terminal fallback. Sixty-four and sixty-two later exact command items respectively completed while those rows remained running. No terminal command identity lacked a retained exact start, excluding completion under a different item identity. `turn_completed` then used `finished_running_rows` with `affectedRowCount = 2` and closed exactly the two stranded rows.

A second child turn independently repeated the same signature: 34 command starts, 32 exact terminal matches, two rows running for approximately 3 minutes 33 seconds, 180 and 179 running render samples, and 25 later exact command completions while each remained running. The immediately following `turn_completed` fallback again closed exactly two rows.

The presentation path remained coherent throughout. Every one of 2,144 projection revisions was followed by Shell notification and render acknowledgement, with maximum captured-order lag of one and three records respectively. No exact row was rendered running after a captured terminal lifecycle event; no indicator-role, theme-color, renderer-fallback, or stream-failure discrepancy occurred. The stale markers were therefore faithful presentation of authoritative Activity rows that had never received exact item-terminal lifecycle events, not stale GPUI state.

`TurnStreamEvent::ItemCompleted` for a recognized `commandExecution` maps directly to a completed `ToolActivityEvent`, and the Shell applies that event to the exact-key projection before transcript handling. The frozen file intentionally does not retain raw backend frames, so it cannot distinguish a CAS omission from loss during backend event decoding or delivery. It does locate the fault before Activity projection: four command completion lifecycles were absent, not delayed, mismatched, or rendered incorrectly. Beryl's turn-terminal fallback behaved correctly but allowed the rows to remain blue for the rest of each long child turn.

The backend turn-stream parser now warns when an `item/started` or `item/completed` notification decodes successfully but carries a generic `item.type` that is neither a recognized Activity source nor a known app-server item outside the current Activity source set. The warning contains only bounded notification method, thread, turn, item, and type fields plus the original type byte length; it does not retain the item payload. A future reproduction can therefore distinguish a completion delivered under an unsupported type from a completion absent before this parser boundary, while preserving existing forward-compatible event delivery and keeping expected plan, hook-prompt, sleep, and review-mode items quiet.

## Evidence

- Instrumented executable: `target/release/beryl.exe`, SHA-256 `E9631D5F3F271170C02E94E22FE50A4EC947895E9129B3BB0F51DE83131FFD3A`.
- Baseline snapshot: `target/phase2-activity-lifecycle-baseline-20260809.json`.
- Three-command smoke snapshot: `target/phase2-activity-lifecycle-evidence-20260809.json`.
- Representative raw pages: `target/phase2-activity-representative-page-01.json` through `page-08.json`.
- Active child screenshot: `target/phase2-activity-active-20260809.png`.
- The task-owned diagnostic child stopped cleanly. Its isolated 8.66 MiB home remains at `target/phase2-activity-lifecycle-home-20260809-1915` because sandbox policy rejected the verified recursive cleanup command before execution.
- Presentation-classification executable: `target/release/beryl.exe`, 27,568,640 bytes, SHA-256 `9659243C342BB0194E4C7306489FFADF8926AB5D6BEF895C0A69E3D90C6729D3`.
- Same-run manifest and baselines: `target/phase5-correlated-manifest.json` and `target/phase5-correlated-baseline.json`.
- Same-run lifecycle page: `target/phase5-correlated-lifecycle-page-001.json`.
- Same-run presentation pages: `target/phase5-correlated-presentation-page-001.json` and `target/phase5-correlated-presentation-page-002.json`.
- Same-run target visual: `target/phase5-correlated-view-1.png`.
- The presentation-classification child, backend descendant, and isolated Beryl home were stopped, verified, and reclaimed.
- Frozen operator capture: `C:\Users\user\.beryl\diagnostic-archives\activity-stale-running-rows-20260810-083923`.
- `activity.previous.jsonl`: 10,482,955 bytes, SHA-256 `0D28AB3915189FDE66E983C4255A7B98CFB1B16EB505538034EC40E2F637522C`.
- `activity.jsonl`: 2,738,887 bytes, SHA-256 `2932A7CFCA3693E1D1004C82B06162A82EE96F8ED44051CDCB376BF00C0FF472`.
- Bounded redacted analysis: `analysis-summary.json`, 21,158 bytes, SHA-256 `DC7C23796D0DE896E49F6B6B399BE28A6667AD8247DC818BDCF5AFD0414A86B4`; `analysis-anomalies.json`, 73,660 bytes, SHA-256 `443E48EFA32C114104042CBE0ECEEC6F318DB2C1152887863301708A2EC10321`.
