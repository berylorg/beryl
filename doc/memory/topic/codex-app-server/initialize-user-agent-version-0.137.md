# Reason For Investigation

The CAS 0.137 migration needed to identify which part of the `initialize.userAgent` string represents the app-server version and which part represents the GUI client identity.

# Outcome

Useful. In CAS 0.137, the slash version in the first product token is the app-server version. When a probe sent `clientInfo.name = "probeclient"` and `clientInfo.version = "9.8.7"`, app-server returned a `userAgent` beginning with `probeclient/0.137.0` and later echoed the client identity as `(probeclient; 9.8.7)`.

For Beryl's CAS 0.137 client identity, the version gate should parse the app-server version from the first product token shaped as `beryl/<app-server-version>`. Beryl's `clientInfo.version` is the GUI client version echoed later in the string and is not the app-server version.

The observed 0.137 initialize response includes `userAgent`, `codexHome`, `platformFamily`, and `platformOs`. It does not include an explicit protocol version or capability matrix, so Beryl combines initialize parsing with targeted required-method probes.

# Sources

- Legacy note: `doc/app-server-contract.md`, migrated on 2026-06-11.
- Local codex-cli 0.137.0 initialize userAgent probe, performed 2026-06-09.
- Failure note recording the invalid older parsing assumption: `doc/failures/cas-version-gate.md`.

# Probe Detail

The probe sent `clientInfo.name = "probeclient"` and `clientInfo.version = "9.8.7"`. The observed userAgent shape was:

```text
probeclient/0.137.0 (...) WindowsTerminal (probeclient; 9.8.7)
```

The same rule applies to Beryl's managed client identity, where the first product token is `beryl/<app-server-version>`.
