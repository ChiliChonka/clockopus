# ADR-0001: One container, two storage backends, two auth modes

- **Status:** proposed
- **Date:** 2026-11-16

## Context

clockopus is a personal time tracker (see [SPEC.md](../SPEC.md)). It must:

1. be easy to try: `docker compose up` without a database or an identity provider, with data in
   human-readable files;
2. run properly in Kubernetes with single sign-on (OpenID Connect) and PostgreSQL;
3. be published as open source (GitHub, container image and Helm chart via OCI);
4. use Angular, Rust and PostgreSQL.

## Decision

### One image, one process

A single Rust binary (Axum) serves the REST API under `/api/v1` and the built Angular app as static
files (with SPA fallback to `index.html`). One image `ghcr.io/chilichonka/clockopus`, one Helm
chart `oci://ghcr.io/chilichonka/charts/clockopus`, both versioned together from this repository.

The frontend reads its runtime configuration (auth mode, issuer, client ID) from
`GET /api/v1/config`, so the same image works for every installation; nothing is baked in at build
time.

### Storage behind one trait, two implementations

The domain logic (one running timer, no overlaps, forgotten-timer detection, reports) is written
once against a `Store` trait. Two implementations:

| Backend | Use | Notes |
|---|---|---|
| `yaml` | trying it out, single small installations | Files per user: `tasks.yaml` (tasks with their time entries and history), `tags.yaml`, `settings.yaml`. Loaded into memory at startup, every change written atomically (temporary file, `fsync`, rename). Exactly **one** replica. |
| `postgres` | real installations | sqlx with migrations; invariants also enforced by the database (partial unique index for “one running entry”, exclusion constraint against overlaps). Any number of replicas. |

Both backends run the same contract test suite, so they cannot drift apart.
Export and import (YAML) move data between them.

### Auth: `oidc` or `none`

- `oidc`: the Angular app signs in with Authorization Code + PKCE as a public client. The backend
  validates every access token itself (signature against the issuer's JWKS, `iss`, `exp`, and the
  client via `aud` or `azp`) and identifies the user by `(issuer, sub)`. The first valid request
  creates the account. No provider-specific code: any compliant OIDC provider works.
- `none`: one built-in user `local`. Must be set explicitly (`CLOCKOPUS_AUTH=none`); the server
  logs a warning at startup. Meant for local trials only.

Auth mode and storage backend are independent; all four combinations are allowed.

### Time

Timestamps are stored as instants (UTC in PostgreSQL, RFC 3339 with the user's offset in YAML).
Everything that depends on “a day” (reports, forgotten timer, week start) is computed in the
user's IANA time zone, so days with a daylight-saving switch have 23 or 25 hours.

## Alternatives considered

- **Separate frontend and backend images (nginx + API):** more moving parts for compose and the
  chart, no benefit at this size.
- **Backend-for-frontend with session cookies instead of bearer tokens:** keeps tokens out of the
  browser, but needs server-side sessions (sticky or shared) and a confidential client. Possible
  later without changing the API.
- **SQLite for the simple mode:** robust, but the files are not readable without tools, which was
  an explicit requirement.
- **Event log instead of state plus history:** complete audit trail, but hard to read and analyse
  by hand. The per-entry history covers the actual need (“what did I correct?”).

## Consequences

- The YAML backend limits the chart to one replica with a persistent volume; the chart refuses
  other combinations.
- Every rule must work in both backends; the shared contract tests are mandatory.
- Provider-specific setup (e.g. creating a Keycloak client) lives outside the app and its chart.
