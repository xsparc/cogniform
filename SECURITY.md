# Security Policy

## Supported versions

Cogniform has no supported release yet. The current repository is an early
foundation and must not be used as a production security boundary. This file
will name supported release lines before the first public release.

## Reporting a vulnerability

Do not disclose suspected vulnerabilities in a public issue, discussion, pull
request, or chat. Use GitHub's private vulnerability-reporting control on this
repository when it is available. If it is unavailable, use private contact
information published on the repository owner's GitHub profile and include only
enough detail to establish a secure follow-up channel.

Include affected revisions, impact, reproduction conditions, and any proposed
mitigation. Do not include real credentials, private data, or destructive proof
of concept material. The maintainers will coordinate disclosure after the issue
has been reproduced and a remediation path is understood. This volunteer OSS
project does not currently promise a response SLA or bug bounty.

## Security boundaries

Agent input, scene data, assets, labels, procedures, and transport messages are
untrusted. Future implementation must enforce limits before expensive decode,
allocation, ECS mutation, or GPU work. Arbitrary native plugins and user shaders
are outside the MVP security model.
