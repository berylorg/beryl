# Default Startup Memory Investigation

## Invalid Diagnostic Route: GUI-Driven Activity Stress Without Observable Turn State

Phase 6 tried to measure live-session retained activity growth by launching a copied-home sacrificial Beryl process and driving a real read-only turn through the GUI.

The copied Beryl home and copied `workspace.redb` were isolated from the active Beryl session, so the database boundary was safe. The diagnostic route still failed because the sacrificial GUI entered a turn-error state that was not exposed clearly enough to automation or logs for a reliable retained-memory measurement.

Course adjustment: do not rely on GUI-driven activity stress as the next memory diagnostic path. Add either stronger turn-state observability suitable for automation, or a separate planned diagnostic/instrumentation path that can synthesize or measure activity retention inside the GUI process without depending on manual UI state interpretation.
