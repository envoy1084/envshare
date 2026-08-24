# Security policy

## Supported versions

Security fixes are provided for the latest published release. Upgrade to the
latest version before reporting a vulnerability that may already be resolved.

## Reporting a vulnerability

Do not open a public issue for a suspected vulnerability. Use GitHub private
vulnerability reporting for this repository. If private reporting is not
available, contact the maintainers privately through the address published in
the repository security settings.

Include the affected revision, reproduction steps, impact, and any suggested
mitigation. Do not include real environment files or credentials. We will
acknowledge a report, coordinate remediation and disclosure, and credit the
reporter when requested.

## Security expectations

Envshare's intended properties and limitations are documented in
[`docs/guides/security.md`](docs/guides/security.md). A share code is a bearer
capability: possession is authorization. A compromised sender or receiver is
outside the protocol's protection.
