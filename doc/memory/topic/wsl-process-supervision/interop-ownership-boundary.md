# Reason For Investigation

The Operator approved a root Linux supervisor/bootstrap while CAS keeps its normal account.
The remaining question is whether a private PID namespace and Windows job satisfy Beryl's
existing managed process-tree disposal contract while preserving WSL interoperability.
This is a distinct ownership investigation, not acceptance of a narrower contract.

# Outcome

Private PID namespace closure and a Windows job each prove only their exact membership.
Their combination does not establish ownership of all work initiated through shared WSL
services. The namespace candidate remains useful for Linux descendants, but implementation
is blocked on the supported lifecycle envelope. The Operator's privilege approval does
not itself approve excluding cross-OS/service-created work from retirement.

Microsoft WSL source selects an interop server using the environment socket or a parent
search. The WSL service can launch a distribution-level interop host. Windows interop
channel loss terminates the root non-GUI process; GUI applications are exempt, and the
root termination is not a complete descendant-tree proof. Thus neither inherited job
membership nor channel loss should be assumed to cover every enabled interop route.

A concrete boundary example is a Linux descendant invoking Windows `wsl.exe`. Its
request can start another Linux process through the WSL service outside the original
PID namespace. Even a job containing the Windows wrapper does not place that new Linux
process in the original namespace. This is an inference from the launch/ownership
boundaries, not a native demonstration that a particular workload survived shutdown.

Existing Beryl source applies `HostProcessTree` only to Host launches. Putting the WSL
wrapper in a Windows job would require correct launch ordering and qualification, and
would still not prove disposal of work created by another service. Numeric enumeration
of the distribution or stopping the entire distribution is not original-owner cleanup.

The bounded proposal is to define managed ownership as CAS and Linux descendants within
the original namespace, together with explicitly owned Windows launcher/job members.
Windows programs and new Linux processes created through services outside those domains
would remain external effects and may outlive retirement. Interoperability stays enabled.
This would narrow the current broad guarantee, so it remains a proposal for Operator
selection in `doc/plan.md`; owning authority must change before implementation if selected.

Otherwise, an additional cross-OS original-owner mechanism must be qualified. No such
mechanism has been established here. No claim that all possible mechanisms are impossible
is made. The analogous distinction between descendants and service-created work can
also arise for Host execution; this investigation does not redefine Host lifecycle.

Independent read-only ownership investigation and root verification on 2026-10-07
confirmed these limits. No process, native interop probe, build or software installation
was run for this investigation. Privilege, artifact and native qualification requirements
from the [namespace investigation](namespace-supervisor-feasibility.md) still apply.

# Sources

- Microsoft WSL, canonical repository `https://github.com/microsoft/WSL`, requested
  release tag `2.6.1`, resolved release commit
  `642331364dda7a3d88bf64acb87e7056918a5fc9`; accessed 2026-10-07.
  Inspected [src/linux/init/util.cpp](https://github.com/microsoft/WSL/blob/642331364dda7a3d88bf64acb87e7056918a5fc9/src/linux/init/util.cpp),
  `UtilConnectToInteropServer`: explicit environment socket and parent search.
- Same repository/commit/access date,
  [src/windows/service/exe/WslCoreInstance.cpp](https://github.com/microsoft/WSL/blob/642331364dda7a3d88bf64acb87e7056918a5fc9/src/windows/service/exe/WslCoreInstance.cpp),
  `WslCoreInstance::Initialize`: service-level `LaunchInteropServer` with user token.
- Same repository/commit/access date,
  [src/windows/common/interop.cpp](https://github.com/microsoft/WSL/blob/642331364dda7a3d88bf64acb87e7056918a5fc9/src/windows/common/interop.cpp),
  `ProcessInteropMessages`, `CreateProcessVmMode` and `WorkerThread`: process launch,
  channel-loss root termination and GUI exemption.
- Microsoft Learn, [Job Objects](https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects),
  accessed 2026-10-07: job membership, normal child inheritance and explicit breakaway
  limits. This is API evidence, not qualification of WSL job inheritance.
- Beryl local source, baseline `5e911c53`, inspected using Serena:
  `crates/beryl-backend/src/server.rs`, `ManagedBackendServer::launch`, and
  `crates/beryl-backend/src/managed_process.rs`, `HostProcessTree::create_for_child`.
  Host-only job construction and post-spawn assignment are existing behavior;
  no source implementation was changed.
