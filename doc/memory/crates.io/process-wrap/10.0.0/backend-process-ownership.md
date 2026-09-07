# Reason For Investigation

Audit BR-009 evaluated whether a maintained process-group/job dependency could replace Beryl's
backend process ownership, bounded teardown and Windows-hosted WSL cleanup.

# Outcome

Retain the current implementation. process-wrap 10.0.0's standard-library JobObject wrapper calls
private make_job_object(handle, false), so its kernel job does not provide Beryl's kill-on-job-close
ownership. JobObject is a unit struct, the child job port is private, and Windows helpers are private.
The standard wrapper exports no KillOnDrop configuration; the Tokio side does. A custom std wrapper
could explicitly kill on Drop, but cannot configure the library-owned job handle through the public
API. Explicit Drop code does not establish kernel cleanup when the owner process exits.

A fork or replacement job wrapper is possible. Beryl's directly replaceable HostProcessTree is
only approximately 110 lines, however, leaving perhaps 40-70 net lines after preserving error and
adapter behavior. Fork maintenance would be added. Bounded grace/kill waits and remote WSL process
group cleanup remain Beryl responsibilities; the library's local Unix groups do not replace the
Windows-to-WSL boundary. No adoption or net deletion is recommended.

The exact version was released 2026-08-24, with first release in March 2024 and roughly 12.8 million
crate downloads at the 2026-09-06 registry check. It is the maintained successor to command-group.
License is Apache-2.0 OR MIT and MSRV 1.87. A potential synchronous configuration would disable
defaults and enable std, job-object and creation-flags; no Tokio conversion is proposed.
command-group 5.0.1 was also considered at registry/successor level: released 2023-11-18, MSRV 1.68,
same dual license, optional with-tokio, approximately 7.2 million downloads. No separate adoption
assessment or advisory clearance was performed for it. Neither package was installed or changed;
no runtime test or resolved dependency-closure security audit was performed.

# Sources

- [process-wrap registry metadata](https://crates.io/api/v1/crates/process-wrap), checked 2026-09-06.
- [Tagged std JobObject implementation](https://raw.githubusercontent.com/watchexec/process-wrap/v10.0.0/src/std/job_object.rs),
  [Windows helpers](https://raw.githubusercontent.com/watchexec/process-wrap/v10.0.0/src/windows.rs),
  and [std exports](https://raw.githubusercontent.com/watchexec/process-wrap/v10.0.0/src/std.rs).
- [Versioned API and successor description](https://docs.rs/process-wrap/10.0.0/process_wrap/).
- [command-group registry metadata](https://crates.io/api/v1/crates/command-group).
- Beryl baseline e6172f7c49bebef78d51831e621e70c8ac0d6a07: managed_process.rs and command.rs,
  fully reviewed with lifecycle and cleanup tests. See
  [BR-009](../../../../audits/code-simplification/reviews/backend-rest-findings.json).
