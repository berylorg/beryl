# Reason For Investigation

The root namespace supervisor must execute CAS under the same effective account and environment
as ordinary WSL `--exec`. Investigate whether a registry default UID and Linux passwd/initgroups
lookup reproduce that context without an ordinary launch.

# Outcome

The simple reconstruction is insufficient. WSLAPI returns the registration's default UID, while
Linux launch also considers an explicit username and the boot-loaded default user from wsl.conf.
Reading the current file cannot establish the cached setting of a running distribution.

WSL also creates a login session for the selected account, supplies DBUS_SESSION_BUS_ADDRESS and
XDG_RUNTIME_DIR, and sets account-derived HOME/USER/LOGNAME/SHELL and supplementary groups. Root
launch establishes root's context; replacing four variables does not prove ordinary session parity.
An explicit executable uses the direct execution path, not the default login shell/profile path.
Account-independent inherited WSL/WSLg/WSLENV/PATH facts should not be guessed from the API's
default environment array. Musl group initialization also has a WSL-specific fallback.

These findings informed the selected ordinary context broker, now specified in
[companion authority](../../../../crates/beryl-wsl-supervisor/doc/design.md) and
[backend orchestration](../../../../crates/beryl-backend/doc/design-wsl-supervision.md). Its actual
ordinary WSL launch provides effective credentials and environment; retained lifetime avoids
substituting root's session. This is a design correction, not qualification of FD/session/context
handling. Native tests must verify ordinary-account parity and broker lifetime. No account/session
probe, process, build or installation ran in this source investigation.

# Sources

Microsoft WSL, canonical repository `https://github.com/microsoft/WSL`, release tag `2.6.1`, resolved
commit `642331364dda7a3d88bf64acb87e7056918a5fc9`; accessed 2026-10-07. A read-only investigator
inspected these bounded paths:

- [src/windows/service/exe/LxssUserSession.cpp](https://github.com/microsoft/WSL/blob/642331364dda7a3d88bf64acb87e7056918a5fc9/src/windows/service/exe/LxssUserSession.cpp),
  `LxssUserSessionImpl::GetDistributionConfiguration`: registry-backed default UID.
- [src/linux/init/config.cpp](https://github.com/microsoft/WSL/blob/642331364dda7a3d88bf64acb87e7056918a5fc9/src/linux/init/config.cpp),
  `ConfigInitializeCommon`, `ConfigCreateEnvironmentBlock` and `CreateLoginSession`:
  cached default account configuration, inherited environment and systemd context.
- [src/linux/init/init.cpp](https://github.com/microsoft/WSL/blob/642331364dda7a3d88bf64acb87e7056918a5fc9/src/linux/init/init.cpp),
  `CreateProcessParseCommon` and `CreateProcessCommon`: effective account selection,
  account/group installation and direct executable selection.
- [src/linux/init/util.cpp](https://github.com/microsoft/WSL/blob/642331364dda7a3d88bf64acb87e7056918a5fc9/src/linux/init/util.cpp),
  `UtilInitGroups`: musl group-count handling.
- Microsoft Learn,
  [WslGetDistributionConfiguration](https://learn.microsoft.com/en-us/windows/win32/api/wslapi/nf-wslapi-wslgetdistributionconfiguration),
  accessed 2026-10-07: API default-UID and default-environment outputs; insufficient to prove
  the complete effective ordinary launch context.
