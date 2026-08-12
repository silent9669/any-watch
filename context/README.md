# any-watch context

This folder separates the observed legacy `ani-web` system from the planned
`any-watch` replacement. It is a planning contract, not an authorization to
modify production.

## Read order

1. [01-product-brief.md](01-product-brief.md) - product decisions and invariants.
2. [02-current-system.md](02-current-system.md) - legacy inventory to preserve.
3. [03-target-architecture.md](03-target-architecture.md) - approved replacement.
4. [04-api-data-provider-contracts.md](04-api-data-provider-contracts.md) - compatibility and provider rules.
5. [05-network-security.md](05-network-security.md) - security and deployment controls.
6. [07-migration-acceptance.md](07-migration-acceptance.md) - acceptance gates.
7. [08-capacity-cost-evolution.md](08-capacity-cost-evolution.md) - limits and growth plan.
8. [09-build-handoff.md](09-build-handoff.md) - implementation evidence.

## Source of truth

When documents disagree, use this order:

1. Executable code and configuration for current `ani-web` behavior.
2. `docs/ANY_WATCH_MIGRATION.md`, `docs/ARCHITECTURE.md`, and `design.md` for
   the approved target state.
3. This context pack for migration constraints and acceptance criteria.
4. Historical release notes and legacy desktop documentation.

## Guardrails

- Preserve account isolation, source-scoped IDs, progress, My List, and stable
  safe errors during migration.
- Do not silently merge provider results or silently change a user's source.
- Do not expose credentials, cookies, required headers, signed URLs, or raw
  provider protocol data to browsers or logs.
- Keep enabled providers independently certified by language and region.
- Do not overwrite current production data, deployment assets, or the legacy
  application until side-by-side cutover approval.
- Do not commit secrets, databases, certificates, downloads, or backups.
