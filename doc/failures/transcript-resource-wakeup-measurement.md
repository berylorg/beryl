# Transcript Resource Wakeup Measurement

A native replacement fixture observed every shell notification, emitted a predecessor resource
event, then waited on a timer before asserting zero notifications. Run
`1296158b-aeb4-4dab-96a1-82f3f12fc378` reached provider-backed Code loading and scrolling but
failed this assertion with one notification. A shell notification observed across a timed wait
does not identify its originating event; ordinary background work can also notify the shell.

Emit each test resource event inside `AsyncApp::update`, which flushes its effects before returning,
and inspect the observer count without an intervening await. Require zero notifications from the
predecessor and exactly one from the current panel. The corrected native replacement case passed
in run `d6890f2f-b5da-482c-80e0-757112678d47` in 9.311 seconds.

This measures subscription routing. Actual bounded range delivery and native inner scrolling
remain separate assertions in the same fixture; shell notification counts alone do not qualify
provider reads or rendered Code.
