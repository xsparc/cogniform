# Security Policy

## Supported versions

Cogniform has no supported release yet. The current repository is an early
foundation and must not be used as a production security boundary. This file
will name supported release lines before the first public release.

## Reporting a vulnerability

Do not disclose suspected vulnerabilities in a public issue, discussion, pull
request, or chat. Use the repository's **Security > Advisories > Report a
vulnerability** control. If GitHub does not show that control, use private
contact information published on the repository owner's GitHub profile and
include only enough detail to establish a secure follow-up channel.

Include affected revisions, impact, reproduction conditions, and any proposed
mitigation. Do not include real credentials, private data, or destructive proof
of concept material. The maintainers will coordinate disclosure after the issue
has been reproduced and a remediation path is understood. This volunteer OSS
project does not currently promise a response SLA or bug bounty.

## Public repository hygiene

GitHub secret scanning and push protection are enabled for this public
repository. Private vulnerability reporting is enabled so security reports do
not need to begin in a public issue. These hosted controls are supplemented by
the checked-in `scripts/check_public_repo.py` path and content rules, which run
inside the existing pull-request quality job without another runner, uploaded
findings, or secret-bearing log output.

The repository check covers common credential files, private-key markers,
provider-token shapes, suspicious credential assignments, credential-bearing
URLs, private endpoints, personal home paths, and unapproved hidden root state.
Vendored crates are excluded from the local content-pattern pass because their
checksummed upstream source is reviewed through the dependency process;
GitHub's repository-level scanning still applies to the complete public
repository.

If a credential reaches a local commit or GitHub, treat it as compromised:
revoke or rotate it, avoid repeating it in logs or reports, and coordinate any
history remediation privately. A later deletion alone is not remediation.

## Security boundaries

Agent input, scene data, assets, labels, procedures, and transport messages are
untrusted. Future implementation must enforce limits before expensive decode,
allocation, ECS mutation, or GPU work. Arbitrary native plugins and user shaders
are outside the MVP security model.

The reviewed [MVP threat model](docs/threat-model/mvp.md) names assets, trust
boundaries, abuse cases, controls, residual risks, and assumptions for the
single-user local profile. The
[failure and recovery guide](docs/operations/failure-and-recovery.md) records
the tested containment behavior and operational gaps. Neither document turns
the current source workspace into a remote, multi-tenant, or production
security boundary.
