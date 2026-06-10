# CAS Version Gate

## Initialize UserAgent Is Not A Stable CLI Version Source

During the CAS 0.137 single-contract migration, Phase 1 implemented the required-version gate by parsing a `codex-cli <major.minor.patch>` version from the app-server `initialize` response userAgent.

Live Phase 6 validation invalidated that assumption. A release diagnostic child launched from `target\release\beryl.exe` on June 8, 2026 started a managed backend, but Beryl marked the host-Windows runtime incompatible because the backend initialize response reported:

```text
beryl/0.137.0 (Windows 10.0.26200; aarch64) WindowsTerminal (beryl; 0.1.0)
```

In the same environment, `Get-Command codex` resolved to `C:\Users\user\apps\bin\codex.exe`, and `codex --version` reported:

```text
codex-cli 0.137.0
```

The failure is that CAS 0.137 initialize userAgent is client/environment shaped rather than `codex-cli` shaped. Treating initialize userAgent as exactly `codex-cli <version>` rejects a valid local CAS 0.137 runtime before transcript validation can run.

The course correction is to parse the app-server version from the first userAgent product token. A follow-up disposable stdio probe with `clientInfo.name = "probeclient"` and `clientInfo.version = "9.8.7"` returned:

```text
probeclient/0.137.0 (Windows 10.0.26200; aarch64) WindowsTerminal (probeclient; 9.8.7)
```

For Beryl's managed client identity, the required CAS 0.137 version gate should therefore parse `beryl/<major.minor.patch>` from the start of `initialize.userAgent`, reject the old `codex-cli <version>` shape, and keep the hardcoded CAS 0.137 protocol probes as the contract proof.
