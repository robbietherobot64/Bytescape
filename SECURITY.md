# Security Policy

## Supported versions

Bytescape is pre-release software. There are no versioned releases yet, so only
the latest `main` branch is supported — fixes land on `main` and there is nothing
older to backport to. If you run an instance, track `main`.

| Version | Supported |
|---|---|
| `main` | ✅ |
| older commits / snapshots | ❌ |

## Reporting a vulnerability

**Do not open a public issue, pull request, or public discussion for a
vulnerability until it has been fixed and disclosed.**

Report privately by email to **admin@bytescape.social** with:

- what the issue is and its impact (who could do what),
- a step-by-step reproduction or proof of concept,
- the affected component (server, web app, federation surface, …),
- any mitigation you already know.

You'll get an acknowledgement when a maintainer sees the report, and we'll keep
you updated as a fix is developed. Please give us reasonable time to fix before
any public disclosure.

## What not to report as a security issue

- Content and moderation complaints — use the report flow in the app, or
  contact the operator of the instance involved.
- Vulnerabilities in third-party services we depend on — report those to their
  operators (tell us too if our usage makes the impact worse).
- Behavior that requires an account owner to control their own account's data —
  that's the point of the product.

If you find a leaked credential, exposed private file, or user data where it
shouldn't be — in this repository, on a Bytescape instance, or in a public
artifact — that **is** a security report. Send it privately.