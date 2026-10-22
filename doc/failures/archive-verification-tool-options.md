# Archive Verification Must Own Tool Options And Entry Types

Independent review on 2026-10-22 rejected two assumptions in a task-scoped archive verifier before
the verifier ran. No source was removed and no archive had been published.

The first implementation read `unzip -Z -1` names and inferred regular-file/directory type from a
trailing slash. That does not validate the actual central-directory attributes. The correction
validates creator, Unix/DOS type and attribute facts, rejects symlink/special/reparse entries and
type/path disagreement, and requires the typed central map to agree with the tool listing before
integrity testing and extraction. Complete extracted path/type/length/SHA-256 parity remains required.

The child command also inherited Info-ZIP option environment variables. In particular an inherited
`ZIPOPT=-m` could turn archive creation into source movement before verification and bypass the
required exact-directory cleanup path. The correction removes `ZIPOPT`, `ZIP`, `UNZIP`, `UNZIPOPT`,
`ZIPINFO` and `ZIPINFOOPT` from those exact archive child commands while preserving unrelated
runtime environment. Focused tests inspect command-local removal without mutating global state.

Archive preparation and source removal remain separate explicit operations. Source manifests must
still match immediately before exact `cleanup-dir.exe` removal, after verified non-replacing
publication and published hash/integrity checks. Unsupported archive forms refuse with source
preserved; a partial transition stops for examination rather than automatically retrying cleanup.
