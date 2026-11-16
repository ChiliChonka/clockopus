# clockopus Helm chart

Installs [clockopus](https://github.com/ChiliChonka/clockopus), a personal time tracker, with
sign-in via OpenID Connect and PostgreSQL.

```sh
helm install clockopus oci://ghcr.io/chilichonka/charts/clockopus --version 0.1.0 -f values.yaml
```

## Minimal values

With CloudNativePG (operator installed) and an OIDC provider:

```yaml
auth:
  issuer: https://auth.example.com/realms/family   # OpenID Connect issuer
  clientId: clockopus                                # public client, Authorization Code + PKCE
cnpg:
  enabled: true
  image: ghcr.io/cloudnative-pg/postgresql:17.11-standard-trixie
httpRoute:
  enabled: true
  parentRef: {name: my-gateway, namespace: gateway-system, sectionName: https}
  hostnames: [clockopus.example.com]
```

With an existing PostgreSQL instead: `storage.postgres.existingSecret` names a Secret whose key
`uri` (`storage.postgres.uriKey`) holds the connection URI. The database user must own the
database: the first migration creates the trusted extension `btree_gist`.

## The OIDC client

A **public** client (no secret) with the Standard flow, PKCE `S256`, redirect URI
`https://<host>/*`, web origin `https://<host>` and post-logout redirect `https://<host>/*`.
Access tokens must carry the client in `azp` (Keycloak does) or in `aud`
(`auth.audience`, default: `clientId`). The scopes `openid profile email` are requested.

## What the chart refuses

`helm template`/`install` fails, with a message, for combinations that would not work or would be
unsafe:

- `auth.mode=oidc` without `auth.issuer` or `auth.clientId`
- `auth.mode=none` together with `httpRoute.enabled` (everyone reaching the route would be the
  same user)
- `storage.type=yaml` with more than one replica
- `storage.type=postgres` without `cnpg.enabled` or `storage.postgres.existingSecret`
- an HTTPRoute parent without `sectionName` (it would bypass the HTTPS redirect)

## Values

| Key | Default | Meaning |
|---|---|---|
| `image.repository` / `image.tag` | `ghcr.io/chilichonka/clockopus` / appVersion | Image |
| `replicaCount` | `1` | More than one with PostgreSQL only; a PodDisruptionBudget is added then |
| `auth.mode` | `oidc` | `oidc` or `none` |
| `auth.issuer`, `auth.clientId`, `auth.audience` | – | OIDC issuer, public client, accepted `aud`/`azp` |
| `storage.type` | `postgres` | `postgres` or `yaml` (files on a PVC, kept on uninstall) |
| `storage.postgres.existingSecret`, `uriKey` | CNPG app secret, `uri` | Database connection |
| `cnpg.*` | disabled | CloudNativePG cluster; `cnpg.backup.*` adds Barman Cloud backups to S3 |
| `httpRoute.*` | disabled | Gateway API route; `timeout` default `30s` |
| `networkPolicy.*` | disabled | Ingress only from the gateway namespace; database only from the app and the operator |
| `logLevel` | `info` | `RUST_LOG` |
| `resources`, `podSecurityContext`, `securityContext` | small, non-root, read-only root fs | Pod settings |

The pod runs as UID 65532 without capabilities, with a read-only root filesystem and without a
service account token. Probes: `/healthz` (liveness, startup) and `/readyz` (readiness; not ready
while the database is unreachable).
