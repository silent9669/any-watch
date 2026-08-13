# any-watch delivery roadmap

`ANY_WATCH_MIGRATION.md` is the canonical migration plan. This file is the
working implementation sequence and replaces completed any-watch desktop tasks.

| Milestone | Deliverable | Exit evidence |
| --- | --- | --- |
| 1 | Nuxt, Go, PostgreSQL, Valkey, Compose foundation | Reproducible local stack and typed API contract |
| 2 | Login, roles, sessions, admin Settings | Account isolation and authentication tests |
| 3 | SQLite importer and canonical catalog | Dry-run report, row counts, rollback procedure |
| 4 | Provider framework and legacy adapter ports | Per-provider contract and certification reports |
| 5 | PWA screens, player, account library, TV controls | Browser, accessibility, and remote navigation tests |
| 6 | Observability, backups, staging, load test | 100-user browsing envelope and restore drill |
| 7 | Side-by-side cutover | Owner acceptance and reversible Caddy switch |

## Working rules

- Build new code beside legacy code until cutover approval.
- Port provider behavior only through the versioned provider contract.
- Do not claim a provider works until the certification suite passes.
- Do not change production DNS, Caddy routing, containers, or databases while
  this roadmap is documentation-only.
- Keep an evidence bundle for every milestone: commit SHA, test output, known
  limitations, migration report, and rollback consequence.
