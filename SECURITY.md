# Security policy

## Supported versions

Envshare is pre-release software. No version is currently supported for
production secret transfer. Supported release lines will be listed here before
the first public beta.

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
[`docs/threat-model.md`](docs/threat-model.md). A share code is a bearer
capability: possession is authorization. A compromised sender or receiver is
outside the protocol's protection.
