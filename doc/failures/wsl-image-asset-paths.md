# WSL image asset paths

- Scope: composer local-image submission from the Windows GUI to a WSL app-server runtime.
- Invalid approach: validating a Windows-hosted Beryl image asset through `\\wsl.localhost\<distro>\mnt\<drive>\...` before submitting the image.
- Evidence: `/mnt/c/...` is WSL's mounted view of the Windows `C:\...` drive. The Windows GUI process already owns and can validate the original `C:\...` asset path; routing that validation through WSL UNC is unnecessary and can reject or stall a submission before `turn/start`.
- Why it failed: the validation path and backend path are different contracts. The backend path must be readable by the WSL app-server, while the preflight read probe runs in the Windows GUI process and should use a Windows-readable path.
- Course correction: for WSL runtimes, map Windows drive assets to `/mnt/<drive>/...` for `localImage.path`, but validate the source asset through its original Windows path.
- Affected docs/tests: `doc/features/workspaces/design.md`, `doc/features/composer/design.md`, `doc/app-server-contract.md`, and `crates/beryl-app/tests/composer_image_delivery.rs`.
