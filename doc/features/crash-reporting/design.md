# Goals

Explain an internal Beryl panic without continuing to use the failed application's state.
Let the user copy useful diagnostic detail and dismiss the final report with minimal machinery.

## Non-goals

- Recovering or restarting the failed application, saving outstanding work, or undoing effects.
- Automatic uploads, report history, background collection, or a general crash-management service.
- Guaranteeing a report for faults that prevent the panic handler or reporter from running.

# Decisions

## Fatal Outcome

- An internal Rust panic is fatal to the application, including a panic that an ordinary worker
  would otherwise catch. The failed application terminates directly without ordinary save, flush,
  recovery, shutdown, or success acknowledgement.
- On Windows, an independent reporter remains available after the failed application terminates.
  It displays `Beryl stopped because of an internal error`, bounded panic detail, and exactly two
  commands: `Copy to clipboard` and `Exit Beryl`.
- `Exit Beryl` closes the remaining reporter. The failed application has already terminated.
  Closing the report window invokes the same exit outcome. Escape cannot dismiss the report while
  leaving any Beryl reporting window or application work running.
- There is no Continue, Retry, Restart, Settings, thread navigation, or normal application command.
  All ordinary application windows disappear with the failed process.
- Copy transfers only the displayed report's bounded diagnostic text to the clipboard. It neither
  writes a report file nor sends data elsewhere. A short inline result reports copy success or
  failure; copy failure leaves both commands available.
- The detail includes Beryl's version, panic source location when available, and a string panic
  payload when available. Truncation is explicit. No stack locals, environment, credentials,
  conversation snapshot, or storage inspection is collected. The report may contain sensitive text
  already present in the panic payload; exporting it requires the explicit Copy command.
- [`gui.md`](gui.md) is the normative report composition. The
  [crash-reporting system](../../systems/crash-reporting/design.md) owns process separation.

## Limits And Failure

- Reporting is best effort. An unavailable reporter, repeated panic, unsupported platform, or
  failure before the panic handler can run results in direct termination without a report.
  Reporter failure terminates that reporter and never starts another reporter.
- Ordinary returned errors retain their existing feature-specific handling. Fatal panic reporting
  takes precedence over running-store recovery and ordinary window-close or Exit behavior.
- No pending edit or external effect is claimed saved, cancelled, or rolled back by this screen.
  A later manually started Beryl validates durable state through its normal startup rules.
- The reporter does not appear after an ordinary application exit without a completed panic report.

# Engineering Rigor

Profile: `production-application/v2`

Modifiers:

- `sensitive-data/v1`

Verify separation from the failed application, the two-command surface, explicit truncation,
clipboard-only export, and silent normal exit. Loss of the best-effort report is permitted;
resuming failed application work or reporting invented durable outcomes is not.
