# Release and rollback

Envshare releases the `envshare` client for the five targets in the
[installation guide](installation.md) and `envshare-node` for the two glibc Linux
targets. All project source and release metadata use Apache-2.0 only.

## Release preparation

1. Start from a reviewed, clean `main` commit with all required CI checks green.
2. Update the workspace version, both first-party installer defaults, `Cargo.lock`,
   and `CHANGELOG.md` in one release-preparation change.
3. Run the full repository gates described in [contributing](../CONTRIBUTING.md).
4. Install the exact dist version recorded in `Cargo.toml`, then run:

   ```console
   scripts/release-check.sh
   ```

   During development only, `--allow-dirty` permits inspecting a plan before the
   release-preparation commit. It never permits publishing from a dirty tree.
5. Review the dry-run manifest. It must contain the client archives, Linux node
   archives, SHA-256 files, CycloneDX SBOMs, shell and PowerShell installers,
   Homebrew formula, source archive, and no node archives for macOS or Windows.
6. Create one annotated, signed tag matching the workspace version exactly, for
   example `v0.1.0-alpha.1`, and push only that tag after approval.

The generated release workflow refuses partial publication: all target builds
must succeed before its host job creates a release. It embeds auditable dependency
metadata, publishes SHA-256 checksums and SBOMs, copies the first-party installers
under stable names, signs every uploaded artifact with GitHub's keyless Sigstore
attestation, and derives release notes from `CHANGELOG.md`. GitHub Actions are
pinned to commit SHAs. Enable GitHub's immutable-release repository setting before
the public beta; the workflow does not attempt to change repository settings.

## CI and clean-machine evidence

Pull requests execute the dist plan through `.github/workflows/release.yml`.
The main CI workflow intentionally runs only a fast subset: workspace formatting,
linting and documentation; capability, cryptography, and protocol tests on Linux,
macOS, and Windows; dependency policy; and installer smoke tests on fresh Linux
and Windows runners. Exhaustive tests, fuzzing, coverage, network emulation, and
load or soak runs are release gates rather than duplicated CI work. The installer
harness installs into a new temporary directory, executes the binary, proves an
existing installation is not silently overwritten, corrupts the fixture checksum,
and proves failed verification leaves the installed binary unchanged.

The harness intercepts only the network fetch and supplies a locally built archive
with cargo-dist's exact layout. A release candidate must also run the published
HTTPS installers and verify GitHub attestations on clean systems after the draft
artifacts exist. Record those URLs, digests, runner images, and results in the
release issue; local fixture tests are not evidence that GitHub hosting works.

## Post-release verification

Before announcing a release:

1. Download every asset and compare it with its `.sha256` or `sha256.sum` entry.
2. Run `gh attestation verify` for every archive, installer, formula, SBOM, and
   checksum, constrained to this repository and the release workflow.
3. Install through `install.sh` on both glibc architectures and both macOS
   architectures, through `install.ps1` on x64 Windows, and through the generated
   Homebrew formula on a clean supported macOS system.
4. Run `envshare --version`, `envshare --help`, and a direct/TCP/relay acceptance
   transfer on the installed binaries. Exercise `envshare-node config check`,
   liveness, readiness, and graceful shutdown on each Linux node archive.
5. Confirm release notes still identify the alpha security boundary and do not
   describe the project as production-ready for secrets before independent review.

## Rollback and withdrawal

Release artifacts are immutable. Never replace bytes under an existing tag or
reuse a version after a failed release.

For an ordinary regression:

1. Stop announcements and mark the affected version clearly in its release notes.
2. Reinstall the last accepted version with its versioned installer and `--force`
   on Unix or `-Force` on Windows. Homebrew users should uninstall the affected
   formula and install the previous release's pinned `envshare.rb`.
3. Roll Linux nodes back one at a time, retaining the current identity and a
   compatible configuration. Verify readiness before moving to the next node.
4. Publish a new patch version with a changelog entry; do not edit the affected
   assets in place.

For a malicious, credential-compromised, or secret-disclosing release, follow the
[security policy](../SECURITY.md) and incident runbook immediately. Remove it from
the recommended installation path, revoke compromised credentials, rotate node
identity only if that identity was exposed, preserve forensic evidence, publish a
security advisory, and delete the release/attestations only when the response lead
decides withdrawal is safer than retaining verification evidence. Users must be
told the exact affected versions and artifact digests.
