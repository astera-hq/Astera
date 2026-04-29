# Security Audit Tracking

Use this file to track the security review lifecycle for Astera before mainnet launch.

## Status Rules

- `Open` means the finding is acknowledged but not yet remediated.
- `In Progress` means a fix is being implemented and is under review.
- `Resolved` means the fix is merged and validated.
- `Accepted Risk` requires explicit sign-off from the maintainers and should be rare.

## Mainnet Gate

- The main branch must not advance to mainnet deployment while any `Critical` or `High` findings remain `Open` or `In Progress`.
- The release checklist should reference the latest audit report before deployment.
- Any new protocol change should be checked against this tracker before merging.

## Tracker Template

| Finding | Severity | Status | Owner | Notes |
| --- | --- | --- | --- | --- |
| <issue or finding reference> | Critical/High/Medium/Low | Open/In Progress/Resolved/Accepted Risk | <name> | <remediation notes> |

