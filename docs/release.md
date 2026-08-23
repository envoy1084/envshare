# Release and rollback

Envshare releases the `envshare` client for the five targets in the
[installation guide](installation.md). The node is distributed as a Linux
container image. All project source and release metadata use Apache-2.0 only.

## Release preparation

1. Start from a reviewed, clean `main` commit with all required CI checks green.
2. Update the workspace version, both first-party installer defaults, `Cargo.lock`,
   and `CHANGELOG.md` in one release-preparation change.
3. Run the [release quality gates](quality-gates.md); the manually dispatched
   smoke workflow is not release-candidate evidence.
4. Install the exact dist version recorded in `Cargo.toml`, then run:

   ```console
   scripts/release-check.sh
   ```

   During development only, `--allow-dirty` permits inspecting a plan before the
   release-preparation commit. It never permits publishing from a dirty tree.
5. Review the dry-run manifest. It must contain the client archives, SHA-256
   files, CycloneDX SBOM, shell and PowerShell installers, Homebrew formula, and
   source archive. It must not contain node artifacts.
6. When a release intentionally changes the node image, manually dispatch the
   container workflow after the tag exists. It publishes `linux/amd64` and
   `linux/arm64`, attaches SBOM/provenance, and keylessly signs the image index
   digest. Client-only tags must not publish a node image.
7. Manually run the CLI release workflow with a tag matching the client version,
   for example `v0.1.0`, after approval.

The generated release workflow refuses partial publication: all target builds
must succeed before its host job creates a release. It embeds auditable dependency
metadata, publishes SHA-256 checksums and SBOMs, copies the first-party installers
under stable names, signs every uploaded artifact with GitHub's keyless Sigstore
attestation, and derives release notes from `CHANGELOG.md`. GitHub Actions are
pinned to commit SHAs. Enable GitHub's immutable-release repository setting before
the public beta; the workflow does not attempt to change repository settings.

## CI and clean-machine evidence

All three GitHub workflows are manual-only. The CI workflow runs one Ubuntu job
containing formatting and the focused code, cryptography, and protocol tests.
Clippy, documentation, cross-platform checks, dependency policy, installer tests,
exhaustive tests, fuzzing, coverage, network emulation, and load or soak runs are
performed locally as release gates rather than duplicated in CI.

The harness intercepts only the network fetch and supplies a locally built archive
with cargo-dist's exact layout. A release candidate must also run the published
HTTPS installers and verify GitHub attestations on clean systems after the draft
artifacts exist. Record those URLs, digests, runner images, and results in the
release issue; local fixture tests are not evidence that GitHub hosting works.

After publication, perform the clean-machine installer and transfer checks in the
post-release checklist manually and record the evidence in the release issue.

## Post-release verification

Before announcing a release:

1. Download every asset and compare it with its `.sha256` or `sha256.sum` entry.
2. Run `gh attestation verify` for every archive, installer, formula, SBOM, and
   checksum, constrained to this repository and the release workflow.
3. Verify the container signature with the command in [deployment](deployment.md),
   inspect its SBOM/provenance, and record the immutable image index digest.
4. Install through `install.sh` on both glibc architectures and both macOS
   architectures, through `install.ps1` on x64 Windows, and through the generated
   Homebrew formula on a clean supported macOS system.
5. Run `envshare --version`, `envshare --help`, and a direct/TCP/relay acceptance
   transfer on the installed binaries. Exercise `envshare-node config check`,
   liveness, readiness, and graceful shutdown on each Linux node archive.
6. Confirm release notes still identify the unaudited 0.x security boundary and do not
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
