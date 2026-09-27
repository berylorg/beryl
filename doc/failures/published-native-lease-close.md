# Published Native Lease And Default Close

The first published-window lease implementation reused GPUI's deferred explicit-removal path,
but an allowed `WM_CLOSE` still returned to the Windows default handler. That handler destroyed
the native window while the observation worker retained its lease. Protecting GPUI wrapper drop
alone therefore did not protect the original HWND through an ordinary admitted close.

The real native operation fixture exposed this in nextest run
`c49f326a-3914-4a16-906e-88e7873dd8ff`: explicit removal, worker unwind and observer abandonment
passed, but the allowed-close case failed its worker-held lifetime assertion. The close callback
had correctly allowed the request; lifetime deferral was the missing boundary.

The correction preserves close callback admission, then records deferred destruction when a
native operation remains active and suppresses default close handling. The existing GUI lease
settlement performs destruction after worker release. Hidden-startup close intent and native
confirmation retain their separate earlier checks. The regression verifies both veto and allowed
close, exact GUI-thread destruction, and the absence of disposal while the worker holds the lease.

Authority: [published native observation lifetime](../../crates/beryl-app/doc/design-shell-lifecycle.md#published-native-observation-lifetime).
Desktop COM observation and Exit-owner draining remain separate integration obligations.
