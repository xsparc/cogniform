# Governance

Cogniform currently uses a maintainer-led governance model appropriate for an
early-stage open-source project.

## Roles

- Contributors propose issues, designs, code, tests, documentation, and review.
- Reviewers provide evidence-based feedback but do not gain merge authority by
  reviewing a change.
- Maintainers define release scope, approve architecture and task transitions,
  merge pull requests, moderate community spaces, and administer the repository.

The repository owner is the initial maintainer. Additional maintainers may be
invited after sustained constructive contribution, sound judgment around the
project's safety boundaries, and demonstrated review and maintenance work.

## Decisions and changes

Routine changes are decided in their pull request. Consequential architecture,
compatibility, security-boundary, dependency, or governance decisions require
an issue or ADR with context, alternatives, consequences, and explicit
maintainer acceptance. Accepted records are superseded rather than silently
rewritten.

Pull requests are dependency ordered and must satisfy the documented task and
review gates. Maintainers may request a smaller change, reject scope that moves
Cogniform toward a general game engine, or defer features whose operational cost
is not justified by evidence.

## Transparency and conduct

Technical decisions, review findings, and release evidence should be public by
default. Security reports, conduct reports, credentials, private data, and
embargoed vulnerabilities are exceptions and follow [SECURITY.md](SECURITY.md)
and [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).

This project is maintained by volunteers and offers no response-time guarantee.
Changes to this governance model require a reviewed pull request and explicit
maintainer approval.
