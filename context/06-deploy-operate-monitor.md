# any-watch deployment and operations plan

## Status

This document describes the target operational model. The current `ani-web`
homelab deployment is legacy and remains in service during migration. Its
current commands live with the legacy deployment assets and must not be applied
to any-watch until its Compose files exist.

## Target VM layout

```text
/srv/any-watch/
  app/        deployment checkout or immutable release definition
  config/     owner-readable environment files
  data/       PostgreSQL and Valkey persistent volumes
  backups/    encrypted local staging for off-host copies
  state/      deployment reports, migration reports, and restore evidence
```

## Target Compose topology

| Service | Exposure | Responsibility |
| --- | --- | --- |
| caddy | Public 80/443/UDP 443 | TLS, proxying, security headers, access logs |
| app | Private | Go API, embedded Nuxt PWA, bounded workers |
| postgres | Private | Durable application data |
| valkey | Private | Expiring sessions, rate limits, cache/job coordination |

All services have pinned image versions, non-root execution where supported,
health checks, resource/PID limits, restart policy, and bounded Docker logs.
Only Caddy binds host ports.

## Deploy flow

1. Build and scan an immutable image in CI.
2. Publish a digest and deployment manifest.
3. Validate Compose with placeholder secrets on staging.
4. Run migrations as an explicit, logged step.
5. Verify readiness, account/API smoke tests, and provider-independent library
   routes.
6. Deploy side by side on a temporary hostname before any Caddy switch.
7. Record image digest, schema version, migration report, and rollback target.

The VM must not build an unreviewed production image from a detached worktree.

## Backup and restore

- Use PostgreSQL custom-format `pg_dump` backups plus globals where required.
- Encrypt and copy backups off the VM on a schedule.
- Verify checksums, retention, and restore at least quarterly.
- Test migrations with a backup restore before production cutover.
- Treat Valkey as reconstructable; back up configuration but do not depend on it
  for durable account state.

## Readiness and monitoring

Readiness checks PostgreSQL connectivity, Valkey connectivity, migration state,
and writable storage. It does not require a provider to be healthy. Monitor
request latency, login failures, provider health, cache hit rate, active media
grants, bytes relayed, PostgreSQL connections, Valkey memory, disk free space,
backup success, TLS expiry, container restarts, and Caddy 5xx rates.

## Rollback

Application rollback returns Caddy to the prior immutable image. Database
migrations are forward-only unless an explicitly tested reverse migration exists;
the primary rollback for incompatible data changes is restoration of the verified
pre-cutover backup to the old deployment. Never use destructive Git or database
commands as a deployment shortcut.
