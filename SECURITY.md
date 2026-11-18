# Security Policy

Astera contracts may hold user funds. Please report vulnerabilities privately
so maintainers can investigate before details become public.

## Report a Vulnerability

Use [GitHub's private vulnerability reporting](https://github.com/astera-hq/Astera/security/advisories/new).
Do not open a public issue, discussion, or pull request with exploit details.
Include the affected component and version or commit, impact, reproduction
steps, and a way for maintainers to follow up. Do not include real private
keys, seed phrases, or other users' data.

If the private advisory form is unavailable, do not post exploit details in a
public channel. Open a non-sensitive issue asking maintainers for an alternate
private contact channel.

## Scope

Reports about contracts, deployment and upgrade tooling, the frontend,
indexer, services, SDK, and CI or supply-chain paths are in scope. Examples
include loss or freezing of assets, authorization bypasses, incorrect
accounting, and exposure of wallet or user secrets. Test against your own
accounts and local or testnet deployments only. Mainnet exploitation, social
engineering, denial-of-service testing against shared infrastructure, and
accessing third-party data are out of scope.

## Response and Disclosure

Maintainers aim to acknowledge a report within three business days and provide
a status update at least every seven days while investigating. These are
targets, not guarantees. Please allow time to confirm impact, prepare a fix,
and coordinate with affected deployers before public disclosure. The reporter
and maintainers should agree on a disclosure date; if coordination stalls,
please give at least 90 days' notice before publishing technical details.

## Safe Harbor

We welcome good-faith research that follows this policy, avoids harm and
privacy violations, and gives maintainers a reasonable opportunity to fix the
issue. For such research, the project will not seek legal action and will work
with the researcher to understand and remediate the finding. This statement
does not authorize activity against third-party systems or waive applicable
law. Researchers remain responsible for complying with local law and the
policies of infrastructure providers.
