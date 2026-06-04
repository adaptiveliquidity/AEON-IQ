# Security Policy

## Supported Versions

Security fixes are focused on the current `main` branch and the latest tagged or documented release.

## Management API Access

The management API must be protected in production with a non-empty `MANAGEMENT_API_KEY`. Requests should send that key with `X-Management-Key: <key>` or `Authorization: Bearer <key>`.

`ALLOW_UNAUTH_MANAGEMENT=true` is for local development only. Production deployments should set `MANAGEMENT_API_KEY` and keep `ALLOW_UNAUTH_MANAGEMENT=false`.

Do not commit real API keys, database passwords, `.env` files, benchmark artifacts, or generated local state.

## Reporting Vulnerabilities

Please report vulnerabilities privately through GitHub private vulnerability reporting or by contacting the project maintainers directly before public disclosure. Include the affected version or commit, reproduction steps, expected impact, and any suggested mitigation.

The maintainers will acknowledge valid reports, investigate scope, and coordinate a fix or disclosure timeline.
